use super::{OperationSelection, bounded_json, valid_token};
use crate::domain::{Fault, FaultCode, projects};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    Copy,
    Move,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureTransfer {
    pub mode: TransferMode,
    pub selection: OperationSelection,
}

impl CaptureTransfer {
    pub fn validate(&self) -> Result<(), Fault> {
        self.selection.validate()?;
        bounded_json(self).map(|_| ())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareTransfer {
    pub token: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub project_id: Option<String>,
    pub destination: String,
    #[serde(default)]
    pub copy_name: Option<String>,
    #[serde(default)]
    pub skip_existing: bool,
}

impl PrepareTransfer {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.skip_existing && self.copy_name.is_some() {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        if let Some(name) = &self.copy_name {
            super::validate_leaf_name(name)?;
        }
        if !valid_token(&self.token) || self.project_id.as_ref().is_some_and(|id| !valid_token(id))
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        projects::normalize_absolute_path(&self.destination)
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
        bounded_json(self).map(|_| ())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    pub token: String,
    pub mode: TransferMode,
    pub count: usize,
}
