use super::{capture_source, identity::recheck, preflight::absent};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, operations::*};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn prepare(
    roots: Vec<OperationSource>,
    destination: OperationSource,
) -> Result<PreparedAction, Fault> {
    let mut descendants = Vec::new();
    OperationPlan {
        project_id: None,
        action: PreparedAction::CopyTree {
            roots: roots.clone(),
            descendants: vec![],
            destination: destination.clone(),
        },
    }
    .validate()?;
    check_roots(&roots, &destination)?;
    let mut budget = Budget::new(&destination)?;
    for source in &roots {
        budget.add(source, &roots, &destination)?;
    }
    let mut pending: Vec<_> = roots
        .iter()
        .filter(|source| source.kind == EntryKind::Folder)
        .cloned()
        .collect();
    while let Some(folder) = pending.pop() {
        recheck(&folder)?;
        for entry in fs::read_dir(&folder.path).map_err(Fault::from)? {
            if roots.len() + descendants.len() >= MAX_OPERATION_RESULTS {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            let path = entry.map_err(Fault::from)?.path();
            let source = capture_source(
                path.to_str()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
            )?;
            budget.add(&source, &roots, &destination)?;
            if source.kind == EntryKind::Folder {
                pending.push(source.clone());
            }
            descendants.push(source);
        }
    }
    Ok(PreparedAction::CopyTree {
        roots,
        descendants,
        destination,
    })
}

pub(super) fn check(
    roots: &[OperationSource],
    descendants: &[OperationSource],
    destination: &OperationSource,
) -> Result<(), Fault> {
    check_roots(roots, destination)?;
    let mut budget = Budget::new(destination)?;
    for source in roots.iter().chain(descendants) {
        recheck(source)?;
        budget.add(source, roots, destination)?;
        if source.kind == EntryKind::Folder {
            check_children(source, descendants)?;
        }
    }
    Ok(())
}

fn check_roots(roots: &[OperationSource], destination: &OperationSource) -> Result<(), Fault> {
    recheck(destination)?;
    let folders: Vec<_> = roots
        .iter()
        .filter(|source| source.kind == EntryKind::Folder)
        .collect();
    let ancestors = std::iter::once(Path::new(&destination.path)).chain(
        roots
            .iter()
            .filter_map(|source| Path::new(&source.path).parent()),
    );
    let mut checked = HashSet::new();
    for path in ancestors {
        for ancestor in path.ancestors() {
            if !checked.insert(ancestor) {
                continue;
            }
            let ancestor = capture_source(
                ancestor
                    .to_str()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
            )?;
            if folders.iter().any(|source| {
                (source.volume, source.file_index) == (ancestor.volume, ancestor.file_index)
            }) {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
        }
    }
    let mut names = HashSet::new();
    for source in roots {
        recheck(source)?;
        let name = Path::new(&source.path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
        if !names.insert(name.to_lowercase()) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        absent(&Path::new(&destination.path).join(name))?;
    }
    Ok(())
}

pub(super) fn check_children(
    folder: &OperationSource,
    descendants: &[OperationSource],
) -> Result<(), Fault> {
    let expected: Vec<_> = descendants
        .iter()
        .filter(|source| Path::new(&source.path).parent() == Some(Path::new(&folder.path)))
        .collect();
    let mut observed = 0;
    for entry in fs::read_dir(&folder.path).map_err(Fault::from)? {
        let path = entry.map_err(Fault::from)?.path();
        let source = capture_source(
            path.to_str()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
        )?;
        if !expected.contains(&&source) {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        observed += 1;
    }
    if observed != expected.len() {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    Ok(())
}

pub(super) fn target(
    source: &OperationSource,
    roots: &[OperationSource],
    destination: &OperationSource,
) -> Result<PathBuf, Fault> {
    let path = Path::new(&source.path);
    let root = roots
        .iter()
        .find(|root| {
            root.path == source.path
                || (root.kind == EntryKind::Folder && path.starts_with(&root.path))
        })
        .ok_or_else(|| Fault::new(FaultCode::InvalidRequest))?;
    let parent = Path::new(&root.path)
        .parent()
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    let relative = path
        .strip_prefix(parent)
        .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
    Ok(Path::new(&destination.path).join(relative))
}

struct Budget {
    intent: usize,
    result: usize,
}
impl Budget {
    fn new(destination: &OperationSource) -> Result<Self, Fault> {
        Ok(Self {
            intent: 4096 + json_size(destination)?,
            result: 4096,
        })
    }
    fn add(
        &mut self,
        source: &OperationSource,
        roots: &[OperationSource],
        destination: &OperationSource,
    ) -> Result<(), Fault> {
        self.intent += json_size(source)? + 1;
        let path = target(source, roots, destination)?
            .into_os_string()
            .into_string()
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
        self.result += json_size(&ItemOutcome {
            source: source.path.clone(),
            destination: Some(path),
            status: ItemStatus::Unknown,
            hresult: i32::MIN,
        })? + 1;
        if self.intent >= MAX_FRAME || self.result >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        Ok(())
    }
}
fn json_size(value: &impl serde::Serialize) -> Result<usize, Fault> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| Fault::new(FaultCode::InvalidRequest))
}
