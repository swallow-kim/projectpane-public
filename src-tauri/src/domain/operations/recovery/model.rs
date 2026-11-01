use super::super::OperationSource;
use crate::domain::EntryKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Content {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapturedFile {
    pub object: OperationSource,
    pub content: Content,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Ready,
    PreserveRequested,
    OriginalPreserved,
    InstallRequested,
    InstallObserved,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    pub schema_version: u32,
    pub id: String,
    pub phase: Phase,
    pub source: CapturedFile,
    pub original: CapturedFile,
    pub staged: CapturedFile,
    pub parent: OperationSource,
    pub artifacts: OperationSource,
    pub backup_path: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Observation {
    Absent,
    Inaccessible,
    Present {
        kind: EntryKind,
        volume: u32,
        #[serde(rename = "fileIndex")]
        file_index: u64,
        #[serde(deserialize_with = "Option::deserialize")]
        content: Option<Content>,
    },
}

impl Observation {
    pub fn matches(&self, expected: &OperationSource, fingerprint: Option<&Content>) -> bool {
        match self {
            Self::Absent | Self::Inaccessible => false,
            Self::Present {
                kind,
                volume,
                file_index,
                content,
            } => {
                *kind == expected.kind
                    && *volume == expected.volume
                    && *file_index == expected.file_index
                    && content.as_ref() == fingerprint
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Observations {
    pub parent: Observation,
    pub artifacts: Observation,
    pub target: Observation,
    pub staged: Observation,
    pub backup: Observation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Inspection {
    pub record: Record,
    pub observations: Observations,
}
