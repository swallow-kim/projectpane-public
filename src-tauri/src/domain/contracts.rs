use super::{Fault, FaultCode, ListingView, MAX_ENTRY_ID};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_ACTIVATION_BATCH: usize = 128;
pub const BATCH_SIZE: usize = 256;
pub const DISPLAY_PAGE_SIZE: usize = 64;
pub const MAX_ENTRY_WIRE_BYTES: usize = (MAX_FRAME - 4096) / DISPLAY_PAGE_SIZE;
pub const MAX_ENTRIES: usize = 100_000;
pub const MAX_FRAME: usize = 256 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub pane: usize,
    pub tab_id: String,
    pub request_id: String,
    pub generation: u64,
}

impl Identity {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.pane >= 4
            || self.generation == 0
            || self.generation > 9_007_199_254_740_991
            || ![&self.tab_id, &self.request_id].iter().all(|s| {
                !s.is_empty()
                    && s.len() <= 80
                    && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    /// Milliseconds since Unix epoch UTC, or null when unavailable/outside JS Date range.
    #[serde(deserialize_with = "Option::deserialize")]
    pub modified_at_ms: Option<i64>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub size_bytes: Option<u64>,
}

impl Entry {
    fn is_valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= MAX_ENTRY_ID
            && self.id.is_ascii()
            && self
                .size_bytes
                .is_none_or(|size| size <= 9_007_199_254_740_991)
            && self.name.encode_utf16().count() <= 32_768
            && self
                .modified_at_ms
                .is_none_or(|ms| ms.unsigned_abs() <= 8_640_000_000_000_000)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Batch {
    pub revision: u64,
    pub offset: usize,
    pub entries: Vec<Entry>,
    pub total: usize,
    pub complete: bool,
    pub capped: bool,
    pub snapshot_bytes: usize,
    #[serde(deserialize_with = "Option::deserialize")]
    pub cap_reason: Option<String>,
}

impl Batch {
    pub(crate) fn validate(&self, request: &Request) -> Result<(), Fault> {
        let valid_cap = match self.cap_reason.as_deref() {
            Some("entry_limit" | "snapshot_budget" | "payload_budget") => {
                self.capped && self.complete
            }
            None => !self.capped,
            Some(_) => false,
        };
        let matches_request = match request {
            Request::Open { .. } | Request::SetView { .. } => self.offset == 0,
            Request::Locate { .. } | Request::Activate { .. } | Request::ActivateMany { .. } => {
                false
            }
            Request::Advance => true,
            Request::Progress => self.entries.is_empty(),
            Request::Window { offset, limit } => {
                (1..=BATCH_SIZE).contains(limit)
                    && self.offset == *offset
                    && self.entries.len() <= *limit
            }
        };
        if self.revision == 0
            || self.revision > 9_007_199_254_740_991
            || self.entries.len() > BATCH_SIZE
            || self.total > MAX_ENTRIES
            || self.snapshot_bytes > MAX_SNAPSHOT_BYTES
            || self.offset > self.total
            || self.entries.len() > self.total - self.offset
            || !valid_cap
            || !matches_request
            || self.entries.iter().any(|entry| !entry.is_valid())
        {
            return Err(Fault::new(FaultCode::ProtocolError));
        }
        let mut ids = HashSet::with_capacity(self.entries.len());
        if self
            .entries
            .iter()
            .any(|entry| !ids.insert(entry.id.as_str()))
        {
            return Err(Fault::new(FaultCode::ProtocolError));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub identity: Identity,
    pub batch: Batch,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    Open {
        path: String,
        #[serde(default)]
        view: ListingView,
    },
    SetView {
        view: ListingView,
    },
    Locate {
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    Activate {
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    ActivateMany {
        #[serde(rename = "entryIds")]
        entry_ids: Vec<String>,
    },
    Advance,
    Progress,
    Window {
        offset: usize,
        limit: usize,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Folder,
    Link,
    Unknown,
}
