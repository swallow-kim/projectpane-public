use super::{Fault, FaultCode, Identity};
use serde::{Deserialize, Serialize};

pub const MAX_ENTRY_ID: usize = 2048;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingView {
    pub sort: SortColumn,
    pub direction: SortDirection,
    pub folders_first: bool,
    pub filter: String,
}

impl Default for ListingView {
    fn default() -> Self {
        Self {
            sort: SortColumn::Name,
            direction: SortDirection::Asc,
            folders_first: true,
            filter: String::new(),
        }
    }
}
impl ListingView {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.filter.len() > 1024
            || self.filter.encode_utf16().count() > 256
            || self.filter.contains('\0')
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    Name,
    Type,
    Size,
    Modified,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocatedEntry {
    pub revision: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub index: Option<usize>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocateReply {
    pub identity: Identity,
    pub revision: u64,
    pub index: Option<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActivationKind {
    Folder,
    File,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationTarget {
    pub kind: ActivationKind,
    pub path: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationReply {
    pub identity: Identity,
    pub kind: ActivationKind,
    pub path: Option<String>,
}
