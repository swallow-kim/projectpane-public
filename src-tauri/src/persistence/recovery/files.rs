use crate::domain::{
    MAX_FRAME,
    operations::{
        MAX_OPERATION_HISTORY,
        recovery::{RecoveryPhase, RecoveryRecord},
        valid_token,
    },
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

pub(super) struct ChainFiles {
    pub id: String,
    pub phases: BTreeMap<RecoveryPhase, PathBuf>,
}

pub(super) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn check_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid("absolute_recovery_directory_required"));
    }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_)))
    {
        return Err(invalid("local_recovery_directory_required"));
    }
    check(path)?;
    if !fs::metadata(path)?.is_dir() {
        return Err(invalid("recovery_directory_required"));
    }
    Ok(())
}

fn check(path: &Path) -> io::Result<()> {
    super::super::paths::check_ancestors(path)
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "unsafe_recovery_path"))
}

pub(super) fn open(path: &Path) -> io::Result<File> {
    check(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(invalid("reparse_recovery_document"));
        }
    }
    if !file.metadata()?.is_file() {
        return Err(invalid("regular_recovery_document_required"));
    }
    Ok(file)
}

pub(super) fn read(path: &Path, total: &mut u64) -> io::Result<(RecoveryRecord, File)> {
    let file = open(path)?;
    let frame = u64::try_from(MAX_FRAME).map_err(|_| invalid("frame_limit"))?;
    let length = file.metadata()?.len();
    if length >= frame
        || total
            .checked_add(length)
            .is_none_or(|size| size > super::MAX_RECOVERY_JOURNAL_BYTES)
    {
        return Err(invalid("recovery_input_limit"));
    }
    let mut bytes = Vec::new();
    (&file).take(frame).read_to_end(&mut bytes)?;
    let actual = u64::try_from(bytes.len()).map_err(|_| invalid("recovery_input_limit"))?;
    *total = total
        .checked_add(actual)
        .ok_or_else(|| invalid("recovery_input_limit"))?;
    if actual != length || *total > super::MAX_RECOVERY_JOURNAL_BYTES {
        return Err(invalid("changed_recovery_document"));
    }
    let record = RecoveryRecord::parse(&bytes).map_err(|_| invalid("invalid_recovery_document"))?;
    Ok((record, file))
}

pub(super) fn inventory(directory: &Path) -> io::Result<Vec<ChainFiles>> {
    let mut chains = BTreeMap::<String, ChainFiles>::new();
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        if count > MAX_OPERATION_HISTORY * 5 {
            return Err(invalid("recovery_file_limit"));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("recovery_filename"))?;
        if name == "journal.lock" {
            continue;
        }
        check(&entry.path())?;
        if !entry.file_type()?.is_file() {
            return Err(invalid("recovery_file_required"));
        }
        let (id, label) = name
            .strip_suffix(".json")
            .and_then(|name| name.split_once('.'))
            .filter(|(id, _)| valid_token(id))
            .ok_or_else(|| invalid("recovery_filename"))?;
        let phase = match label {
            "ready" => RecoveryPhase::Ready,
            "preserve_requested" => RecoveryPhase::PreserveRequested,
            "original_preserved" => RecoveryPhase::OriginalPreserved,
            "install_requested" => RecoveryPhase::InstallRequested,
            "install_observed" => RecoveryPhase::InstallObserved,
            _ => return Err(invalid("recovery_phase_filename")),
        };
        let chain = chains
            .entry(id.to_ascii_lowercase())
            .or_insert_with(|| ChainFiles {
                id: id.to_owned(),
                phases: BTreeMap::new(),
            });
        if chain.id != id || chain.phases.insert(phase, entry.path()).is_some() {
            return Err(invalid("ambiguous_recovery_id"));
        }
        if chains.len() > MAX_OPERATION_HISTORY {
            return Err(invalid("recovery_history_limit"));
        }
    }
    Ok(chains.into_values().collect())
}
