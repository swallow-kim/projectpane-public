//! Bounded recovery journal loading and create-new phase writes; never replay or deletion.
mod files;
#[cfg(windows)]
mod writer;
#[cfg(windows)]
pub use writer::{AppendFailure, JournalLease, PhaseAcknowledgement, append_phase};
#[cfg(test)]
mod tests;

use crate::domain::operations::recovery::{RecoveryPhase, RecoveryRecord};
use std::{io, path::Path};

/// Total raw phase-document bytes allowed in one inspection, not resident memory.
pub const MAX_RECOVERY_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;

/// Loads existing contiguous phase chains while holding a shared journal lock.
/// Records remain unobserved claims; no successful file operation is inferred.
/// The directory and lock must already exist. Cooperating writers must use an exclusive lock.
///
/// # Errors
/// Refuses unsafe paths, lock contention, unknown files, missing/mutated phases,
/// malformed records and size/history limits. No bytes are repaired on failure.
pub fn read_journal(directory: &Path) -> io::Result<Vec<RecoveryRecord>> {
    files::check_directory(directory)?;
    let lock = files::open(&directory.join("journal.lock"))?;
    lock.try_lock_shared()
        .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "recovery_journal_busy"))?;
    load(directory, drop).map(|(records, _)| records)
}

fn load(
    directory: &Path,
    mut retain: impl FnMut(std::fs::File),
) -> io::Result<(Vec<RecoveryRecord>, u64)> {
    let chains = files::inventory(directory)?;
    let mut total = 0_u64;
    let mut recovered = Vec::with_capacity(chains.len());
    for chain in chains {
        let mut previous: Option<RecoveryRecord> = None;
        for (phase, path) in chain.phases {
            let (record, file) = files::read(&path, &mut total)?;
            retain(file);
            if record.id() != chain.id || record.phase() != phase {
                return Err(files::invalid("recovery_filename_mismatch"));
            }
            let contiguous = match &previous {
                Some(prior) => record.follows(prior),
                None => phase == RecoveryPhase::Ready,
            };
            if !contiguous {
                return Err(files::invalid("recovery_chain_mismatch"));
            }
            previous = Some(record);
        }
        let record = previous.ok_or_else(|| files::invalid("empty_recovery_chain"))?;
        recovered.push(record);
    }
    Ok((recovered, total))
}
