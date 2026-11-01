//! Same WSL namespace/volume only: rename the captured object, never copy/delete.
//! A failed native call is never retried or converted into cross-volume copying.
use super::{NativeProgress, capture_source, identity::recheck, preflight::absent};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    mem::size_of,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    },
    Win32::{
        Storage::FileSystem::*,
        System::{
            IO::IO_STATUS_BLOCK,
            Threading::{INFINITE, WaitForSingleObject},
        },
    },
};

fn share(path: &str) -> Option<&str> {
    let mut parts = path.strip_prefix(r"\\")?.split('\\');
    let host = parts.next()?;
    if !host.eq_ignore_ascii_case("wsl.localhost") && !host.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    parts.next().filter(|part| !part.is_empty())
}
pub(super) fn compatible(sources: &[OperationSource], destination: &OperationSource) -> bool {
    let Some(distro) = share(&destination.path) else {
        return false;
    };
    !sources.is_empty()
        && sources.iter().all(|source| {
            source.volume == destination.volume
                && share(&source.path).is_some_and(|s| s.eq_ignore_ascii_case(distro))
        })
}
pub(super) fn check(
    sources: &[OperationSource],
    destination: &OperationSource,
) -> Result<(), Fault> {
    if !compatible(sources, destination) {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    recheck(destination)?;
    let mut names = HashSet::new();
    for source in sources {
        recheck(source)?;
        let name = leaf(source)?;
        validate_leaf_name(name)?;
        if !names.insert(name.to_lowercase()) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        absent(&Path::new(&destination.path).join(name))?;
    }
    // Reject self/descendant moves and overlapping selected roots by identity,
    // including aliases. No recursive traversal of folder contents is required.
    let mut checked = HashSet::new();
    for root in std::iter::once(Path::new(&destination.path))
        .chain(sources.iter().filter_map(|s| Path::new(&s.path).parent()))
    {
        for path in root.ancestors() {
            if !checked.insert(path) {
                continue;
            }
            let ancestor = capture_source(path.to_str().ok_or_else(invalid)?)?;
            if sources
                .iter()
                .any(|s| s.kind == EntryKind::Folder && same_id(s, &ancestor))
            {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
        }
    }
    Ok(())
}
fn invalid() -> Fault {
    Fault::new(FaultCode::StaleRequest)
}
fn same_id(a: &OperationSource, b: &OperationSource) -> bool {
    (a.volume, a.file_index, a.kind) == (b.volume, b.file_index, b.kind)
}
fn leaf(source: &OperationSource) -> Result<&str, Fault> {
    Path::new(&source.path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(invalid)
}
fn open(source: &OperationSource, deleting: bool) -> Result<File, Fault> {
    let file = OpenOptions::new()
        .access_mode(
            FILE_READ_ATTRIBUTES
                | if deleting {
                    DELETE
                } else {
                    FILE_LIST_DIRECTORY
                },
        )
        .share_mode(FILE_SHARE_READ | if deleting { 0 } else { FILE_SHARE_WRITE })
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&source.path)
        .map_err(Fault::from)?;
    let info = super::guarded_io::info(&file)?;
    if (
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ) != (source.volume, source.file_index)
        || (info.dwFileAttributes & 0x10 != 0) != (source.kind == EntryKind::Folder)
    {
        return Err(invalid());
    }
    recheck(source)?;
    Ok(file)
}
fn rename(file: &File, parent: &File, name: &str) -> Result<(), Fault> {
    validate_leaf_name(name)?;
    let name: Vec<u16> = name.encode_utf16().collect();
    let length = size_of::<FILE_RENAME_INFORMATION>() + name.len() * 2;
    let mut storage = vec![0u64; length.div_ceil(8)];
    let pointer = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let mut io = IO_STATUS_BLOCK::default();
    // SAFETY: aligned initialized buffer and UTF-16 tail; live synchronous handles.
    // Zero ReplaceIfExists gives atomic no-clobber. RootDirectory + one leaf cannot
    // follow a replacement target path. Rename cannot cross a volume; no fallback.
    let status = unsafe {
        (*pointer).RootDirectory = parent.as_raw_handle();
        (*pointer).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*pointer).FileName.as_mut_ptr(), name.len());
        let status = NtSetInformationFile(
            file.as_raw_handle(),
            &mut io,
            pointer.cast(),
            length as u32,
            FileRenameInformation,
        );
        if status == 0x103 {
            WaitForSingleObject(file.as_raw_handle(), INFINITE);
            io.Anonymous.Status
        } else {
            status
        }
    };
    if status == 0 {
        Ok(())
    } else {
        Err(Fault::new(FaultCode::IoError))
    }
}
pub(super) fn execute(
    sources: &[OperationSource],
    destination: &OperationSource,
    cancel: &AtomicBool,
    progress: &NativeProgress,
) -> Result<OperationReport, Fault> {
    execute_with_probe(sources, destination, cancel, progress, &|_, _| Ok(()))
}
// Deterministic test seams surround mutation; production passes a no-op.
pub(super) fn execute_with_probe(
    sources: &[OperationSource],
    destination: &OperationSource,
    cancel: &AtomicBool,
    progress: &NativeProgress,
    probe: &dyn Fn(usize, bool) -> Result<(), Fault>,
) -> Result<OperationReport, Fault> {
    check(sources, destination)?;
    let parent = open(destination, false)?;
    let mut report = OperationReport {
        failure: None,
        counts: OutcomeCounts::default(),
        items: vec![],
        user_skipped: vec![],
        user_decisions_only: None,
        evidence: FinishEvidence {
            perform_succeeded: true,
            aborted: Some(false),
            outcomes_complete: true,
            cancellation_confirmed: false,
        },
    };
    let mut stopped = false;
    for (index, source) in sources.iter().enumerate() {
        let target = Path::new(&destination.path).join(leaf(source)?);
        let target = target.to_str().ok_or_else(invalid)?.to_owned();
        let cancelled = cancel.load(Ordering::Acquire);
        let status = if stopped || cancelled {
            report.counts.skipped += 1;
            if cancelled {
                report.evidence.cancellation_confirmed = true;
                report.evidence.aborted = Some(true);
            }
            report.evidence.perform_succeeded = false;
            ItemStatus::Skipped
        } else {
            let moved = (|| {
                recheck(destination)?;
                let file = open(source, true)?;
                absent(Path::new(&target))?;
                probe(index, false)?;
                if cancel.load(Ordering::Acquire) {
                    return Err(invalid());
                }
                rename(&file, &parent, leaf(source)?)?;
                probe(index, true)?;
                recheck(destination)?;
                let arrived = capture_source(&target)?;
                if !same_id(source, &arrived) {
                    return Err(invalid());
                }
                Ok::<_, Fault>(())
            })();
            if moved.is_ok() {
                report.counts.changed += 1;
                ItemStatus::Changed
            } else {
                stopped = true;
                report.evidence.perform_succeeded = false;
                // Preserve uncertainty even if rename succeeded but observation failed.
                // Never replay or delete either path in response to an ambiguous result.
                if capture_source(&source.path)
                    .as_ref()
                    .is_ok_and(|s| s == source)
                {
                    report.counts.failed += 1;
                    ItemStatus::Failed
                } else {
                    report.counts.unknown += 1;
                    ItemStatus::Unknown
                }
            }
        };
        report.items.push(ItemOutcome {
            source: source.path.clone(),
            destination: Some(target),
            status,
            hresult: if status == ItemStatus::Changed { 0 } else { -1 },
        });
        progress.update(sources.len() as u32, index as u32 + 1);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_distro_and_volume_are_both_required() {
        let source = OperationSource {
            path: r"\\wsl.localhost\Ubuntu\home\test\a".into(),
            kind: EntryKind::File,
            volume: 7,
            file_index: 8,
        };
        let mut destination = OperationSource {
            path: r"\\wsl$\Ubuntu\home\test\b".into(),
            kind: EntryKind::Folder,
            volume: 7,
            file_index: 9,
        };
        assert!(compatible(std::slice::from_ref(&source), &destination));
        for path in [r"\\server\Ubuntu\b", r"\\wsl.localhost\Debian\b", r"C:\b"] {
            destination.path = path.into();
            assert!(!compatible(std::slice::from_ref(&source), &destination));
        }
        destination.path = r"\\wsl.localhost\Ubuntu\b".into();
        destination.volume = 42;
        assert!(!compatible(&[source], &destination));
    }
}
