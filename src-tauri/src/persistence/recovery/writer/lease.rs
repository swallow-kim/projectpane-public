use super::*;

#[cfg(test)]
mod tests;

/// Exclusive coordination for one existing journal, not target-mutation authority.
/// Guards and lock live across phases; documents are revalidated at each append.
/// Noncooperating changes between appends are not an immutable snapshot.
#[must_use]
pub struct JournalLease {
    directory: PathBuf,
    _lock: File,
    _guards: Vec<File>,
}

impl JournalLease {
    /// Acquires an existing journal's exclusive lock without creating anything.
    /// The journal's document contents are checked on append, not on open.
    ///
    /// # Errors
    /// Returns `NotCreated` on unsafe/missing paths or lock contention.
    pub fn open(directory: &Path) -> Result<Self, AppendFailure> {
        let directory = normalize(directory).map_err(AppendFailure::before)?;
        let guards = guard_recovery_directory(&directory).map_err(AppendFailure::before)?;
        files::check_directory(&directory).map_err(AppendFailure::before)?;
        let lock = files::open(&directory.join("journal.lock")).map_err(AppendFailure::before)?;
        lock.try_lock()
            .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::WouldBlock))?;
        Ok(Self {
            directory,
            _lock: lock,
            _guards: guards,
        })
    }

    /// Consumes this lease and returns it only after an acknowledged append.
    /// Dropping the returned lease releases the lock without deleting any files.
    ///
    /// # Errors
    /// Any failure releases this lease without a continuation capability.
    /// Effects classify this append only; prior acknowledged phases remain.
    pub fn append(
        self,
        record: &RecoveryRecord,
    ) -> Result<(Self, PhaseAcknowledgement), AppendFailure> {
        self.append_with(record, persist)
    }

    pub(super) fn append_with(
        self,
        record: &RecoveryRecord,
        write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
    ) -> Result<(Self, PhaseAcknowledgement), AppendFailure> {
        let ack = append_locked(&self.directory, record, write)?;
        Ok((self, ack))
    }
}
