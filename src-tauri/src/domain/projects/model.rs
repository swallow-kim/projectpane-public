use crate::domain::ListingView;
use serde::{Deserialize, Serialize};

// Wire models are intentionally constructible; use bounded parse/serialize APIs at storage boundaries.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    #[serde(deserialize_with = "integer")]
    pub schema_version: u32,
    pub id: String,
    #[serde(deserialize_with = "integer")]
    pub revision: u64,
    pub name: String,
    pub root_path: String,
    pub preset: Workspace,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    #[serde(deserialize_with = "integer")]
    pub schema_version: u32,
    pub project_id: String,
    #[serde(deserialize_with = "integer")]
    pub project_revision: u64,
    #[serde(deserialize_with = "integer")]
    pub revision: u64,
    pub workspace: Workspace,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    #[serde(deserialize_with = "integer")]
    pub schema_version: u32,
    #[serde(deserialize_with = "Option::deserialize")]
    pub last_project_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workspace {
    #[serde(deserialize_with = "integer")]
    pub layout: u8,
    pub columns: f64,
    pub rows: f64,
    #[serde(deserialize_with = "integer")]
    pub active_pane: usize,
    pub panes: [Pane; 4],
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pane {
    pub id: String,
    pub active_tab_id: String,
    pub tabs: Vec<Tab>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tab {
    pub id: String,
    pub path: PathRef,
    pub view: ListingView,
    pub scroll: Scroll,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PathRef {
    Relative { value: String },
    Absolute { value: String },
    Empty { value: String },
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scroll {
    #[serde(deserialize_with = "Option::deserialize")]
    pub entry_id: Option<String>,
    #[serde(deserialize_with = "integer")]
    pub index: u32,
    pub intra_row_offset: f64,
}

fn integer<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: TryFrom<u64>,
{
    let number = serde_json::Number::deserialize(deserializer)?;
    let integer = number
        .as_u64()
        .or_else(|| {
            number
                .as_f64()
                .filter(|value| value.is_finite() && value.fract() == 0.0)
                .and_then(|value| {
                    if value == 0.0 {
                        Some(0)
                    } else {
                        value.to_string().parse::<u64>().ok()
                    }
                })
        })
        .filter(|value| *value <= super::MAX_SAFE_REVISION);
    integer
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| serde::de::Error::custom("invalid_document"))
}
