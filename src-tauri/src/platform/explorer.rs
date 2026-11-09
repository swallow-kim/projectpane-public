//! Narrow Shell integration for physical filesystem items. External transfers are copy-only.
pub mod context_menu;
pub mod path_text;
// Research implementation only. No desktop command, feature flag, or clipboard
// entry point can enable external moves until recovery integration is complete.
#[cfg(test)]
pub(crate) mod external_move;
use crate::domain::{AbsolutePath, ActivationKind, ActivationTarget, Fault, FaultCode};
use std::{collections::HashSet, path::Path, ptr};
use windows::{
    Win32::{
        Foundation::{
            CLIPBRD_E_CANT_OPEN, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
            DV_E_FORMATETC, E_ABORT, S_OK,
        },
        System::{
            Com::{
                CoTaskMemFree, DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL,
            },
            DataExchange::{GetClipboardSequenceNumber, RegisterClipboardFormatW},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
                OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard,
                OleUninitialize, ReleaseStgMedium,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{
            Common::ITEMIDLIST, ILFindLastID, SHCreateDataObject, SHOP_FILEPATH,
            SHObjectProperties, SHOpenFolderAndSelectItems, SHParseDisplayName,
        },
    },
    core::{BOOL, HRESULT, Interface, PCWSTR, implement, w},
};

// Native Shell formats require a contiguous payload; bound bytes, not item count.
const MAX_PATH_BYTES: usize = 16 * 1024 * 1024;

fn native_error(_: windows::core::Error) -> Fault {
    Fault::new(FaultCode::IoError)
}
struct OleApartment;
impl OleApartment {
    fn new() -> Result<Self, Fault> {
        // SAFETY: each successful initialization is balanced on this same thread.
        unsafe { OleInitialize(None) }.map_err(native_error)?;
        Ok(Self)
    }
}
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}
struct Pidl(*mut ITEMIDLIST);
impl Pidl {
    fn parse(path: &str) -> Result<Self, Fault> {
        AbsolutePath::new(path.to_owned())?;
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut pidl = ptr::null_mut();
        // SAFETY: input is NUL-terminated; the output allocation is owned by this guard.
        unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) }
            .map_err(native_error)?;
        if pidl.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        Ok(Self(pidl))
    }
}
impl Drop for Pidl {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast())) };
    }
}

pub fn validate_paths(paths: &[String]) -> Result<(), Fault> {
    if paths.is_empty() || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_BYTES {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let mut unique = HashSet::new();
    for path in paths {
        AbsolutePath::new(path.clone())?;
        let key = path.replace('/', "\\");
        // UNC providers (including WSL) may have case-sensitive names.
        let key = if key.starts_with(r"\\") {
            key
        } else {
            key.to_lowercase()
        };
        if !unique.insert(key) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    Ok(())
}

pub fn targets(paths: Vec<String>) -> Result<Vec<ActivationTarget>, Fault> {
    let paths = normalize_import_paths(paths)?;
    paths
        .into_iter()
        .map(|path| {
            use std::os::windows::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(&path).map_err(Fault::from)?;
            if metadata.file_attributes() & 0x400 != 0
                || (!metadata.is_dir() && !metadata.is_file())
            {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            Ok(ActivationTarget {
                kind: if metadata.is_dir() {
                    ActivationKind::Folder
                } else {
                    ActivationKind::File
                },
                path,
            })
        })
        .collect()
}

// Windows file drops can carry verbatim UNC paths. Convert only the documented
// filesystem prefixes; never treat a device path or file:// text as an address.
pub fn normalize_import_paths(paths: Vec<String>) -> Result<Vec<String>, Fault> {
    validate_paths(&paths)?;
    let normalized = paths
        .into_iter()
        .map(|path| {
            let ordinary = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{rest}")
            } else if let Some(rest) = path.strip_prefix(r"\\?\") {
                let bytes = rest.as_bytes();
                if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
                    return Err(Fault::new(FaultCode::InvalidPath));
                }
                rest.to_owned()
            } else {
                path.clone()
            };
            // Verbatim paths do not interpret dot segments. Do not invent a different
            // target by normalizing them as ordinary Win32 paths.
            if path.starts_with(r"\\?\") && ordinary.split('\\').any(|p| p == "." || p == "..") {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            crate::domain::projects::normalize_absolute_path(&ordinary)
                .map_err(|_| Fault::new(FaultCode::InvalidPath))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_paths(&normalized)?;
    Ok(normalized)
}

struct ShellSelection {
    parent: Pidl,
    items: Vec<Pidl>,
}
impl ShellSelection {
    fn new(paths: &[String]) -> Result<Self, Fault> {
        validate_paths(paths)?;
        let parent = Path::new(&paths[0])
            .parent()
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
        if paths
            .iter()
            .any(|path| Path::new(path).parent() != Some(parent))
        {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        let parent = Pidl::parse(
            parent
                .to_str()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
        )?;
        let items: Vec<Pidl> = paths
            .iter()
            .map(|path| Pidl::parse(path))
            .collect::<Result<_, _>>()?;
        Ok(Self { parent, items })
    }
    fn children(&self) -> Vec<*const ITEMIDLIST> {
        // SAFETY: each child pointer remains valid while its owning PIDL is alive.
        self.items
            .iter()
            .map(|item| unsafe { ILFindLastID(item.0) as *const ITEMIDLIST })
            .collect()
    }
}

fn shell_object(paths: &[String]) -> Result<IDataObject, Fault> {
    let selection = ShellSelection::new(paths)?;
    let children = selection.children();
    // SAFETY: parent and child PIDLs stay alive until the Shell copies them.
    unsafe { SHCreateDataObject(Some(selection.parent.0), Some(&children), None) }
        .map_err(native_error)
}

pub fn reveal(paths: Vec<String>) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    let selection = ShellSelection::new(&paths)?;
    let children = selection.children();
    unsafe { SHOpenFolderAndSelectItems(selection.parent.0, Some(&children), 0) }
        .map_err(native_error)
}

pub fn properties(path: String) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    AbsolutePath::new(path.clone())?;
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // A fixed Shell command, with no shell command-line or arbitrary verb input.
    if unsafe { SHObjectProperties(None, SHOP_FILEPATH, PCWSTR(wide.as_ptr()), PCWSTR::null()) }
        .as_bool()
    {
        Ok(())
    } else {
        Err(Fault::new(FaultCode::IoError))
    }
}

// Called only on background workers. At most four extra attempts and 150 ms of
// deliberate waiting; this does not impose a timeout on a native OLE call.
const CLIPBOARD_RETRY_DELAYS_MS: [u64; 4] = [10, 20, 40, 80];

fn retry_clipboard_open<T>(
    mut operation: impl FnMut() -> windows::core::Result<T>,
    mut unchanged: impl FnMut() -> bool,
    mut sleep: impl FnMut(std::time::Duration),
) -> windows::core::Result<T> {
    let mut delays = CLIPBOARD_RETRY_DELAYS_MS.into_iter();
    loop {
        match operation() {
            Err(error) if error.code() == CLIPBRD_E_CANT_OPEN => {
                let Some(delay) = delays.next() else {
                    return Err(error);
                };
                if !unchanged() {
                    return Err(error);
                }
                sleep(std::time::Duration::from_millis(delay));
                if !unchanged() {
                    return Err(error);
                }
            }
            result => return result,
        }
    }
}

fn same_clipboard_sequence(expected: u32, current: u32) -> bool {
    expected != 0 && current == expected
}

fn clipboard_is_current(result: HRESULT) -> bool {
    result == S_OK
}

fn current_clipboard_object(object: &IDataObject) -> bool {
    // The generated wrapper converts both S_OK and S_FALSE to Ok(()). Preserve
    // the HRESULT: S_FALSE means another object owns the clipboard.
    windows::core::link!("ole32.dll" "system" fn OleIsCurrentClipboard(object: *mut core::ffi::c_void) -> HRESULT);
    // SAFETY: the live IDataObject remains owned by this OLE apartment.
    clipboard_is_current(unsafe { OleIsCurrentClipboard(object.as_raw()) })
}

fn set_and_flush_clipboard(
    set: impl FnOnce() -> windows::core::Result<()>,
    mut flush: impl FnMut() -> windows::core::Result<()>,
    mut owns_clipboard: impl FnMut() -> bool,
    unchanged: impl FnMut() -> bool,
    sleep: impl FnMut(std::time::Duration),
) -> windows::core::Result<()> {
    // Never retry Set: a separate sequence check cannot atomically prevent
    // overwriting a newer copy. In particular, Flush failure is not Set failure.
    set()?;
    retry_clipboard_open(
        || {
            if !owns_clipboard() {
                return Err(windows::core::Error::from_hresult(E_ABORT));
            }
            flush()
        },
        unchanged,
        sleep,
    )
}

pub fn copy_to_clipboard(paths: Vec<String>) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    let object = shell_object(&paths)?;
    let sequence = std::cell::Cell::new(0);
    // Flush renders formats before this thread and its data object go away.
    set_and_flush_clipboard(
        || {
            unsafe { OleSetClipboard(&object) }?;
            sequence.set(unsafe { GetClipboardSequenceNumber() });
            Ok(())
        },
        || unsafe { OleFlushClipboard() },
        || current_clipboard_object(&object),
        || same_clipboard_sequence(sequence.get(), unsafe { GetClipboardSequenceNumber() }),
        std::thread::sleep,
    )
    .map_err(native_error)
}

fn get_clipboard_object() -> Result<IDataObject, Fault> {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    retry_clipboard_open(
        || unsafe { OleGetClipboard() },
        || same_clipboard_sequence(sequence, unsafe { GetClipboardSequenceNumber() }),
        std::thread::sleep,
    )
    .map_err(native_error)
}

fn format(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

// CF_HDROP is untrusted cross-process data. Bound and parse the Unicode DROPFILES
// buffer ourselves instead of allowing an unterminated list to escape GlobalSize.
fn parse_drop_files(bytes: &[u8]) -> Result<Vec<String>, Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    if bytes.len() < 24 || bytes.len() > MAX_PATH_BYTES * 2 + 64 {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let wide = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    if offset < 20 || !offset.is_multiple_of(2) || offset > bytes.len() - 4 || wide == 0 {
        return Err(invalid());
    }
    let mut paths = Vec::new();
    let mut name = Vec::new();
    for unit in bytes[offset..].as_chunks::<2>().0 {
        let unit = u16::from_le_bytes([unit[0], unit[1]]);
        if unit == 0 {
            if name.is_empty() {
                return normalize_import_paths(paths);
            }
            paths.push(String::from_utf16(&name).map_err(|_| invalid())?);
            name.clear();
        } else {
            name.push(unit);
            if name.len() > 32768 {
                return Err(invalid());
            }
        }
    }
    Err(invalid())
}

fn require_copy_effect(effect: &STGMEDIUM) -> Result<(), Fault> {
    if effect.tymed != TYMED_HGLOBAL.0 as u32 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let handle = unsafe { effect.u.hGlobal };
    if unsafe { GlobalSize(handle) } < 4 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let value = unsafe { GlobalLock(handle) };
    if value.is_null() {
        return Err(Fault::new(FaultCode::IoError));
    }
    let effect = unsafe { ptr::read_unaligned(value.cast::<u32>()) };
    let _ = unsafe { GlobalUnlock(handle) };
    if effect == DROPEFFECT_COPY.0 {
        Ok(())
    } else {
        Err(Fault::new(FaultCode::InvalidRequest))
    }
}

fn read_paths(object: &IDataObject) -> Result<Vec<String>, Fault> {
    // Missing preference is allowed; an unreadable, move or link preference is
    // not permission to copy. External moves require a completion protocol.
    let effect_format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if effect_format == 0 {
        return Err(Fault::new(FaultCode::IoError));
    }
    match unsafe { object.GetData(&format(effect_format as u16)) } {
        Ok(mut effect) => {
            let result = require_copy_effect(&effect);
            unsafe { ReleaseStgMedium(&mut effect) };
            result?;
        }
        Err(error) if error.code() == DV_E_FORMATETC => {}
        Err(error) => return Err(native_error(error)),
    }
    read_file_paths(object)
}

fn read_file_paths(object: &IDataObject) -> Result<Vec<String>, Fault> {
    let mut data = unsafe { object.GetData(&format(15)) }
        .map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
    let result = (|| {
        if data.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let handle = unsafe { data.u.hGlobal };
        let size = unsafe { GlobalSize(handle) };
        if !(24..=MAX_PATH_BYTES * 2 + 64).contains(&size) {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let value = unsafe { GlobalLock(handle) };
        if value.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        // SAFETY: locked storage is readable for the checked GlobalSize bytes.
        let result =
            parse_drop_files(unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) });
        let _ = unsafe { GlobalUnlock(handle) };
        result
    })();
    // SAFETY: GetData transfers ownership of exactly this medium; release on every outcome.
    unsafe { ReleaseStgMedium(&mut data) };
    result
}

/// Stream native CF_HDROP names into a bounded capture manifest.
pub fn visit_clipboard_paths(visit: impl FnMut(String) -> Result<(), Fault>) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    let object = get_clipboard_object()?;
    visit_object_paths(&object, visit)
}

fn visit_object_paths(
    object: &IDataObject,
    mut visit: impl FnMut(String) -> Result<(), Fault>,
) -> Result<(), Fault> {
    let effect_format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if effect_format == 0 {
        return Err(Fault::new(FaultCode::IoError));
    }
    match unsafe { object.GetData(&format(effect_format as u16)) } {
        Ok(mut effect) => {
            let result = require_copy_effect(&effect);
            unsafe { ReleaseStgMedium(&mut effect) };
            result?;
        }
        Err(error) if error.code() == DV_E_FORMATETC => {}
        Err(error) => return Err(native_error(error)),
    }
    let mut data = unsafe { object.GetData(&format(15)) }.map_err(native_error)?;
    let result = (|| {
        if data.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let handle = unsafe { data.u.hGlobal };
        let size = unsafe { GlobalSize(handle) };
        if !(24..=MAX_PATH_BYTES * 2 + 64).contains(&size) {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        // SAFETY: checked GlobalSize, storage remains locked through parsing.
        let result = visit_drop_files(
            unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) },
            &mut visit,
        );
        let _ = unsafe { GlobalUnlock(handle) };
        result
    })();
    unsafe { ReleaseStgMedium(&mut data) };
    result
}
fn visit_drop_files(
    bytes: &[u8],
    visit: &mut impl FnMut(String) -> Result<(), Fault>,
) -> Result<(), Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    if !(24..=MAX_PATH_BYTES * 2 + 64).contains(&bytes.len()) {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let wide = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    if offset < 20 || !offset.is_multiple_of(2) || offset > bytes.len() - 4 || wide == 0 {
        return Err(invalid());
    }
    let mut name = Vec::new();
    let mut count = 0;
    for unit in bytes[offset..].as_chunks::<2>().0 {
        let unit = u16::from_le_bytes(*unit);
        if unit == 0 {
            if name.is_empty() {
                return if count > 0 { Ok(()) } else { Err(invalid()) };
            }
            let path = String::from_utf16(&name).map_err(|_| invalid())?;
            let path = normalize_import_paths(vec![path])?.remove(0);
            visit(path)?;
            count += 1;
            name.clear();
        } else {
            name.push(unit);
            if name.len() > 32768 {
                return Err(invalid());
            }
        }
    }
    Err(invalid())
}

pub fn clipboard_paths() -> Result<Vec<String>, Fault> {
    let _ole = OleApartment::new()?;
    let object = get_clipboard_object()?;
    read_paths(&object)
}

/// Read only the window-station change counter, never clipboard content.
pub fn clipboard_sequence() -> Result<u32, Fault> {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    if sequence == 0 {
        return Err(Fault::new(FaultCode::IoError));
    }
    Ok(sequence)
}

#[implement(IDropSource)]
struct CopySource;
impl IDropSource_Impl for CopySource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & 1 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Run on the window's UI thread while the user's left button is held.
pub fn drag_copy(paths: Vec<String>) -> Result<bool, Fault> {
    let _ole = OleApartment::new()?;
    let object = shell_object(&paths)?;
    let source: IDropSource = CopySource.into();
    let mut effect = DROPEFFECT(0);
    let result = unsafe { DoDragDrop(&object, &source, DROPEFFECT_COPY, &mut effect) };
    if result == DRAGDROP_S_CANCEL {
        return Ok(false);
    }
    result.ok().map_err(native_error)?;
    Ok(effect == DROPEFFECT_COPY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::{
        Com::STGMEDIUM_0,
        Memory::{GMEM_MOVEABLE, GlobalAlloc},
    };

    fn clipboard_error(code: HRESULT) -> windows::core::Error {
        windows::core::Error::from_hresult(code)
    }

    #[test]
    fn clipboard_open_retry_exhaustion_has_five_attempts_and_150_ms_wait() {
        let mut attempts = 0;
        let mut waits = Vec::new();
        let result = retry_clipboard_open::<()>(
            || {
                attempts += 1;
                Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
            },
            || true,
            |delay| waits.push(delay.as_millis()),
        );
        assert_eq!(result.unwrap_err().code(), CLIPBRD_E_CANT_OPEN);
        assert_eq!(attempts, 5);
        assert_eq!(waits, [10, 20, 40, 80]);
        assert_eq!(waits.iter().sum::<u128>(), 150);
    }

    #[test]
    fn clipboard_retry_does_not_retry_other_errors_or_success() {
        use windows::Win32::Foundation::{CLIPBRD_E_CANT_CLOSE, CLIPBRD_E_CANT_SET, E_FAIL};
        for code in [CLIPBRD_E_CANT_CLOSE, CLIPBRD_E_CANT_SET, E_FAIL, E_ABORT] {
            let mut attempts = 0;
            let result = retry_clipboard_open::<()>(
                || {
                    attempts += 1;
                    Err(clipboard_error(code))
                },
                || panic!("permanent error must not check retry permission"),
                |_| panic!("permanent error must not sleep"),
            );
            assert_eq!(result.unwrap_err().code(), code);
            assert_eq!(attempts, 1);
        }
        assert_eq!(
            retry_clipboard_open(|| Ok(7), || panic!("no retry"), |_| panic!("no wait")).unwrap(),
            7
        );
    }

    #[test]
    fn clipboard_retry_stops_on_unknown_or_changed_sequence() {
        for (expected, current) in [(0, 0), (0, 12), (12, 0), (12, 13)] {
            let mut attempts = 0;
            let result = retry_clipboard_open::<()>(
                || {
                    attempts += 1;
                    Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
                },
                || same_clipboard_sequence(expected, current),
                |_| panic!("changed or unknown sequence must not sleep"),
            );
            assert_eq!(result.unwrap_err().code(), CLIPBRD_E_CANT_OPEN);
            assert_eq!(attempts, 1);
        }
        assert!(same_clipboard_sequence(12, 12));
        let current = std::cell::Cell::new(12);
        let mut attempts = 0;
        let mut waits = 0;
        let result = retry_clipboard_open::<()>(
            || {
                attempts += 1;
                Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
            },
            || same_clipboard_sequence(12, current.get()),
            |_| {
                waits += 1;
                current.set(13);
            },
        );
        assert!(result.is_err());
        assert_eq!((attempts, waits), (1, 1));
    }

    #[test]
    fn clipboard_flush_retry_never_repeats_successful_set() {
        for succeeds in [false, true] {
            let mut sets = 0;
            let mut flushes = 0;
            let mut waits = 0;
            let result = set_and_flush_clipboard(
                || {
                    sets += 1;
                    Ok(())
                },
                || {
                    flushes += 1;
                    if succeeds && flushes == 3 {
                        Ok(())
                    } else {
                        Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
                    }
                },
                || true,
                || true,
                |_| waits += 1,
            );
            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(sets, 1);
            assert_eq!(flushes, if succeeds { 3 } else { 5 });
            assert_eq!(waits, flushes - 1);
        }
    }

    #[test]
    fn clipboard_set_failure_never_retries_or_flushes() {
        let mut sets = 0;
        let result = set_and_flush_clipboard(
            || {
                sets += 1;
                Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
            },
            || panic!("failed set must not flush"),
            || panic!("failed set must not check ownership"),
            || panic!("failed set must not retry"),
            |_| panic!("failed set must not sleep"),
        );
        assert_eq!(result.unwrap_err().code(), CLIPBRD_E_CANT_OPEN);
        assert_eq!(sets, 1);
    }

    #[test]
    fn clipboard_flush_stops_after_ownership_loss_including_s_false() {
        use windows::Win32::Foundation::S_FALSE;
        assert!(clipboard_is_current(S_OK));
        assert!(!clipboard_is_current(S_FALSE));
        assert!(!clipboard_is_current(E_ABORT));
        for lost_before_flush in [false, true] {
            let owns = std::cell::Cell::new(!lost_before_flush);
            let mut sets = 0;
            let mut flushes = 0;
            let mut waits = 0;
            let result = set_and_flush_clipboard(
                || {
                    sets += 1;
                    Ok(())
                },
                || {
                    flushes += 1;
                    Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
                },
                || owns.get(),
                || true,
                |_| {
                    waits += 1;
                    owns.set(false);
                },
            );
            assert_eq!(result.unwrap_err().code(), E_ABORT);
            assert_eq!(sets, 1);
            assert_eq!(flushes, usize::from(!lost_before_flush));
            assert_eq!(waits, flushes);
        }
    }

    #[test]
    fn clipboard_acquisition_retries_do_not_replay_path_visitor() {
        let mut bytes = vec![0u8; 20];
        bytes[..4].copy_from_slice(&20u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for unit in r"C:\fixture\one.txt".encode_utf16().chain([0, 0]) {
            bytes.extend(unit.to_le_bytes());
        }
        let mut acquisitions = 0;
        let acquired = retry_clipboard_open(
            || {
                acquisitions += 1;
                if acquisitions < 3 {
                    Err(clipboard_error(CLIPBRD_E_CANT_OPEN))
                } else {
                    Ok(&bytes)
                }
            },
            || true,
            |_| {},
        )
        .unwrap();
        let mut visits = 0;
        let result = visit_drop_files(acquired, &mut |_| {
            visits += 1;
            Err(Fault::new(FaultCode::IoError))
        });
        assert!(result.is_err());
        assert_eq!(acquisitions, 3);
        assert_eq!(visits, 1);
    }

    #[test]
    fn drop_stream_accepts_more_than_1024_names_without_clipboard_access() {
        let mut bytes = vec![0u8; 20];
        bytes[..4].copy_from_slice(&20u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for n in 0..2051 {
            for unit in format!(r"C:\fixture\file-{n}.txt")
                .encode_utf16()
                .chain([0])
            {
                bytes.extend(unit.to_le_bytes());
            }
        }
        bytes.extend(0u16.to_le_bytes());
        let mut count = 0;
        visit_drop_files(&bytes, &mut |path| {
            assert_eq!(path, format!(r"C:\fixture\file-{count}.txt"));
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 2051);
        assert!(visit_drop_files(&bytes[..bytes.len() - 2], &mut |_| Ok(())).is_err());
    }
    #[test]
    fn unc_selection_does_not_collapse_case_sensitive_names() {
        assert!(
            validate_paths(&[
                r"\\wsl.localhost\Ubuntu\tmp\Foo".into(),
                r"\\wsl.localhost\Ubuntu\tmp\foo".into()
            ])
            .is_ok()
        );
    }
    #[test]
    fn shell_preference_rejects_move_link_and_invalid_native_media() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            assert!(require_copy_effect(&STGMEDIUM::default()).is_err());
            for effect in [1u32, 2, 4, 3, 0, u32::MAX] {
                let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, 4) }.unwrap();
                let value = unsafe { GlobalLock(handle) };
                assert!(!value.is_null());
                unsafe { ptr::write_unaligned(value.cast::<u32>(), effect) };
                let _ = unsafe { GlobalUnlock(handle) };
                let mut medium = STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: handle },
                    ..Default::default()
                };
                let result = require_copy_effect(&medium);
                unsafe { ReleaseStgMedium(&mut medium) };
                if effect == 1 {
                    assert!(result.is_ok());
                } else {
                    assert!(result.is_err(), "effect {effect}");
                }
            }
        })
        .join()
        .unwrap();
    }
    #[test]
    fn drop_files_rejects_unterminated_invalid_and_oversized_buffers() {
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(&20u32.to_le_bytes());
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        for unit in "C:\\한글.txt".encode_utf16().chain([0, 0]) {
            data.extend(unit.to_le_bytes());
        }
        assert_eq!(parse_drop_files(&data).unwrap(), vec!["C:\\한글.txt"]);
        assert!(parse_drop_files(&data[..data.len() - 2]).is_err());
        let mut malformed = data.clone();
        malformed[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_drop_files(&malformed).is_err());
        malformed = data.clone();
        malformed[16..20].fill(0);
        assert!(parse_drop_files(&malformed).is_err());
        assert!(parse_drop_files(&vec![0; MAX_PATH_BYTES * 2 + 65]).is_err());
    }
    #[test]
    fn drop_files_normalizes_verbatim_wsl_without_decoding_names_or_devices() {
        let paths = [
            r"\\?\UNC\wsl.localhost\Ubuntu\home\example-user\한글 space.txt",
            r"\\wsl$\Ubuntu\home\example-user\literal%20.txt",
        ];
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(&20u32.to_le_bytes());
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        for path in paths {
            for unit in path.encode_utf16().chain([0]) {
                data.extend(unit.to_le_bytes());
            }
        }
        data.extend(0u16.to_le_bytes());
        assert_eq!(
            parse_drop_files(&data).unwrap(),
            [
                r"\\wsl.localhost\Ubuntu\home\example-user\한글 space.txt",
                paths[1]
            ]
        );
        for bad in [
            r"\\?\GLOBALROOT\Device\x",
            r"\\.\PhysicalDrive0",
            r"\\?\UNC\wsl$\Ubuntu\a\..\b",
            "file://wsl.localhost/Ubuntu/a",
        ] {
            assert!(normalize_import_paths(vec![bad.into()]).is_err(), "{bad}");
        }
        assert!(
            normalize_import_paths(vec![
                r"\\?\UNC\wsl$\Ubuntu\a".into(),
                r"\\wsl$\Ubuntu\a".into()
            ])
            .is_err()
        );
    }
    #[test]
    fn rejects_empty_duplicate_relative_and_excessive_inputs() {
        for paths in [
            vec![],
            vec!["relative.txt".into()],
            vec!["C:\\one".into(), "c:\\ONE".into()],
            vec!["C:\\one".into(); 1025],
        ] {
            assert!(validate_paths(&paths).is_err());
        }
    }
    #[test]
    fn shell_file_object_roundtrip_keeps_unicode_paths_without_using_clipboard() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            let root = tempfile::tempdir().unwrap();
            let file = root.path().join("한글 공백.txt");
            std::fs::write(&file, b"fixture").unwrap();
            let second = root.path().join("두 번째.txt");
            std::fs::write(&second, b"second").unwrap();
            let paths = vec![
                file.to_str().unwrap().to_owned(),
                second.to_str().unwrap().to_owned(),
            ];
            let selection = ShellSelection::new(&paths).unwrap();
            assert_eq!(selection.children().len(), 2);
            let object = shell_object(&paths).unwrap();
            assert_eq!(read_paths(&object).unwrap(), paths);
            let other = tempfile::tempdir().unwrap();
            let outside = other.path().join("other.txt");
            std::fs::write(&outside, b"other").unwrap();
            assert!(
                ShellSelection::new(&[paths[0].clone(), outside.to_str().unwrap().to_owned()])
                    .is_err()
            );
        })
        .join()
        .unwrap();
    }

    #[test]
    fn shell_file_object_streams_129_and_1025_unicode_paths_without_clipboard() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            for count in [129, 1025] {
                let root = tempfile::tempdir().unwrap();
                let paths: Vec<_> = (0..count)
                    .map(|index| {
                        let path = root.path().join(format!("한글 파일-{index}.txt"));
                        std::fs::write(&path, b"owned shell roundtrip fixture").unwrap();
                        path.to_str().unwrap().to_owned()
                    })
                    .collect();
                let object = shell_object(&paths).unwrap();
                assert_eq!(read_paths(&object).unwrap(), paths);
                let mut streamed = Vec::new();
                visit_object_paths(&object, |path| {
                    streamed.push(path);
                    Ok(())
                })
                .unwrap();
                assert_eq!(streamed, paths);
            }
        })
        .join()
        .unwrap();
    }
}
