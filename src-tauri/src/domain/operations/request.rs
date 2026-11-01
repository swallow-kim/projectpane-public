use crate::domain::{Fault, FaultCode, Identity, MAX_ENTRY_ID, MAX_FRAME, projects};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_OPERATION_SOURCES: usize = 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationSelection {
    pub identity: Identity,
    pub entry_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationAction {
    Archive {
        selection: OperationSelection,
        destination: String,
        name: String,
        extract: bool,
    },
    NewFolder {
        parent: String,
        name: String,
    },
    Rename {
        selection: OperationSelection,
        name: String,
    },
    Copy {
        selection: OperationSelection,
        destination: String,
    },
    CopySkipExisting {
        selection: OperationSelection,
        destination: String,
    },
    CopyAs {
        selection: OperationSelection,
        destination: String,
        name: String,
    },
    Move {
        selection: OperationSelection,
        destination: String,
    },
    Recycle {
        selection: OperationSelection,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareOperation {
    #[serde(deserialize_with = "Option::deserialize")]
    pub project_id: Option<String>,
    pub action: OperationAction,
}

impl PrepareOperation {
    pub fn selection(&self) -> Option<&OperationSelection> {
        match &self.action {
            OperationAction::NewFolder { .. } => None,
            OperationAction::Archive { selection, .. }
            | OperationAction::Rename { selection, .. }
            | OperationAction::Copy { selection, .. }
            | OperationAction::CopyAs { selection, .. }
            | OperationAction::CopySkipExisting { selection, .. }
            | OperationAction::Move { selection, .. }
            | OperationAction::Recycle { selection } => Some(selection),
        }
    }

    pub fn validate(&self) -> Result<(), Fault> {
        if self.project_id.as_ref().is_some_and(|id| {
            id.is_empty()
                || id.len() > 80
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        match &self.action {
            OperationAction::Archive {
                selection,
                destination,
                name,
                extract,
            } => {
                selection.validate()?;
                validate_path(destination)?;
                validate_leaf_name(name)?;
                if *extract && selection.entry_ids.len() != 1 {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                if !extract && !name.to_lowercase().ends_with(".zip") {
                    return Err(Fault::new(FaultCode::InvalidPath));
                }
            }
            OperationAction::NewFolder { parent, name } => {
                validate_path(parent)?;
                validate_leaf_name(name)?;
            }
            OperationAction::Rename { selection, name } => {
                selection.validate()?;
                if selection.entry_ids.len() != 1 {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                validate_leaf_name(name)?;
            }
            OperationAction::CopyAs {
                selection,
                destination,
                name,
            } => {
                selection.validate()?;
                if selection.entry_ids.len() != 1 {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                validate_path(destination)?;
                validate_leaf_name(name)?;
            }
            OperationAction::Copy {
                selection,
                destination,
            }
            | OperationAction::CopySkipExisting {
                selection,
                destination,
            }
            | OperationAction::Move {
                selection,
                destination,
            } => {
                selection.validate()?;
                validate_path(destination)?;
            }
            OperationAction::Recycle { selection } => selection.validate()?,
        }
        let bytes = serde_json::to_vec(self).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
        if bytes.len() >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        Ok(())
    }
}

impl OperationSelection {
    pub fn validate(&self) -> Result<(), Fault> {
        self.identity.validate()?;
        if self.entry_ids.is_empty() || self.entry_ids.len() > MAX_OPERATION_SOURCES {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let mut seen = HashSet::new();
        for id in &self.entry_ids {
            if id.is_empty()
                || id.len() > MAX_ENTRY_ID
                || !id.is_ascii()
                || id.bytes().any(|byte| byte.is_ascii_control())
                || !seen.insert(id)
            {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
        }
        Ok(())
    }
}

fn validate_path(path: &str) -> Result<(), Fault> {
    projects::normalize_absolute_path(path)
        .map(|_| ())
        .map_err(|_| Fault::new(FaultCode::InvalidPath))
}

pub fn validate_leaf_name(name: &str) -> Result<(), Fault> {
    let invalid = || Fault::new(FaultCode::InvalidPath);
    if name.is_empty() || name.encode_utf16().count() > 255 || name.contains(['/', '\\']) {
        return Err(invalid());
    }
    let normalized = projects::normalize_relative_path(name).map_err(|_| invalid())?;
    if normalized != name {
        return Err(invalid());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    if !stem.is_empty() {
        projects::normalize_relative_path(stem).map_err(|_| invalid())?;
    }
    Ok(())
}

pub fn parse_operation(bytes: &[u8]) -> Result<PrepareOperation, Fault> {
    if bytes.len() >= MAX_FRAME {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    let request: PrepareOperation =
        serde_json::from_slice(bytes).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
    request.validate()?;
    Ok(request)
}
