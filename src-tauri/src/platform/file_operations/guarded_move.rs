//! Test-only executor: ordinary single-link files, same volume, no replacement.
//! Handles remain owned through operation-result persistence and OLE notification.
use super::capture_source;
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    mem::size_of,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileInformationByHandle, GetVolumeInformationByHandleW,
};
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    },
    Win32::System::{
        IO::IO_STATUS_BLOCK,
        Threading::{INFINITE, WaitForSingleObject},
    },
};

fn invalid() -> Fault {
    Fault::new(FaultCode::StaleRequest)
}

fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, Fault> {
    let mut value = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: borrowed live File; exclusive initialized output; no retained pointers.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut value) } == 0 {
        return Err(Fault::from(std::io::Error::last_os_error()));
    }
    if value.dwFileAttributes & 0x400 != 0 {
        return Err(invalid());
    }
    Ok(value)
}
fn matches(file: &File, expected: &OperationSource) -> Result<(), Fault> {
    let value = info(file)?;
    if value.dwVolumeSerialNumber != expected.volume
        || ((u64::from(value.nFileIndexHigh) << 32) | u64::from(value.nFileIndexLow))
            != expected.file_index
        || (value.dwFileAttributes & 0x10 != 0) != (expected.kind == EntryKind::Folder)
    {
        return Err(invalid());
    }
    Ok(())
}
fn open(path: &str, access: u32, sharing: u32) -> Result<File, Fault> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(Fault::from)
}
fn absent(path: &Path) -> Result<(), Fault> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(invalid()), // access denied is not absence
    }
}

pub(crate) struct GuardedMove {
    plan: OperationPlan,
    files: Vec<File>,
    targets: Vec<String>,
    // Root-to-leaf ancestors of sources, target and both journals, never released.
    // Deny deletion/reparenting. Destination directory permits write sharing:
    // Native rename internally opens it for FILE_ADD_FILE. The target is the
    // pinned directory HANDLE plus one leaf, never an absolute destination path.
    directories: Vec<(File, OperationSource)>,
    destination_index: usize,
}
impl GuardedMove {
    pub(crate) fn acquire(plan: OperationPlan, journals: &[PathBuf]) -> Result<Self, Fault> {
        plan.validate()?;
        let PreparedAction::Move {
            sources,
            destination,
        } = &plan.action
        else {
            return Err(invalid());
        };
        if sources.len() > 128 {
            return Err(invalid());
        }
        let mut seen = HashSet::new();
        let mut directories = Vec::new();
        let roots = sources
            .iter()
            .map(|s| Path::new(&s.path).parent().ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        for path in roots
            .into_iter()
            .chain([Path::new(&destination.path)])
            .chain(journals.iter().map(PathBuf::as_path))
        {
            let mut ancestors: Vec<_> = path.ancestors().collect();
            ancestors.reverse();
            for ancestor in ancestors {
                let text = ancestor.to_str().ok_or_else(invalid)?;
                if !seen.insert(text.to_lowercase()) {
                    continue;
                }
                if directories.len() >= 4096 {
                    return Err(Fault::new(FaultCode::ResourceLimit));
                }
                let captured = capture_source(text)?;
                if captured.kind != EntryKind::Folder {
                    return Err(invalid());
                }
                // Attribute-only handles do not participate in sharing checks.
                // Actual read access makes Windows enforce the sharing contract.
                let sharing = if captured.path.eq_ignore_ascii_case(&destination.path) {
                    FILE_SHARE_READ | FILE_SHARE_WRITE
                } else {
                    FILE_SHARE_READ
                };
                let file = open(
                    &captured.path,
                    FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
                    sharing,
                )?;
                matches(&file, &captured)?;
                if capture_source(&captured.path)? != captured {
                    return Err(invalid());
                }
                directories.push((file, captured));
            }
        }
        if capture_source(&destination.path)? != *destination {
            return Err(invalid());
        }
        let destination_index = directories
            .iter()
            .position(|(_, value)| value == destination)
            .ok_or_else(invalid)?;
        let mut filesystem = [0u16; 32];
        // Restrict this proof to fixed local NTFS; other filesystems fail closed.
        let volume_ok = unsafe {
            GetVolumeInformationByHandleW(
                directories[destination_index].0.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        };
        if volume_ok == 0 || filesystem[..5] != [78, 84, 70, 83, 0] {
            return Err(invalid());
        }
        let mut files = Vec::new();
        let mut targets = Vec::new();
        let mut names = HashSet::new();
        for source in sources {
            if source.kind != EntryKind::File || source.volume != destination.volume {
                return Err(invalid());
            }
            let file = open(&source.path, DELETE | FILE_READ_ATTRIBUTES, FILE_SHARE_READ)?;
            matches(&file, source)?;
            if info(&file)?.nNumberOfLinks != 1 || capture_source(&source.path)? != *source {
                return Err(invalid());
            }
            let name = Path::new(&source.path)
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(invalid)?;
            validate_leaf_name(name)?;
            if !names.insert(name.to_lowercase()) {
                return Err(invalid());
            }
            let target = Path::new(&destination.path).join(name);
            absent(&target)?;
            targets.push(target.to_str().ok_or_else(invalid)?.to_owned());
            files.push(file);
        }
        Ok(Self {
            plan,
            files,
            targets,
            directories,
            destination_index,
        })
    }
    pub(crate) fn targets(&self) -> Vec<String> {
        self.targets.clone()
    }

    fn move_one(&self, index: usize, probe: &Probe) -> Result<(), Fault> {
        let PreparedAction::Move { sources, .. } = &self.plan.action else {
            return Err(invalid());
        };
        for (file, directory) in &self.directories {
            matches(file, directory)?;
            if capture_source(&directory.path)? != *directory {
                return Err(invalid());
            }
        }
        matches(&self.files[index], &sources[index])?;
        if capture_source(&sources[index].path)? != sources[index] {
            return Err(invalid());
        }
        absent(Path::new(&self.targets[index]))?;
        let leaf = Path::new(&self.targets[index])
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(invalid)?;
        validate_leaf_name(leaf)?;
        let name: Vec<u16> = leaf.encode_utf16().chain([0]).collect();
        let length = size_of::<FILE_RENAME_INFORMATION>() + name.len() * 2;
        // Properly aligned variable tail; native structure uses the documented
        // RootDirectory-relative simple-name contract, not a Win32 absolute path.
        let mut storage = vec![0u64; length.div_ceil(8)];
        let pointer = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        probe(Stage::BeforeRename(index)); // deterministic last-check-to-syscall race
        // SAFETY: aligned zeroed allocation includes complete UTF-16 tail. Zero
        // ReplaceIfExists means atomic no-clobber; same live DELETE handle does
        // the rename without reopening or relaxing its sharing protection.
        let mut io = IO_STATUS_BLOCK::default();
        let status = unsafe {
            (*pointer).RootDirectory = self.directories[self.destination_index].0.as_raw_handle();
            (*pointer).FileNameLength = ((name.len() - 1) * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (*pointer).FileName.as_mut_ptr(),
                name.len(),
            );
            let status = NtSetInformationFile(
                self.files[index].as_raw_handle(),
                &mut io,
                pointer.cast(),
                length as u32,
                FileRenameInformation,
            );
            // These are synchronous File handles. Keep the buffer and handles
            // alive even if a driver unexpectedly reports pending completion.
            if status == 0x103 {
                WaitForSingleObject(self.files[index].as_raw_handle(), INFINITE);
                io.Anonymous.Status
            } else {
                status
            }
        };
        if status != 0 {
            eprintln!("guarded native rename NTSTATUS={status:#010x}");
            return Err(Fault::new(FaultCode::IoError));
        }
        matches(&self.files[index], &sources[index])?;
        let arrived = capture_source(&self.targets[index])?;
        if arrived.volume != sources[index].volume
            || arrived.file_index != sources[index].file_index
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(crate) fn execute(
        &self,
        cancel: &AtomicBool,
        expired: &AtomicBool,
        probe: &Probe,
    ) -> OperationReport {
        let PreparedAction::Move { sources, .. } = &self.plan.action else {
            unreachable!()
        };
        let mut items = Vec::new();
        let mut counts = OutcomeCounts::default();
        let mut stopped = false;
        let mut cancelled = false;
        for (index, source) in sources.iter().enumerate() {
            probe(Stage::BeforeItem(index));
            cancelled |= cancel.load(Ordering::Acquire) || expired.load(Ordering::Acquire);
            let (status, hresult) = if cancelled || stopped {
                counts.skipped += 1;
                (ItemStatus::Skipped, 0)
            } else {
                match self.move_one(index, probe) {
                    Ok(()) => {
                        counts.changed += 1;
                        (ItemStatus::Changed, 0)
                    }
                    Err(_) => {
                        stopped = true;
                        // A native failure may follow a successful rename. Observe
                        // both names; do not infer unchanged from an HRESULT alone.
                        let current = capture_source(&source.path).ok();
                        if current.as_ref() == Some(source)
                            && absent(Path::new(&self.targets[index])).is_ok()
                        {
                            counts.failed += 1;
                            (ItemStatus::Failed, -1)
                        } else {
                            counts.unknown += 1;
                            (ItemStatus::Unknown, -1)
                        }
                    }
                }
            };
            items.push(ItemOutcome {
                source: source.path.clone(),
                destination: Some(self.targets[index].clone()),
                status,
                hresult,
            });
            probe(Stage::AfterItem(index));
        }
        OperationReport {
            failure: None,
            counts,
            items,
            user_skipped: vec![],
            user_decisions_only: None,
            evidence: FinishEvidence {
                perform_succeeded: !stopped && !cancelled,
                aborted: Some(cancelled),
                outcomes_complete: true,
                cancellation_confirmed: cancelled,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Armed,
    BeforeItem(usize),
    BeforeRename(usize),
    AfterItem(usize),
    BeforeResult,
    AfterResult,
    AfterNotice,
}
pub(crate) type Probe = Arc<dyn Fn(Stage) + Send + Sync>;
