//! Incremental, fail-closed copy-to-new-name. Never collects a tree or deletes a
//! partial artifact. Callers must durably append intent/outcome records and treat
//! any returned error (including callback failure) as an incomplete operation.
use super::{capture_source, guarded_io};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    cell::Cell,
    collections::HashSet,
    fs::{self, File, OpenOptions, ReadDir},
    io::{Read, Write},
    mem::size_of,
    os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use windows_sys::Win32::{
    Foundation::{ERROR_HANDLE_EOF, INVALID_HANDLE_VALUE},
    Storage::FileSystem::*,
};

const MAX_DEPTH: usize = 256;
const BUFFER_SIZE: usize = 1024 * 1024;
const E_ABORT: i32 = 0x80004004_u32 as i32;
const E_FAIL: i32 = 0x80004005_u32 as i32;

struct Frame {
    entries: ReadDir,
    // Live handles pin both namespaces until all their children finish.
    _source: File,
    target: File,
    target_path: String,
    source_identity: OperationSource,
    basic: FILE_BASIC_INFO,
}

fn cancelled(cancel: &AtomicBool) -> Result<(), Fault> {
    if cancel.load(Ordering::Acquire) {
        Err(Fault::new(FaultCode::Cancelled))
    } else {
        Ok(())
    }
}
fn path_text(path: &Path) -> Result<&str, Fault> {
    path.to_str()
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))
}
fn outcome(
    source: &OperationSource,
    target: &str,
    status: ItemStatus,
    hresult: i32,
) -> ItemOutcome {
    ItemOutcome {
        source: source.path.clone(),
        destination: Some(target.to_owned()),
        status,
        hresult,
    }
}

/// Copies one root, streaming outcomes rather than retaining a result array.
/// Existing names are never overwritten. `skip_existing` skips an entire occupied
/// entry, including a directory subtree; it does not merge existing directories.
/// `Unknown` with a destination means a created artifact may be partial and must
/// be retained. Callback errors stop immediately, leaving the durable intent for
/// recovery. There is no tree-size limit; depth and live handles are bounded.
pub fn copy_root(
    source: &OperationSource,
    destination: &OperationSource,
    name: Option<&str>,
    skip_existing: bool,
    cancel: &AtomicBool,
    on_intent: impl FnMut(&str, &str) -> Result<(), Fault>,
    mut on_outcome: impl FnMut(ItemOutcome) -> Result<(), Fault>,
) -> Result<(), Fault> {
    let reported = Cell::new(false);
    let callback_failed = Cell::new(false);
    let mut on_intent = on_intent;
    let result = copy_root_inner(
        source,
        destination,
        name,
        skip_existing,
        cancel,
        |source, target| {
            let result = on_intent(source, target);
            if result.is_err() {
                callback_failed.set(true);
            }
            result
        },
        |item| {
            reported.set(true);
            let result = on_outcome(item);
            if result.is_err() {
                callback_failed.set(true);
            }
            result
        },
    );
    if result.is_err() && !reported.get() && !callback_failed.get() {
        on_outcome(ItemOutcome {
            source: source.path.clone(),
            destination: None,
            status: ItemStatus::Failed,
            hresult: E_FAIL,
        })?;
    }
    result
}

fn copy_root_inner(
    source: &OperationSource,
    destination: &OperationSource,
    name: Option<&str>,
    skip_existing: bool,
    cancel: &AtomicBool,
    mut on_intent: impl FnMut(&str, &str) -> Result<(), Fault>,
    mut on_outcome: impl FnMut(ItemOutcome) -> Result<(), Fault>,
) -> Result<(), Fault> {
    cancelled(cancel)?;
    super::identity::recheck(source)?;
    super::identity::recheck(destination)?;
    if destination.kind != EntryKind::Folder {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    let leaf = match name {
        Some(name) => name,
        None => Path::new(&source.path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
    };
    validate_leaf_name(leaf)?;
    let source_parent = Path::new(&source.path)
        .parent()
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    let pins = guarded_io::ancestors(
        &[source_parent, Path::new(&destination.path)],
        &HashSet::new(),
    )?;
    // Native identities also reject case aliases and destination-inside-source,
    // without trusting textual prefixes or mapped drive spellings.
    for pin in &pins {
        let info = guarded_io::info(pin)?;
        let id = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
        if info.dwVolumeSerialNumber == source.volume && id == source.file_index {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
    }
    let target_parent = guarded_io::open(destination, false)?;
    let mut buffer = vec![0; BUFFER_SIZE];
    let mut stack = Vec::new();
    if let Some(frame) = copy_entry(
        source,
        &target_parent,
        &destination.path,
        leaf,
        skip_existing,
        cancel,
        &mut buffer,
        &mut on_intent,
        &mut on_outcome,
    )? {
        stack.push(frame);
    }
    let traversal = (|| {
        while !stack.is_empty() {
            cancelled(cancel)?;
            let frame = stack.last_mut().expect("nonempty DFS stack");
            let Some(entry) = frame.entries.next() else {
                let completed = stack.last().expect("nonempty DFS stack");
                on_intent(&completed.source_identity.path, &completed.target_path)?;
                if let Err(error) = apply_basic(&completed.target, &completed.basic) {
                    on_outcome(outcome(
                        &completed.source_identity,
                        &completed.target_path,
                        ItemStatus::Unknown,
                        E_FAIL,
                    ))?;
                    return Err(error);
                }
                let completed = stack.pop().expect("nonempty DFS stack");
                on_outcome(outcome(
                    &completed.source_identity,
                    &completed.target_path,
                    ItemStatus::Changed,
                    0,
                ))?;
                continue;
            };
            let entry = entry.map_err(Fault::from)?;
            let path = entry.path();
            let child = match capture_source(path_text(&path)?) {
                Ok(child) => child,
                Err(error) => {
                    let target = Path::new(&frame.target_path).join(entry.file_name());
                    on_outcome(ItemOutcome {
                        source: path_text(&path)?.to_owned(),
                        destination: Some(path_text(&target)?.to_owned()),
                        status: ItemStatus::Failed,
                        hresult: E_FAIL,
                    })?;
                    return Err(error);
                }
            };
            if child.kind == EntryKind::Folder && stack.len() >= MAX_DEPTH {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            let name = entry.file_name();
            let leaf = name
                .to_str()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
            let frame = stack.last().expect("nonempty DFS stack");
            if let Some(child_frame) = copy_entry(
                &child,
                &frame.target,
                &frame.target_path,
                leaf,
                skip_existing,
                cancel,
                &mut buffer,
                &mut on_intent,
                &mut on_outcome,
            )? {
                stack.push(child_frame);
            }
        }
        Ok(())
    })();
    traversal?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_entry(
    source: &OperationSource,
    parent: &File,
    parent_path: &str,
    name: &str,
    skip_existing: bool,
    cancel: &AtomicBool,
    buffer: &mut [u8],
    on_intent: &mut impl FnMut(&str, &str) -> Result<(), Fault>,
    on_outcome: &mut impl FnMut(ItemOutcome) -> Result<(), Fault>,
) -> Result<Option<Frame>, Fault> {
    let reported = Cell::new(false);
    let callback_failed = Cell::new(false);
    let result = copy_entry_inner(
        source,
        parent,
        parent_path,
        name,
        skip_existing,
        cancel,
        buffer,
        &mut |source, target| {
            let result = on_intent(source, target);
            if result.is_err() {
                callback_failed.set(true);
            }
            result
        },
        &mut |item| {
            reported.set(true);
            let result = on_outcome(item);
            if result.is_err() {
                callback_failed.set(true);
            }
            result
        },
    );
    if result.is_err() && !reported.get() && !callback_failed.get() {
        let target = Path::new(parent_path).join(name);
        on_outcome(outcome(
            source,
            path_text(&target)?,
            ItemStatus::Failed,
            E_FAIL,
        ))?;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn copy_entry_inner(
    source: &OperationSource,
    parent: &File,
    parent_path: &str,
    name: &str,
    skip_existing: bool,
    cancel: &AtomicBool,
    buffer: &mut [u8],
    on_intent: &mut impl FnMut(&str, &str) -> Result<(), Fault>,
    on_outcome: &mut impl FnMut(ItemOutcome) -> Result<(), Fault>,
) -> Result<Option<Frame>, Fault> {
    cancelled(cancel)?;
    validate_leaf_name(name)?;
    let target_path = Path::new(parent_path).join(name);
    let target = path_text(&target_path)?;
    let mut input = guarded_io::open(source, false)?;
    // Check occupied targets before journal intent, and bind an existing item to
    // native identity so aliases/reparse points cannot become a silent skip.
    match fs::symlink_metadata(&target_path) {
        Ok(_) => {
            let existing = capture_source(target)?;
            let _existing_pin = guarded_io::open(&existing, false)?;
            if (existing.volume, existing.file_index) == (source.volume, source.file_index) {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            if skip_existing {
                on_outcome(outcome(source, target, ItemStatus::Skipped, 0))?;
                return Ok(None);
            }
            return Err(Fault::new(FaultCode::DestinationExists));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let basic = inspect_basic(&input, &source.path)?;
    let mut streams = capture_streams(source, &input, &basic)?;
    if !streams.is_empty() && !has_named_streams(parent)? {
        return Err(Fault::new(FaultCode::DestinationStreamsUnsupported));
    }
    on_intent(&source.path, target)?;
    if let Err(error) = cancelled(cancel) {
        on_outcome(outcome(source, target, ItemStatus::Failed, E_ABORT))?;
        return Err(error);
    }
    let folder = source.kind == EntryKind::Folder;
    let mut output = match guarded_io::create(parent, name, folder) {
        Ok(file) => file,
        Err(error) => {
            // A racing collision is not a proven skip. No mutation was made.
            on_outcome(outcome(source, target, ItemStatus::Failed, E_FAIL))?;
            return Err(error);
        }
    };
    let mut copied_streams = Vec::new();
    let result = (|| {
        let info = guarded_io::info(&output)?;
        if info.dwVolumeSerialNumber == source.volume
            && ((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
                == source.file_index
        {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        if !folder {
            copy_bytes(&mut input, &mut output, buffer, cancel)?;
        }
        copied_streams = copy_named_streams(&mut streams, parent, &output, name, buffer, cancel)?;
        verify_source_streams(source, &input, &basic, &streams)?;
        let target_identity = OperationSource {
            path: target.to_owned(),
            kind: source.kind,
            volume: info.dwVolumeSerialNumber,
            file_index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        };
        verify_stream_inventory(&target_identity, &output, &streams)?;
        if !folder {
            apply_basic(&output, &basic)?;
            output.sync_all().map_err(Fault::from)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        on_outcome(outcome(
            source,
            target,
            ItemStatus::Unknown,
            if error.code == FaultCode::Cancelled {
                E_ABORT
            } else {
                E_FAIL
            },
        ))?;
        return Err(error);
    }
    let _stream_guards = copied_streams;
    if folder {
        let entries = match fs::read_dir(&source.path) {
            Ok(entries) => entries,
            Err(error) => {
                on_outcome(outcome(source, target, ItemStatus::Unknown, E_FAIL))?;
                return Err(error.into());
            }
        };
        on_outcome(outcome(source, target, ItemStatus::Changed, 0))?;
        Ok(Some(Frame {
            entries,
            source_identity: source.clone(),
            basic,
            _source: input,
            target: output,
            target_path: target.to_owned(),
        }))
    } else {
        on_outcome(outcome(source, target, ItemStatus::Changed, 0))?;
        Ok(None)
    }
}

const MAX_NAMED_STREAMS: usize = 128;
const MAX_STREAM_NAME_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StreamDescription {
    name: Vec<u16>,
    size: u64,
}
struct NamedStream {
    description: StreamDescription,
    handle: File,
}

fn read_basic(file: &File) -> Result<FILE_BASIC_INFO, Fault> {
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: live pinned handle and exclusive initialized fixed-size output.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(Fault::new(FaultCode::UnsupportedProvider));
    }
    Ok(basic)
}
fn inspect_basic(file: &File, _path: &str) -> Result<FILE_BASIC_INFO, Fault> {
    let basic = read_basic(file)?;
    let ordinary = FILE_ATTRIBUTE_READONLY
        | FILE_ATTRIBUTE_HIDDEN
        | FILE_ATTRIBUTE_SYSTEM
        | FILE_ATTRIBUTE_DIRECTORY
        | FILE_ATTRIBUTE_ARCHIVE
        | FILE_ATTRIBUTE_NORMAL
        | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
        | FILE_ATTRIBUTE_TEMPORARY;
    if basic.FileAttributes & !ordinary != 0 {
        return Err(Fault::new(FaultCode::UnsupportedProvider));
    }
    Ok(basic)
}
fn has_named_streams(file: &File) -> Result<bool, Fault> {
    let mut flags = 0;
    // SAFETY: live pinned handle, exclusive flags output; optional outputs null.
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            std::ptr::null_mut(),
            0,
        )
    } == 0
    {
        return Err(Fault::new(FaultCode::UnsupportedProvider));
    }
    Ok(flags & 0x0004_0000 != 0) // FILE_NAMED_STREAMS
}

// Only captured, canonical ordinary paths reach this FFI boundary. Use the
// extended namespace so legal long paths do not depend on process manifest flags.
fn stream_query_path(path: &str) -> Vec<u16> {
    let extended = if let Some(unc) = path.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}")
    } else {
        format!(r"\\?\{path}")
    };
    extended.encode_utf16().chain([0]).collect()
}

fn stream_inventory(
    source: &OperationSource,
    primary: &File,
) -> Result<Vec<StreamDescription>, Fault> {
    let path = stream_query_path(&source.path);
    let mut stream = WIN32_FIND_STREAM_DATA::default();
    // SAFETY: pinned namespace, terminated path, exclusive fixed-size output.
    let handle = unsafe {
        FindFirstStreamW(
            path.as_ptr(),
            FindStreamInfoStandard,
            (&mut stream as *mut WIN32_FIND_STREAM_DATA).cast(),
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_HANDLE_EOF as i32)
            || !has_named_streams(primary)?
        {
            return Ok(Vec::new());
        }
        return Err(Fault::new(FaultCode::UnsupportedProvider));
    }
    let result = (|| {
        let mut names = Vec::new();
        let mut bytes = 0usize;
        loop {
            let end = stream
                .cStreamName
                .iter()
                .position(|&c| c == 0)
                .ok_or_else(|| Fault::new(FaultCode::UnsupportedProvider))?;
            let name = &stream.cStreamName[..end];
            if name != [58, 58, 36, 68, 65, 84, 65] {
                // ::$DATA
                guarded_io::validate_stream_name(name)?;
                if stream.StreamSize < 0 {
                    return Err(Fault::new(FaultCode::InvalidState));
                }
                bytes = bytes
                    .checked_add(name.len() * 2)
                    .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
                if names.len() >= MAX_NAMED_STREAMS || bytes > MAX_STREAM_NAME_BYTES {
                    return Err(Fault::new(FaultCode::ResourceLimit));
                }
                names.push(StreamDescription {
                    name: name.to_vec(),
                    size: stream.StreamSize as u64,
                });
            }
            // SAFETY: live enumeration handle and exclusive fixed-size output.
            if unsafe {
                FindNextStreamW(handle, (&mut stream as *mut WIN32_FIND_STREAM_DATA).cast())
            } == 0
            {
                if std::io::Error::last_os_error().raw_os_error() != Some(ERROR_HANDLE_EOF as i32) {
                    return Err(Fault::new(FaultCode::UnsupportedProvider));
                }
                names.sort();
                if names.windows(2).any(|pair| pair[0].name == pair[1].name) {
                    return Err(Fault::new(FaultCode::StaleRequest));
                }
                return Ok(names);
            }
        }
    })();
    // SAFETY: closes the uniquely owned successful enumeration handle.
    unsafe {
        FindClose(handle);
    }
    result
}
fn capture_streams(
    source: &OperationSource,
    primary: &File,
    basic: &FILE_BASIC_INFO,
) -> Result<Vec<NamedStream>, Fault> {
    let inventory = stream_inventory(source, primary)?;
    let mut streams = Vec::with_capacity(inventory.len());
    for description in inventory {
        // All ancestors and primary stay pinned. Validated suffix cannot redirect
        // the path; native UTF-16 stream names are never lossy-decoded.
        let path = std::ffi::OsString::from_wide(
            &source
                .path
                .encode_utf16()
                .chain(description.name.iter().copied())
                .collect::<Vec<_>>(),
        );
        let handle = OpenOptions::new()
            .access_mode(FILE_GENERIC_READ)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(Fault::from)?;
        let info = guarded_io::info(&handle)?;
        if info.dwVolumeSerialNumber != source.volume
            || ((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
                != source.file_index
        {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        streams.push(NamedStream {
            description,
            handle,
        });
    }
    // All captured ADS remain deny-write/delete until the file outcome callback.
    verify_source_streams(source, primary, basic, &streams)?;
    Ok(streams)
}
fn verify_source_streams(
    source: &OperationSource,
    primary: &File,
    basic: &FILE_BASIC_INFO,
    streams: &[NamedStream],
) -> Result<(), Fault> {
    let latest = read_basic(primary)?;
    let info = guarded_io::info(primary)?;
    if latest.LastWriteTime != basic.LastWriteTime
        || latest.CreationTime != basic.CreationTime
        || latest.FileAttributes != basic.FileAttributes
        || info.dwVolumeSerialNumber != source.volume
        || ((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
            != source.file_index
    {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    verify_stream_inventory(source, primary, streams)
}
fn verify_stream_inventory(
    source: &OperationSource,
    primary: &File,
    streams: &[NamedStream],
) -> Result<(), Fault> {
    let actual = stream_inventory(source, primary)?;
    if actual.len() != streams.len() || actual.iter().zip(streams).any(|(a, b)| a != &b.description)
    {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    Ok(())
}
fn copy_named_streams(
    streams: &mut [NamedStream],
    parent: &File,
    primary: &File,
    leaf: &str,
    buffer: &mut [u8],
    cancel: &AtomicBool,
) -> Result<Vec<File>, Fault> {
    let mut handles = Vec::with_capacity(streams.len());
    for stream in streams {
        cancelled(cancel)?;
        let mut target =
            guarded_io::create_stream(parent, primary, leaf, &stream.description.name)?;
        let bytes = copy_bytes(&mut stream.handle, &mut target, buffer, cancel)?;
        if bytes != stream.description.size {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        target.sync_all().map_err(Fault::from)?;
        handles.push(target);
    }
    Ok(handles)
}

fn apply_basic(file: &File, basic: &FILE_BASIC_INFO) -> Result<(), Fault> {
    // SAFETY: live created handle and initialized fixed-size input; no retained pointer.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (basic as *const FILE_BASIC_INFO).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(Fault::from(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn copy_bytes(
    input: &mut impl Read,
    output: &mut impl Write,
    buffer: &mut [u8],
    cancel: &AtomicBool,
) -> Result<u64, Fault> {
    let mut copied = 0u64;
    loop {
        cancelled(cancel)?;
        let count = input.read(buffer).map_err(Fault::from)?;
        if count == 0 {
            return Ok(copied);
        }
        cancelled(cancel)?;
        output.write_all(&buffer[..count]).map_err(Fault::from)?;
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_queries_preserve_local_and_unc_names_in_extended_namespace() {
        for (path, expected) in [
            (r"C:\owned\name.txt", r"\\?\C:\owned\name.txt"),
            (
                r"\\server\share\owned\name.txt",
                r"\\?\UNC\server\share\owned\name.txt",
            ),
        ] {
            let wide = stream_query_path(path);
            assert_eq!(wide.last(), Some(&0));
            assert_eq!(
                String::from_utf16(&wide[..wide.len() - 1]).unwrap(),
                expected
            );
        }
    }
    #[test]
    fn copies_long_paths_and_named_streams_in_both_directions() {
        let (root, _source, destination) = fixture();
        let mut long = root.path().join("long");
        for n in 0..10 {
            long = long.join(format!("{n}-{}", "long-segment-".repeat(5)));
        }
        fs::create_dir_all(&long).unwrap();
        let file = long.join("payload.txt");
        fs::write(&file, b"long default bytes").unwrap();
        fs::write(
            std::ffi::OsString::from(format!("{}:Zone.Identifier", file.display())),
            b"[ZoneTransfer]\nZoneId=3",
        )
        .unwrap();
        assert!(file.to_str().unwrap().len() > 500);
        let source = capture_source(file.to_str().unwrap()).unwrap();
        copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        let short = Path::new(&destination.path).join("payload.txt");
        assert_eq!(fs::read(&short).unwrap(), b"long default bytes");
        assert_eq!(
            fs::read(format!("{}:Zone.Identifier", short.display())).unwrap(),
            b"[ZoneTransfer]\nZoneId=3"
        );
        let source = capture_source(short.to_str().unwrap()).unwrap();
        let target = capture_source(long.to_str().unwrap()).unwrap();
        copy_root(
            &source,
            &target,
            Some("copy.txt"),
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            fs::read(long.join("copy.txt")).unwrap(),
            b"long default bytes"
        );
        assert_eq!(
            fs::read(format!(
                "{}:Zone.Identifier",
                long.join("copy.txt").display()
            ))
            .unwrap(),
            b"[ZoneTransfer]\nZoneId=3"
        );
    }
    #[test]
    fn cancellation_between_chunks_retains_only_completed_bytes() {
        struct CancellingWriter<'a> {
            bytes: Vec<u8>,
            cancel: &'a AtomicBool,
        }
        impl Write for CancellingWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                self.cancel.store(true, Ordering::Release);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let cancel = AtomicBool::new(false);
        let mut writer = CancellingWriter {
            bytes: Vec::new(),
            cancel: &cancel,
        };
        let mut input = std::io::Cursor::new(vec![7; 32]);
        let result = copy_bytes(&mut input, &mut writer, &mut [0; 8], &cancel);
        assert_eq!(result.unwrap_err().code, FaultCode::Cancelled);
        assert_eq!(writer.bytes, vec![7; 8]);
    }
    #[test]
    fn hard_link_alias_is_not_a_skip() {
        let (_root, source_dir, destination) = fixture();
        let source_path = Path::new(&source_dir.path).join("a.txt");
        fs::write(&source_path, b"original").unwrap();
        fs::hard_link(&source_path, Path::new(&destination.path).join("a.txt")).unwrap();
        let source = capture_source(source_path.to_str().unwrap()).unwrap();
        let result = copy_root(
            &source,
            &destination,
            None,
            true,
            &AtomicBool::new(false),
            |_, _| panic!("alias"),
            |item| {
                assert_eq!(item.status, ItemStatus::Failed);
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, FaultCode::InvalidPath);
        assert_eq!(fs::read(source_path).unwrap(), b"original");
    }
    #[test]
    fn depth_first_copy_preserves_nested_contents() {
        let (_root, source, destination) = fixture();
        fs::create_dir_all(Path::new(&source.path).join("one/two")).unwrap();
        fs::write(Path::new(&source.path).join("one/two/file"), b"nested").unwrap();
        fs::write(Path::new(&source.path).join("sibling"), b"sibling").unwrap();
        copy_root(
            &source,
            &destination,
            Some("renamed"),
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            fs::read(Path::new(&destination.path).join("renamed/one/two/file")).unwrap(),
            b"nested"
        );
        assert_eq!(
            fs::read(Path::new(&destination.path).join("renamed/sibling")).unwrap(),
            b"sibling"
        );
    }
    #[test]
    fn preserves_default_zone_unicode_and_empty_streams() {
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("download.txt");
        fs::write(&path, b"primary").unwrap();
        let values = [
            (
                "Zone.Identifier",
                b"[ZoneTransfer]\r\nZoneId=3\r\n".to_vec(),
            ),
            ("\u{d55c}\u{ae00}", vec![42; BUFFER_SIZE + 17]),
            ("empty", Vec::new()),
        ];
        for (name, value) in &values {
            fs::write(format!("{}:{name}", path.display()), value).unwrap();
        }
        let source = capture_source(path.to_str().unwrap()).unwrap();
        copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        let target = Path::new(&destination.path).join("download.txt");
        assert_eq!(fs::read(&target).unwrap(), b"primary");
        assert_eq!(fs::read(&path).unwrap(), b"primary");
        for (name, value) in &values {
            assert_eq!(
                fs::read(format!("{}:{name}", target.display())).unwrap(),
                *value
            );
            assert_eq!(
                fs::read(format!("{}:{name}", path.display())).unwrap(),
                *value
            );
        }
    }
    #[test]
    fn preserves_basic_file_metadata() {
        use std::os::windows::fs::MetadataExt;
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("metadata.txt");
        fs::write(&path, b"contents").unwrap();
        let file = File::options().read(true).write(true).open(&path).unwrap();
        let mut basic = inspect_basic(&file, path.to_str().unwrap()).unwrap();
        basic.CreationTime = 132000000000000000;
        basic.LastWriteTime = 132000000010000000;
        basic.LastAccessTime = 132000000020000000;
        basic.FileAttributes =
            FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_ARCHIVE;
        apply_basic(&file, &basic).unwrap();
        drop(file);
        let source = capture_source(path.to_str().unwrap()).unwrap();
        copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        let target = Path::new(&destination.path).join("metadata.txt");
        let result = fs::metadata(&target).unwrap();
        assert_eq!(result.creation_time(), basic.CreationTime as u64);
        assert_eq!(result.last_write_time(), basic.LastWriteTime as u64);
        assert_eq!(result.file_attributes(), basic.FileAttributes);
        // Restore fixture-only readonly flags so TempDir can remove test files.
        for path in [path, target] {
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            // Windows-only synthetic fixture cleanup.
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    /// Run explicitly with PROJECTPANE_COPY_SOURCE_ROOT and
    /// PROJECTPANE_COPY_DESTINATION_ROOT naming existing permitted fixture roots.
    #[test]
    #[ignore = "requires explicit local/SMB/WSL fixture roots"]
    fn explicit_provider_fixture() {
        let source_base =
            std::env::var("PROJECTPANE_COPY_SOURCE_ROOT").expect("source fixture root");
        let target_base =
            std::env::var("PROJECTPANE_COPY_DESTINATION_ROOT").expect("destination fixture root");
        let source_fixture = tempfile::Builder::new()
            .prefix("projectpane-copy-source-")
            .tempdir_in(source_base)
            .unwrap();
        let target_fixture = tempfile::Builder::new()
            .prefix("projectpane-copy-target-")
            .tempdir_in(target_base)
            .unwrap();
        fs::create_dir(source_fixture.path().join("nested")).unwrap();
        fs::write(
            source_fixture.path().join("nested/data.txt"),
            b"cross-provider fixture",
        )
        .unwrap();
        let case_pair = std::env::var_os("PROJECTPANE_COPY_CASE_PAIR").is_some();
        if case_pair {
            fs::write(source_fixture.path().join("Foo.txt"), b"UPPER fixture").unwrap();
            fs::write(source_fixture.path().join("foo.txt"), b"lower fixture").unwrap();
            assert_eq!(
                fs::read(source_fixture.path().join("Foo.txt")).unwrap(),
                b"UPPER fixture"
            );
            assert_eq!(
                fs::read(source_fixture.path().join("foo.txt")).unwrap(),
                b"lower fixture"
            );
        }
        let result = (|| {
            let source = capture_source(source_fixture.path().to_str().unwrap())?;
            let target = capture_source(target_fixture.path().to_str().unwrap())?;
            copy_root(
                &source,
                &target,
                Some("copied"),
                false,
                &AtomicBool::new(false),
                |_, _| Ok(()),
                |item| {
                    eprintln!("{:?}: {}", item.status, item.source);
                    Ok(())
                },
            )?;
            if fs::read(target_fixture.path().join("copied/nested/data.txt"))
                .map_err(Fault::from)?
                != b"cross-provider fixture"
            {
                return Err(Fault::new(FaultCode::IoError));
            }
            if case_pair {
                use sha2::{Digest, Sha256};
                let destination = target_fixture.path().join("copied");
                let names = fs::read_dir(&destination)
                    .map_err(Fault::from)?
                    .map(|item| item.map(|item| item.file_name()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(Fault::from)?;
                for (name, bytes) in [
                    ("Foo.txt", b"UPPER fixture".as_slice()),
                    ("foo.txt", b"lower fixture".as_slice()),
                ] {
                    if !names.iter().any(|actual| actual == name) {
                        return Err(Fault::new(FaultCode::IoError));
                    }
                    let copied = fs::read(destination.join(name)).map_err(Fault::from)?;
                    if copied != bytes {
                        return Err(Fault::new(FaultCode::IoError));
                    }
                    eprintln!("CASE VERIFIED {name}: {:x}", Sha256::digest(&copied));
                }
            }
            Ok::<(), Fault>(())
        })();
        if let Err(error) = result {
            let source_path = source_fixture.keep();
            let target_path = target_fixture.keep();
            panic!(
                "provider copy failed: {:?}; preserved fixtures: {:?}, {:?}",
                error.code, source_path, target_path
            );
        }
    }

    #[test]
    fn nested_locked_stream_reports_exact_failed_child() {
        let (_root, source, destination) = fixture();
        let child = Path::new(&source.path).join("unsupported.txt");
        fs::write(&child, b"primary").unwrap();
        fs::write(format!("{}:extra", child.to_str().unwrap()), b"extra").unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(format!("{}:extra", child.display()))
            .unwrap();
        let mut outcomes = Vec::new();
        let error = copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |item| {
                outcomes.push(item);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            FaultCode::IoError | FaultCode::AccessDenied
        ));
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].status, ItemStatus::Changed);
        assert_eq!(outcomes[1].status, ItemStatus::Failed);
        assert_eq!(outcomes[1].source, child.to_str().unwrap());
        assert_eq!(
            outcomes[1].destination.as_deref(),
            Path::new(&destination.path)
                .join("source")
                .join("unsupported.txt")
                .to_str()
        );
    }

    /// A real provider is required: faking a capability does not prove native
    /// no-stream behavior. Neither fixture root may contain user data to mutate.
    #[test]
    #[ignore = "requires explicit ADS source and no-stream destination fixture roots"]
    fn explicit_no_stream_provider_reports_exact_child_without_creating_it() {
        let source_base = std::env::var("PROJECTPANE_COPY_SOURCE_ROOT").unwrap();
        let target_base = std::env::var("PROJECTPANE_COPY_DESTINATION_ROOT").unwrap();
        let source_fixture = tempfile::Builder::new()
            .prefix("projectpane-ads-source-")
            .tempdir_in(source_base)
            .unwrap();
        let target_fixture = tempfile::Builder::new()
            .prefix("projectpane-ads-target-")
            .tempdir_in(target_base)
            .unwrap();
        // Retain native evidence even if an assertion fails.
        let source_path = source_fixture.keep();
        let target_path = target_fixture.keep();
        eprintln!("ADS diagnostic fixtures: {source_path:?}, {target_path:?}");
        let child = source_path.join("ads.txt");
        fs::write(&child, b"primary sentinel").unwrap();
        let stream = format!("{}:Zone.Identifier", child.display());
        fs::write(&stream, b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        let source = capture_source(source_path.to_str().unwrap()).unwrap();
        let destination = capture_source(target_path.to_str().unwrap()).unwrap();
        let target = guarded_io::open(&destination, false).unwrap();
        assert!(
            !has_named_streams(&target).unwrap(),
            "fixture must lack named streams"
        );
        drop(target);
        let mut outcomes = Vec::new();
        let mut intents = Vec::new();
        let error = copy_root(
            &source,
            &destination,
            Some("copied"),
            false,
            &AtomicBool::new(false),
            |source, _| {
                intents.push(source.to_owned());
                Ok(())
            },
            |item| {
                outcomes.push(item);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::DestinationStreamsUnsupported);
        assert_eq!(intents, vec![source.path.clone()]);
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].status, ItemStatus::Changed);
        assert_eq!(outcomes[1].status, ItemStatus::Failed);
        assert_eq!(outcomes[1].source, child.to_str().unwrap());
        assert_eq!(
            outcomes[1].destination.as_deref(),
            target_path.join("copied").join("ads.txt").to_str()
        );
        assert!(target_path.join("copied").is_dir());
        assert!(!target_path.join("copied").join("ads.txt").exists());
        assert_eq!(fs::read(&child).unwrap(), b"primary sentinel");
        assert_eq!(
            fs::read(&stream).unwrap(),
            b"[ZoneTransfer]\r\nZoneId=3\r\n"
        );
    }

    #[test]
    #[ignore = "requires Windows symbolic-link creation privilege or Developer Mode"]
    fn symbolic_links_report_exact_child_and_reject_ancestor_without_following() {
        let (root, source, destination) = fixture();
        let real = root.path().join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("sentinel"), b"retain").unwrap();
        let link = Path::new(&source.path).join("link");
        std::os::windows::fs::symlink_dir(&real, &link).unwrap();
        assert_eq!(
            capture_source(link.to_str().unwrap()).unwrap_err().code,
            FaultCode::UnsafeReparsePoint
        );
        assert_eq!(
            capture_source(link.join("sentinel").to_str().unwrap())
                .unwrap_err()
                .code,
            FaultCode::UnsafeReparsePoint
        );
        let mut outcomes = Vec::new();
        let error = copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |item| {
                outcomes.push(item);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::UnsafeReparsePoint);
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[1].status, ItemStatus::Failed);
        assert_eq!(outcomes[1].source, link.to_str().unwrap());
        assert_eq!(
            outcomes[1].destination.as_deref(),
            Path::new(&destination.path)
                .join("source")
                .join("link")
                .to_str()
        );
        assert!(
            !Path::new(&destination.path)
                .join("source")
                .join("link")
                .exists()
        );
        assert_eq!(fs::read(real.join("sentinel")).unwrap(), b"retain");
    }
    #[test]
    fn failed_intent_never_appends_a_fallback_outcome() {
        let (_root, source, destination) = fixture();
        assert!(
            copy_root(
                &source,
                &destination,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| Err(Fault::new(FaultCode::IoError)),
                |_| panic!("journal callback failed; no further append is safe")
            )
            .is_err()
        );
    }

    #[test]
    fn preserves_directory_streams_before_creating_children() {
        let (_root, source, destination) = fixture();
        fs::write(format!("{}:note", source.path), b"directory ADS").unwrap();
        fs::write(Path::new(&source.path).join("child"), b"child").unwrap();
        copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            fs::read(format!(
                "{}:note",
                Path::new(&destination.path).join("source").display()
            ))
            .unwrap(),
            b"directory ADS"
        );
        assert_eq!(
            fs::read(Path::new(&destination.path).join("source/child")).unwrap(),
            b"child"
        );
    }
    #[test]
    fn occupied_target_stream_is_never_overwritten() {
        let (_root, source_dir, destination) = fixture();
        let source_path = Path::new(&source_dir.path).join("file");
        fs::write(&source_path, b"primary").unwrap();
        fs::write(format!("{}:extra", source_path.display()), b"source ADS").unwrap();
        let source = capture_source(source_path.to_str().unwrap()).unwrap();
        let input = guarded_io::open(&source, false).unwrap();
        let basic = inspect_basic(&input, &source.path).unwrap();
        let mut streams = capture_streams(&source, &input, &basic).unwrap();
        let parent = guarded_io::open(&destination, false).unwrap();
        let output = guarded_io::create(&parent, "file", false).unwrap();
        let target_stream = format!(
            "{}:extra",
            Path::new(&destination.path).join("file").display()
        );
        fs::write(&target_stream, b"racing writer").unwrap();
        let error = copy_named_streams(
            &mut streams,
            &parent,
            &output,
            "file",
            &mut [0; 32],
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::DestinationExists);
        assert_eq!(fs::read(target_stream).unwrap(), b"racing writer");
        assert_eq!(
            fs::read(format!("{}:extra", source_path.display())).unwrap(),
            b"source ADS"
        );
    }
    #[test]
    fn captured_streams_deny_writes_and_detect_new_source_streams() {
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("file");
        fs::write(&path, b"primary").unwrap();
        fs::write(format!("{}:first", path.display()), b"first").unwrap();
        let source = capture_source(path.to_str().unwrap()).unwrap();
        let mut outcomes = Vec::new();
        let error = copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| {
                assert!(
                    OpenOptions::new()
                        .write(true)
                        .open(format!("{}:first", path.display()))
                        .is_err()
                );
                fs::write(format!("{}:new", path.display()), b"new source stream").unwrap();
                Ok(())
            },
            |item| {
                outcomes.push(item);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::StaleRequest);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].status, ItemStatus::Unknown);
        assert_eq!(
            fs::read(Path::new(&destination.path).join("file")).unwrap(),
            b"primary"
        );
        assert_eq!(
            fs::read(format!("{}:new", path.display())).unwrap(),
            b"new source stream"
        );
    }
    #[test]
    fn cancellation_before_stream_creation_leaves_primary_and_no_stream() {
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("file");
        fs::write(&path, b"primary").unwrap();
        fs::write(format!("{}:extra", path.display()), b"source ADS").unwrap();
        let source = capture_source(path.to_str().unwrap()).unwrap();
        let input = guarded_io::open(&source, false).unwrap();
        let basic = inspect_basic(&input, &source.path).unwrap();
        let mut streams = capture_streams(&source, &input, &basic).unwrap();
        let parent = guarded_io::open(&destination, false).unwrap();
        let output = guarded_io::create(&parent, "file", false).unwrap();
        let error = copy_named_streams(
            &mut streams,
            &parent,
            &output,
            "file",
            &mut [0; 8],
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::Cancelled);
        assert!(
            !Path::new(&format!(
                "{}:extra",
                Path::new(&destination.path).join("file").display()
            ))
            .exists()
        );
    }
    #[test]
    fn stream_names_cannot_redirect_native_creation() {
        for name in [
            "::$DATA",
            ":x\\y:$DATA",
            ":x/y:$DATA",
            ":x:y:$DATA",
            ":x:$INDEX_ALLOCATION",
            ":x\0:$DATA",
        ] {
            assert!(
                guarded_io::validate_stream_name(&name.encode_utf16().collect::<Vec<_>>()).is_err()
            );
        }
        assert!(
            guarded_io::validate_stream_name(
                &":Zone.Identifier:$DATA".encode_utf16().collect::<Vec<_>>()
            )
            .is_ok()
        );
    }

    #[test]
    fn native_partial_ads_survives_mid_stream_cancellation() {
        struct CancelAfterWrite<'a> {
            file: &'a mut File,
            cancel: &'a AtomicBool,
        }
        impl Write for CancelAfterWrite<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let count = self.file.write(bytes)?;
                self.cancel.store(true, Ordering::Release);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.file.flush()
            }
        }
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("file");
        fs::write(&path, b"primary").unwrap();
        fs::write(format!("{}:extra", path.display()), b"0123456789abcdef").unwrap();
        let source = capture_source(path.to_str().unwrap()).unwrap();
        let input = guarded_io::open(&source, false).unwrap();
        let basic = inspect_basic(&input, &source.path).unwrap();
        let mut streams = capture_streams(&source, &input, &basic).unwrap();
        let parent = guarded_io::open(&destination, false).unwrap();
        let output = guarded_io::create(&parent, "file", false).unwrap();
        let mut ads =
            guarded_io::create_stream(&parent, &output, "file", &streams[0].description.name)
                .unwrap();
        let cancel = AtomicBool::new(false);
        let error = copy_bytes(
            &mut streams[0].handle,
            &mut CancelAfterWrite {
                file: &mut ads,
                cancel: &cancel,
            },
            &mut [0; 8],
            &cancel,
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::Cancelled);
        ads.sync_all().unwrap();
        drop(ads);
        assert_eq!(
            fs::read(format!(
                "{}:extra",
                Path::new(&destination.path).join("file").display()
            ))
            .unwrap(),
            b"01234567"
        );
        assert_eq!(
            fs::read(format!("{}:extra", path.display())).unwrap(),
            b"0123456789abcdef"
        );
    }
    #[test]
    fn per_file_stream_budget_fails_before_destination_creation() {
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("file");
        fs::write(&path, b"primary").unwrap();
        for n in 0..=MAX_NAMED_STREAMS {
            fs::write(format!("{}:stream{n}", path.display()), b"x").unwrap();
        }
        let source = capture_source(path.to_str().unwrap()).unwrap();
        let mut failures = 0;
        let error = copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| panic!("budget must be checked before intent"),
            |item| {
                assert_eq!(item.status, ItemStatus::Failed);
                failures += 1;
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error.code, FaultCode::ResourceLimit);
        assert_eq!(failures, 1);
        assert!(!Path::new(&destination.path).join("file").exists());
    }

    #[test]
    fn skip_existing_does_not_need_locked_source_ads() {
        let (_root, source_dir, destination) = fixture();
        let path = Path::new(&source_dir.path).join("file");
        fs::write(&path, b"source").unwrap();
        fs::write(format!("{}:extra", path.display()), b"locked").unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(format!("{}:extra", path.display()))
            .unwrap();
        let source = capture_source(path.to_str().unwrap()).unwrap();
        let target = Path::new(&destination.path).join("file");
        fs::write(&target, b"keep").unwrap();
        let mut skipped = 0;
        copy_root(
            &source,
            &destination,
            None,
            true,
            &AtomicBool::new(false),
            |_, _| panic!("skip must not mutate"),
            |item| {
                assert_eq!(item.status, ItemStatus::Skipped);
                skipped += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }
    #[test]
    fn unexpected_destination_stream_is_detected_without_deletion() {
        let (_root, source, destination) = fixture();
        let parent = guarded_io::open(&destination, false).unwrap();
        let output = guarded_io::create(&parent, "file", false).unwrap();
        let path = Path::new(&destination.path).join("file");
        fs::write(format!("{}:external", path.display()), b"retain").unwrap();
        let target = capture_source(path.to_str().unwrap()).unwrap();
        assert_eq!(
            verify_stream_inventory(&target, &output, &[])
                .unwrap_err()
                .code,
            FaultCode::StaleRequest
        );
        assert_eq!(
            fs::read(format!("{}:external", path.display())).unwrap(),
            b"retain"
        );
        assert!(Path::new(&source.path).exists());
    }

    fn fixture() -> (tempfile::TempDir, OperationSource, OperationSource) {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        let source = capture_source(source.to_str().unwrap()).unwrap();
        let destination = capture_source(destination.to_str().unwrap()).unwrap();
        (root, source, destination)
    }
    #[test]
    fn streams_more_than_2048_descendants() {
        let (_root, source, destination) = fixture();
        for n in 0..2051 {
            fs::write(Path::new(&source.path).join(format!("{n}.txt")), b"bytes").unwrap();
        }
        let mut intents = 0;
        let mut outcomes = 0;
        copy_root(
            &source,
            &destination,
            None,
            false,
            &AtomicBool::new(false),
            |_, _| {
                intents += 1;
                Ok(())
            },
            |item| {
                assert_eq!(item.status, ItemStatus::Changed);
                outcomes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!((intents, outcomes), (2053, 2053));
        assert_eq!(
            fs::read_dir(Path::new(&destination.path).join("source"))
                .unwrap()
                .count(),
            2051
        );
        assert_eq!(
            fs::read(Path::new(&destination.path).join("source/2050.txt")).unwrap(),
            b"bytes"
        );
    }
    #[test]
    fn cancellation_after_intent_does_not_mutate() {
        let (_root, source, destination) = fixture();
        let cancel = AtomicBool::new(false);
        let result = copy_root(
            &source,
            &destination,
            None,
            false,
            &cancel,
            |_, _| {
                cancel.store(true, Ordering::Release);
                Ok(())
            },
            |item| {
                assert_eq!(item.status, ItemStatus::Failed);
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, FaultCode::Cancelled);
        assert!(!Path::new(&destination.path).join("source").exists());
    }
    #[test]
    fn cancellation_preserves_completed_directory() {
        let (_root, source, destination) = fixture();
        fs::write(Path::new(&source.path).join("a"), b"a").unwrap();
        let cancel = AtomicBool::new(false);
        let mut statuses = Vec::new();
        let result = copy_root(
            &source,
            &destination,
            None,
            false,
            &cancel,
            |_, _| Ok(()),
            |item| {
                statuses.push(item.status);
                cancel.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, FaultCode::Cancelled);
        assert_eq!(statuses, vec![ItemStatus::Changed]);
        assert!(Path::new(&destination.path).join("source").is_dir());
        assert!(!Path::new(&destination.path).join("source/a").exists());
    }
    #[test]
    fn rejects_self_descendant_stale_and_unsafe_names() {
        let (_root, source, destination) = fixture();
        for name in ["..", "a/b", "a:b"] {
            assert!(
                copy_root(
                    &source,
                    &destination,
                    Some(name),
                    false,
                    &AtomicBool::new(false),
                    |_, _| panic!("invalid name"),
                    |_| Ok(())
                )
                .is_err()
            );
        }
        assert!(
            copy_root(
                &source,
                &source,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| panic!("self copy"),
                |_| Ok(())
            )
            .is_err()
        );
        let nested = Path::new(&source.path).join("nested");
        fs::create_dir(&nested).unwrap();
        let nested = capture_source(nested.to_str().unwrap()).unwrap();
        assert!(
            copy_root(
                &source,
                &nested,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| panic!("recursive copy"),
                |_| Ok(())
            )
            .is_err()
        );
        let mut stale = source.clone();
        stale.file_index ^= 1;
        assert!(
            copy_root(
                &stale,
                &destination,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| panic!("stale identity"),
                |_| Ok(())
            )
            .is_err()
        );
    }
    #[test]
    fn conflicts_never_overwrite_and_skip_does_not_merge() {
        let (_root, source, destination) = fixture();
        let target = Path::new(&destination.path).join("source");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"original").unwrap();
        assert_eq!(
            copy_root(
                &source,
                &destination,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| panic!("occupied"),
                |_| Ok(())
            )
            .unwrap_err()
            .code,
            FaultCode::DestinationExists
        );
        let mut skipped = 0;
        copy_root(
            &source,
            &destination,
            None,
            true,
            &AtomicBool::new(false),
            |_, _| panic!("skip has no mutation"),
            |item| {
                assert_eq!(item.status, ItemStatus::Skipped);
                skipped += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"original");
    }
    #[test]
    fn intent_failure_prevents_mutation_and_outcome_failure_retains_artifact() {
        let (_root, source, destination) = fixture();
        let error = || Fault::new(FaultCode::IoError);
        assert!(
            copy_root(
                &source,
                &destination,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| Err(error()),
                |_| Ok(())
            )
            .is_err()
        );
        assert!(!Path::new(&destination.path).join("source").exists());
        assert!(
            copy_root(
                &source,
                &destination,
                None,
                false,
                &AtomicBool::new(false),
                |_, _| Ok(()),
                |_| Err(error())
            )
            .is_err()
        );
        assert!(Path::new(&destination.path).join("source").is_dir());
    }
}
