use super::{MAX_RECOVERY_JOURNAL_BYTES, files, load};
use crate::{
    domain::{
        operations::{
            MAX_OPERATION_HISTORY,
            recovery::{RecoveryPhase, RecoveryRecord},
        },
        projects,
    },
    platform::recovery_observation::guard_recovery_directory,
};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod failure_tests;
mod lease;
pub use lease::JournalLease;
#[cfg(test)]
mod tests;

/// No new file was created, or a created file may contain partial/unacknowledged data.
#[derive(Debug)]
pub enum AppendFailure {
    NotCreated(io::ErrorKind),
    Unacknowledged(io::ErrorKind),
}

impl AppendFailure {
    fn before(error: io::Error) -> Self {
        Self::NotCreated(error.kind())
    }
}

/// A write/flush/sync completed; not mutation authority or power-loss proof.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[must_use]
pub struct PhaseAcknowledgement {
    id: String,
    phase: RecoveryPhase,
}

/// Appends one immutable phase to an existing Windows recovery journal.
/// Existing documents are never overwritten, repaired or removed. No directory/lock is created.
///
/// # Errors
/// `NotCreated` means this call did not create a document. After creation every failure is
/// `Unacknowledged`; preserve the file and inspect the journal rather than retrying blindly.
pub fn append_phase(
    directory: &Path,
    record: &RecoveryRecord,
) -> Result<PhaseAcknowledgement, AppendFailure> {
    append_with(directory, record, persist)
}

fn append_with(
    directory: &Path,
    record: &RecoveryRecord,
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> Result<PhaseAcknowledgement, AppendFailure> {
    JournalLease::open(directory)?
        .append_with(record, write)
        .map(|(_lease, ack)| ack)
}

fn append_locked(
    directory: &Path,
    record: &RecoveryRecord,
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> Result<PhaseAcknowledgement, AppendFailure> {
    let mut retained = Vec::new();
    let (records, total) =
        load(directory, |file| retained.push(file)).map_err(AppendFailure::before)?;
    validate_next(&records, record).map_err(AppendFailure::before)?;
    let bytes = record
        .to_bytes()
        .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
    let length = u64::try_from(bytes.len())
        .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
    if total
        .checked_add(length)
        .is_none_or(|size| size > MAX_RECOVERY_JOURNAL_BYTES)
    {
        return Err(AppendFailure::NotCreated(io::ErrorKind::InvalidData));
    }
    let path = directory.join(format!(
        "{}.{}.json",
        record.id(),
        record.phase().file_label()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(AppendFailure::before)?;
    write(&mut file, &bytes).map_err(|error| AppendFailure::Unacknowledged(error.kind()))?;
    Ok(PhaseAcknowledgement {
        id: record.id().to_owned(),
        phase: record.phase(),
    })
}

fn normalize(path: &Path) -> io::Result<PathBuf> {
    files::check_directory(path)?;
    let text = path
        .to_str()
        .ok_or_else(|| files::invalid("unicode_recovery_directory"))?;
    projects::normalize_absolute_path(text)
        .map(PathBuf::from)
        .map_err(|_| files::invalid("invalid_recovery_directory"))
}

fn validate_next(records: &[RecoveryRecord], record: &RecoveryRecord) -> io::Result<()> {
    match records
        .iter()
        .find(|prior| prior.id().eq_ignore_ascii_case(record.id()))
    {
        Some(prior) if prior.id() == record.id() && record.follows(prior) => Ok(()),
        Some(_) => Err(files::invalid("recovery_phase_already_present_or_changed")),
        None if records.len() < MAX_OPERATION_HISTORY && record.phase() == RecoveryPhase::Ready => {
            Ok(())
        }
        None => Err(files::invalid("recovery_initial_phase_or_capacity")),
    }
}

trait SyncWrite: Write {
    fn sync(&mut self) -> io::Result<()>;
}

impl SyncWrite for File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}

fn persist(file: &mut impl SyncWrite, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.flush()?;
    file.sync()
}
