use super::{
    capture_source, copy_tree::check_children, identity::recheck, recycle::in_recycle_bin,
};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, operations::*};
use serde::Serialize;
use std::{fs, io, path::Path};

/// Captured identities for a single folder and its bounded descendants.
#[derive(Debug, Serialize)]
pub struct FolderRecycleSnapshot {
    root: OperationSource,
    descendants: Vec<OperationSource>,
}

impl FolderRecycleSnapshot {
    pub fn into_action(self) -> PreparedAction {
        PreparedAction::RecycleTree {
            root: self.root,
            descendants: self.descendants,
        }
    }

    pub(super) fn from_prepared(
        root: &OperationSource,
        descendants: &[OperationSource],
    ) -> Result<Self, Fault> {
        let snapshot = Self {
            root: root.clone(),
            descendants: descendants.to_vec(),
        };
        OperationPlan {
            project_id: None,
            action: PreparedAction::RecycleTree {
                root: root.clone(),
                descendants: descendants.to_vec(),
            },
        }
        .validate()?;
        snapshot.revalidate()?;
        Ok(snapshot)
    }

    pub fn capture(path: &str) -> Result<Self, Fault> {
        let root = capture_source(path)?;
        check_root(&root)?;
        let mut bytes = 4096 + bounded_json(&root)?.len();
        if bytes >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        let mut pending = vec![root.clone()];
        let mut snapshot = Self {
            root,
            descendants: Vec::new(),
        };
        while let Some(folder) = pending.pop() {
            recheck(&folder)?;
            for entry in fs::read_dir(&folder.path).map_err(Fault::from)? {
                if snapshot.descendants.len() + 1 >= MAX_OPERATION_RESULTS {
                    return Err(Fault::new(FaultCode::ResourceLimit));
                }
                let path = entry.map_err(Fault::from)?.path();
                let source = capture_source(
                    path.to_str()
                        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
                )?;
                bytes += bounded_json(&source)?.len() + 1;
                if bytes >= MAX_FRAME {
                    return Err(Fault::new(FaultCode::PayloadLimit));
                }
                if source.kind == EntryKind::Folder {
                    pending.push(source.clone());
                }
                snapshot.descendants.push(source);
            }
        }
        snapshot.revalidate()?;
        Ok(snapshot)
    }

    pub fn revalidate(&self) -> Result<(), Fault> {
        check_root(&self.root)?;
        for source in std::iter::once(&self.root).chain(&self.descendants) {
            recheck(source)?;
            if source.kind == EntryKind::Folder {
                check_children(source, &self.descendants)?;
            }
        }
        Ok(())
    }

    pub const fn root(&self) -> &OperationSource {
        &self.root
    }

    pub fn descendants(&self) -> &[OperationSource] {
        &self.descendants
    }

    /// Checks returned recycle evidence; it never moves or removes anything.
    pub fn confirm_recycled(&self, destination: &str) -> Result<(), Fault> {
        let actual = capture_source(destination)?;
        if !in_recycle_bin(&actual.path) {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        self.confirm_relocated(actual)
    }

    fn confirm_relocated(&self, actual: OperationSource) -> Result<(), Fault> {
        match fs::symlink_metadata(&self.root.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => return Err(Fault::new(FaultCode::StaleRequest)),
        }
        if (actual.kind, actual.volume, actual.file_index)
            != (self.root.kind, self.root.volume, self.root.file_index)
        {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        let mut bytes = 4096 + bounded_json(&actual)?.len();
        let mut descendants = Vec::new();
        for source in &self.descendants {
            let relative = Path::new(&source.path)
                .strip_prefix(&self.root.path)
                .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
            let path = Path::new(&actual.path)
                .join(relative)
                .into_os_string()
                .into_string()
                .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
            let moved = OperationSource {
                path,
                kind: source.kind,
                volume: source.volume,
                file_index: source.file_index,
            };
            bytes += bounded_json(&moved)?.len() + 1;
            if bytes >= MAX_FRAME {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            recheck(&moved)?;
            descendants.push(moved);
        }
        for folder in std::iter::once(&actual)
            .chain(&descendants)
            .filter(|source| source.kind == EntryKind::Folder)
        {
            check_children(folder, &descendants)?;
        }
        Ok(())
    }
}

fn check_root(root: &OperationSource) -> Result<(), Fault> {
    if root.kind != EntryKind::Folder
        || Path::new(&root.path).file_name().is_none()
        || in_recycle_bin(&root.path)
    {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    Ok(())
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
