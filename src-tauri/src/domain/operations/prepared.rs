use super::{MAX_OPERATION_SOURCES, validate_leaf_name};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, projects};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationSource {
    pub path: String,
    pub kind: EntryKind,
    pub volume: u32,
    pub file_index: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparedAction {
    Archive {
        roots: Vec<OperationSource>,
        descendants: Vec<OperationSource>,
        destination: OperationSource,
        name: String,
        extract: bool,
    },
    NewFolder {
        parent: OperationSource,
        name: String,
    },
    Rename {
        source: OperationSource,
        name: String,
    },
    Copy {
        sources: Vec<OperationSource>,
        destination: OperationSource,
    },
    CopySkipExisting {
        sources: Vec<OperationSource>,
        destination: OperationSource,
        skipped: Vec<super::SkipExistingTarget>,
    },
    CopyAs {
        source: OperationSource,
        destination: OperationSource,
        name: String,
    },
    CopyTree {
        roots: Vec<OperationSource>,
        descendants: Vec<OperationSource>,
        destination: OperationSource,
    },
    Move {
        sources: Vec<OperationSource>,
        destination: OperationSource,
    },
    MoveTree {
        roots: Vec<OperationSource>,
        descendants: Vec<OperationSource>,
        destination: OperationSource,
    },
    Recycle {
        sources: Vec<OperationSource>,
    },
    RecycleTree {
        root: OperationSource,
        descendants: Vec<OperationSource>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationPlan {
    #[serde(deserialize_with = "Option::deserialize")]
    pub project_id: Option<String>,
    pub action: PreparedAction,
}

impl OperationSource {
    pub(super) fn validate(&self) -> Result<(), Fault> {
        projects::normalize_absolute_path(&self.path)
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
        if !matches!(self.kind, EntryKind::File | EntryKind::Folder) || self.file_index == 0 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        Ok(())
    }

    fn directory(&self) -> Result<(), Fault> {
        self.validate()?;
        if self.kind != EntryKind::Folder {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        Ok(())
    }
}

impl OperationPlan {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.project_id.as_ref().is_some_and(|id| !valid_token(id)) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        match &self.action {
            PreparedAction::Archive {
                roots,
                descendants,
                destination,
                name,
                extract,
            } => {
                validate_sources(roots)?;
                destination.directory()?;
                validate_leaf_name(name)?;
                if *extract {
                    if roots.len() != 1
                        || roots[0].kind != EntryKind::File
                        || !descendants.is_empty()
                        || !roots[0].path.to_lowercase().ends_with(".zip")
                    {
                        return Err(Fault::new(FaultCode::InvalidRequest));
                    }
                } else {
                    if !name.to_lowercase().ends_with(".zip") {
                        return Err(Fault::new(FaultCode::InvalidPath));
                    }
                    if roots.iter().any(|s| s.kind == EntryKind::Folder) {
                        super::tree::validate_tree(roots, descendants)?;
                    } else if !descendants.is_empty() {
                        return Err(Fault::new(FaultCode::InvalidRequest));
                    }
                }
            }
            PreparedAction::NewFolder { parent, name } => {
                parent.directory()?;
                validate_leaf_name(name)?;
            }
            PreparedAction::Rename { source, name } => {
                source.validate()?;
                validate_leaf_name(name)?;
            }
            PreparedAction::CopySkipExisting {
                sources,
                destination,
                skipped,
            } => {
                validate_sources(sources)?;
                destination.directory()?;
                super::skip_existing::validate(sources, destination, skipped)?;
            }
            PreparedAction::CopyAs {
                source,
                destination,
                name,
            } => {
                source.validate()?;
                if source.kind != EntryKind::File {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                destination.directory()?;
                validate_leaf_name(name)?;
                let target = std::path::Path::new(&destination.path).join(name);
                let target = target
                    .to_str()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
                projects::normalize_absolute_path(target)
                    .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
                let result_paths = serde_json::to_vec(&(&source.path, target))
                    .map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
                if result_paths.len() + 4096 >= MAX_FRAME {
                    return Err(Fault::new(FaultCode::PayloadLimit));
                }
            }
            PreparedAction::Copy {
                sources,
                destination,
            }
            | PreparedAction::Move {
                sources,
                destination,
            } => {
                validate_sources(sources)?;
                destination.directory()?;
            }
            PreparedAction::Recycle { sources } => validate_sources(sources)?,
            PreparedAction::RecycleTree { root, descendants } => {
                root.directory()?;
                super::tree::validate_tree(std::slice::from_ref(root), descendants)?;
            }
            PreparedAction::CopyTree {
                roots,
                descendants,
                destination,
            }
            | PreparedAction::MoveTree {
                roots,
                descendants,
                destination,
            } => {
                validate_sources(roots)?;
                destination.directory()?;
                super::tree::validate_tree(roots, descendants)?;
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
        if bytes.len() >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        Ok(())
    }
}

fn validate_sources(sources: &[OperationSource]) -> Result<(), Fault> {
    if sources.is_empty() || sources.len() > MAX_OPERATION_SOURCES {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let mut seen = std::collections::HashSet::new();
    for source in sources {
        source.validate()?;
        if !seen.insert((source.volume, source.file_index)) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    Ok(())
}

pub(crate) fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
