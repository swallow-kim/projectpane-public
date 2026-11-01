use super::model::{Inspection, Observation, Record};
use crate::domain::{EntryKind, Fault, FaultCode, operations::validate_leaf_name, projects};

pub(super) fn inspection(input: &Inspection) -> Result<(), Fault> {
    record(&input.record)?;
    observations(input)
}

pub(super) fn record(record: &Record) -> Result<(), Fault> {
    if record.schema_version != 1 || !super::super::valid_token(&record.id) {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let objects = [
        (&record.source.object, EntryKind::File),
        (&record.original.object, EntryKind::File),
        (&record.staged.object, EntryKind::File),
        (&record.parent, EntryKind::Folder),
        (&record.artifacts, EntryKind::Folder),
    ];
    let mut identities = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    for (object, kind) in objects {
        object.validate()?;
        let normalized = projects::normalize_absolute_path(&object.path)
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
        if normalized != object.path
            || object.path.starts_with('\\')
            || object.kind != kind
            || object.volume != record.parent.volume
            || !identities.insert((object.volume, object.file_index))
            || !paths.insert(object.path.to_ascii_lowercase())
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    let parent = record.parent.path.trim_end_matches('\\');
    let directory = format!("{parent}\\.projectpane-recovery-{}", record.id);
    if record.artifacts.path != directory
        || record.staged.object.path != format!("{directory}\\staged")
        || record.backup_path != format!("{directory}\\original")
        || record.source.content != record.staged.content
        || paths.contains(&record.backup_path.to_ascii_lowercase())
        || record
            .source
            .object
            .path
            .to_ascii_lowercase()
            .starts_with(&(directory.to_ascii_lowercase() + "\\"))
    {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    projects::normalize_absolute_path(&record.backup_path)
        .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
    let leaf = record
        .original
        .object
        .path
        .strip_prefix(&format!("{parent}\\"))
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    validate_leaf_name(leaf)?;
    record
        .source
        .content
        .bytes
        .checked_add(record.original.content.bytes)
        .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
    Ok(())
}

fn observations(input: &Inspection) -> Result<(), Fault> {
    for observed in [
        &input.observations.parent,
        &input.observations.artifacts,
        &input.observations.target,
        &input.observations.staged,
        &input.observations.backup,
    ] {
        match observed {
            Observation::Absent | Observation::Inaccessible => {}
            Observation::Present {
                kind,
                file_index,
                content,
                ..
            } => {
                let valid_content = match kind {
                    EntryKind::File => content.is_some(),
                    EntryKind::Folder => content.is_none(),
                    EntryKind::Link | EntryKind::Unknown => false,
                };
                if *file_index == 0 || !valid_content {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
            }
        }
    }
    Ok(())
}
