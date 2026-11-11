use crate::domain::projects::Workspace;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Warning {
    pub code: String,
    pub project_id: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Preset,
    Session,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    pub project: Metadata,
    pub workspace: Workspace,
    pub session_revision: u64,
    pub source: Source,
    pub read_only: bool,
    pub warnings: Vec<Warning>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub projects: Vec<Metadata>,
    pub last_project_id: Option<String>,
    pub warnings: Vec<Warning>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSaved {
    pub project_id: String,
    pub project_revision: u64,
    pub session_revision: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    pub removed_project_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    pub name: String,
    pub root_path: String,
    pub preset: Workspace,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadRequest {
    pub project_id: String,
    pub source: Source,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NameRequest {
    pub project_id: String,
    pub expected_revision: u64,
    pub name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditRequest {
    pub project_id: String,
    pub expected_revision: u64,
    pub name: String,
    pub root_path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveRequest {
    pub project_id: String,
    pub expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetRequest {
    pub project_id: String,
    pub expected_revision: u64,
    pub workspace: Workspace,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRequest {
    pub project_id: String,
    pub expected_project_revision: u64,
    pub expected_session_revision: u64,
    pub workspace: Workspace,
}

#[derive(Debug, Serialize)]
pub struct ProjectError {
    pub code: &'static str,
}
impl From<std::io::Error> for ProjectError {
    fn from(error: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let code = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ServiceError>())
            .map(|inner| inner.0)
            .unwrap_or(match error.kind() {
                ErrorKind::WouldBlock => "busy",
                ErrorKind::InvalidInput => "invalid_document",
                ErrorKind::InvalidData => "invalid_document",
                ErrorKind::NotFound => "not_found",
                ErrorKind::PermissionDenied => "invalid_path",
                _ => "io_error",
            });
        Self { code }
    }
}
#[derive(Debug)]
struct ServiceError(&'static str);
impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ServiceError {}
pub(super) fn fail(code: &'static str) -> std::io::Error {
    std::io::Error::other(ServiceError(code))
}
