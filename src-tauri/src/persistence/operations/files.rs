use crate::domain::{
    MAX_FRAME,
    operations::{MAX_OPERATION_HISTORY, bounded_json, valid_token},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub(super) fn invalid(_: impl std::fmt::Debug) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid_operation_document")
}

pub(super) fn check(path: &Path) -> io::Result<()> {
    super::super::paths::check_ancestors(path)
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "unsafe_operation_journal"))
}

pub(super) fn create(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = bounded_json(value).map_err(invalid)?;
    check(path)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()
}

pub(super) fn read<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    check(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() >= MAX_FRAME as u64 {
        return Err(invalid("document_size"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FRAME as u64).read_to_end(&mut bytes)?;
    if bytes.len() >= MAX_FRAME {
        return Err(invalid("document_size"));
    }
    serde_json::from_slice(&bytes).map(Some).map_err(invalid)
}

#[derive(Default)]
pub(super) struct JournalEntry {
    intent: bool,
    pub retired: bool,
}

pub(super) fn inventory(directory: &Path) -> io::Result<BTreeMap<String, JournalEntry>> {
    let mut ids = BTreeMap::<String, JournalEntry>::new();
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        if count > MAX_OPERATION_HISTORY * 2 {
            return Err(invalid("journal_capacity"));
        }
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(invalid)?;
        if name == "journal.lock" {
            continue;
        }
        let (id, kind) = ["intent", "result", "retired"]
            .into_iter()
            .find_map(|kind| {
                name.strip_suffix(&format!(".{kind}.json"))
                    .map(|id| (id, kind))
            })
            .filter(|(id, _)| valid_token(id))
            .ok_or_else(|| invalid("unexpected_journal_file"))?;
        check(&entry.path())?;
        if !entry.file_type()?.is_file() {
            return Err(invalid("unexpected_journal_entry"));
        }
        let record = ids.entry(id.to_string()).or_default();
        match kind {
            "intent" => record.intent = true,
            "result" => {}
            _ => record.retired = true,
        }
        if ids.len() > MAX_OPERATION_HISTORY {
            return Err(invalid("journal_capacity"));
        }
    }
    if ids.values().any(|record| record.intent == record.retired) {
        return Err(invalid("missing_or_ambiguous_operation_intent"));
    }
    Ok(ids)
}

pub(super) fn remove(path: &Path) -> io::Result<()> {
    check(path)?;
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub(super) fn finish_retirement(directory: &Path, id: &str) -> io::Result<()> {
    remove(&directory.join(format!("{id}.result.json")))?;
    remove(&directory.join(format!("{id}.retired.json")))
}
