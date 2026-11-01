mod files;
mod retirement;

use crate::domain::operations::{
    MAX_OPERATION_HISTORY, OperationCompletion, OperationIntent, OperationStatus,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

pub struct RecoveredOperation {
    pub intent: OperationIntent,
    pub completion: Option<OperationCompletion>,
}

impl RecoveredOperation {
    pub fn status(&self) -> OperationStatus {
        self.completion
            .as_ref()
            .map_or(OperationStatus::InterruptedUnknown, |record| {
                record.report.status()
            })
    }
}

pub struct OperationJournal {
    directory: PathBuf,
    _lock: File,
    records: BTreeMap<String, OperationIntent>,
    writable: bool,
}

impl OperationJournal {
    #[cfg(test)]
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn open(
        default: &Path,
        override_path: Option<&OsStr>,
    ) -> io::Result<(Self, Vec<RecoveredOperation>)> {
        let foundation = super::resolve_data_root(default, override_path)
            .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "unsafe_data_root"))?;
        let directory = foundation.join("operations-v1");
        files::check(&directory)?;
        fs::create_dir_all(&directory)?;
        files::check(&directory)?;
        let lock_path = directory.join("journal.lock");
        files::check(&lock_path)?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "operation_journal_busy"))?;
        let names = files::inventory(&directory)?;
        let mut records = BTreeMap::new();
        let mut recovered = Vec::new();
        let mut retired = Vec::new();
        for (id, entry) in names {
            let suffix = if entry.retired { "retired" } else { "intent" };
            let intent: OperationIntent =
                files::read(&directory.join(format!("{id}.{suffix}.json")))?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing_operation_intent")
                })?;
            intent.validate().map_err(files::invalid)?;
            if intent.id != id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "operation_id_mismatch",
                ));
            }
            if entry.retired {
                if let Some(result) = files::read::<OperationCompletion>(
                    &directory.join(format!("{id}.result.json")),
                )? {
                    result.validate().map_err(files::invalid)?;
                    result
                        .report
                        .validate_for_plan(&intent.plan)
                        .map_err(files::invalid)?;
                    if result.id != id
                        || result.report.status() == OperationStatus::InterruptedUnknown
                    {
                        return Err(files::invalid("invalid_retired_result"));
                    }
                }
                retired.push(id);
                continue;
            }
            let completion = match files::read::<OperationCompletion>(
                &directory.join(format!("{id}.result.json")),
            ) {
                Ok(Some(result))
                    if result.id == id
                        && result.validate().is_ok()
                        && result.report.validate_for_plan(&intent.plan).is_ok() =>
                {
                    Some(result)
                }
                Ok(_) => None,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => None,
                Err(error) => return Err(error),
            };
            records.insert(id, intent.clone());
            recovered.push(RecoveredOperation { intent, completion });
        }
        for id in retired {
            files::finish_retirement(&directory, &id)?;
        }
        Ok((
            Self {
                directory,
                _lock: lock,
                records,
                writable: true,
            },
            recovered,
        ))
    }

    pub fn record_intent(&mut self, intent: &OperationIntent) -> io::Result<()> {
        self.ready()?;
        intent.validate().map_err(files::invalid)?;
        if self.records.contains_key(&intent.id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "operation_already_recorded",
            ));
        }
        if self.records.len() >= MAX_OPERATION_HISTORY {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "operation_history_full",
            ));
        }
        let result = files::create(
            &self.directory.join(format!("{}.intent.json", intent.id)),
            intent,
        );
        if result.is_ok() {
            self.records.insert(intent.id.clone(), intent.clone());
        } else {
            self.writable = false;
        }
        result
    }

    pub fn record_completion(&mut self, completion: &OperationCompletion) -> io::Result<()> {
        self.ready()?;
        completion.validate().map_err(files::invalid)?;
        if let Some(intent) = self.records.get(&completion.id) {
            completion
                .report
                .validate_for_plan(&intent.plan)
                .map_err(files::invalid)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown_operation",
            ));
        }
        let result = files::create(
            &self
                .directory
                .join(format!("{}.result.json", completion.id)),
            completion,
        );
        if result.is_err() {
            self.writable = false;
        }
        result
    }

    pub fn remaining(&self) -> usize {
        MAX_OPERATION_HISTORY.saturating_sub(self.records.len())
    }

    fn ready(&self) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "operation_journal_faulted",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
