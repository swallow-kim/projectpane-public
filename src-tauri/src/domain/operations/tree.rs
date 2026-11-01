use super::{MAX_OPERATION_RESULTS, OperationSource};
use crate::domain::{EntryKind, Fault, FaultCode, projects};
use std::collections::{HashMap, HashSet};

pub(super) fn validate_tree(
    roots: &[OperationSource],
    descendants: &[OperationSource],
) -> Result<(), Fault> {
    if roots.len().saturating_add(descendants.len()) > MAX_OPERATION_RESULTS {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    if !roots.iter().any(|source| source.kind == EntryKind::Folder) {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let mut paths = HashMap::new();
    let mut directories = HashSet::new();
    for source in roots.iter().chain(descendants) {
        source.validate()?;
        if projects::normalize_absolute_path(&source.path)
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?
            != source.path
            || paths.insert(source.path.to_lowercase(), source).is_some()
            || (source.kind == EntryKind::Folder
                && !directories.insert((source.volume, source.file_index)))
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    for source in descendants {
        let parent = source
            .path
            .rsplit_once('\\')
            .map(|(parent, _)| parent.to_lowercase());
        if !parent
            .and_then(|key| paths.get(&key))
            .is_some_and(|parent| {
                parent.kind == EntryKind::Folder && parent.volume == source.volume
            })
            || !roots
                .iter()
                .any(|root| root.kind == EntryKind::Folder && inside(&source.path, &root.path))
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    if roots.iter().any(|source| {
        roots
            .iter()
            .any(|parent| inside(&source.path, &parent.path))
    }) {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    Ok(())
}

fn inside(path: &str, parent: &str) -> bool {
    path.get(..parent.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(parent))
        && path.as_bytes().get(parent.len()) == Some(&b'\\')
}
