//! Bounded observations and separately invoked new-file staging; no replacement.
#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod layout_tests;
mod native;
pub mod staging;
#[cfg(test)]
mod tests;

use crate::domain::{
    EntryKind,
    operations::recovery::{FileObservation, ObservedContent, RecoveryAssessment, RecoveryRecord},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

/// Conservative total content-input ceiling; not a process-memory budget.
pub const MAX_OBSERVATION_BYTES: u64 = 16 * 1024 * 1024;
const MAX_OBSERVATION_HANDLES: usize = 128;

pub(crate) fn guard_recovery_directory(directory: &std::path::Path) -> io::Result<Vec<File>> {
    let leaf = directory.join("journal.lock");
    native::ancestors(
        leaf.to_str().ok_or_else(native::invalid)?,
        MAX_OBSERVATION_HANDLES,
    )
}

/// Observes recorded paths on Windows fixed drives without changing their contents.
/// Inaccessible, unsupported, changed or over-budget evidence stays unknown.
/// Reads are sequential; the result never authorizes replacement, recovery or deletion.
pub fn observe_replacement(record: &RecoveryRecord) -> RecoveryAssessment {
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    record.observe_with(|path| collector.observe(path))
}

struct Collector {
    remaining: u64,
    held: Vec<File>,
}

impl Collector {
    fn observe(&mut self, path: &str) -> FileObservation {
        self.try_observe(path)
            .unwrap_or(FileObservation::Inaccessible)
    }

    fn try_observe(&mut self, path: &str) -> io::Result<FileObservation> {
        let available = MAX_OBSERVATION_HANDLES
            .checked_sub(self.held.len() + 1)
            .ok_or_else(native::invalid)?;
        let mut guards = native::ancestors(path, available)?;
        self.held.append(&mut guards);
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(FileObservation::Absent);
            }
            Err(error) => return Err(error),
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(native::invalid());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        let mut file = options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let before = native::identity(&file)?;
        let directory = before.attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if directory != metadata.is_dir() || (!directory && before.links != 1) {
            return Err(native::invalid());
        }
        let content = if directory {
            None
        } else {
            self.remaining = self
                .remaining
                .checked_sub(before.bytes)
                .ok_or_else(native::invalid)?;
            Some(fingerprint(&mut file, before.bytes)?)
        };
        if native::identity(&file)? != before {
            return Err(native::invalid());
        }
        self.held.push(file);
        Ok(FileObservation::Present {
            kind: if directory {
                EntryKind::Folder
            } else {
                EntryKind::File
            },
            volume: before.volume,
            file_index: before.index,
            content,
        })
    }
}

fn fingerprint(file: &mut File, bytes: u64) -> io::Result<ObservedContent> {
    let mut reader = file.take(bytes);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut read_bytes = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read_bytes += u64::try_from(count).map_err(|_| native::invalid())?;
        digest.update(&buffer[..count]);
    }
    if read_bytes != bytes {
        return Err(native::invalid());
    }
    Ok(ObservedContent {
        bytes,
        sha256: digest.finalize().into(),
    })
}
