use super::api::{Metadata, fail};
use crate::{domain::projects::*, persistence::projects::Transaction};
use serde::{Deserialize, Serialize};
use std::io;

pub(super) const MAX_PROJECTS: usize = 128;
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    pub schema_version: u32,
    pub projects: Vec<ManifestProject>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ManifestProject {
    pub id: String,
    pub root_path: String,
}

pub(super) fn contract(error: ProjectContractError) -> io::Error {
    fail(match error {
        ProjectContractError::InvalidDocument => "invalid_document",
        ProjectContractError::UnsupportedSchema => "unsupported_schema",
        ProjectContractError::PayloadLimit => "payload_limit",
        ProjectContractError::InvalidPath => "invalid_path",
    })
}
pub(super) fn manifest(tx: &Transaction<'_>) -> io::Result<Manifest> {
    match tx.read("manifest.json")? {
        Some(bytes) => parse_manifest(&bytes),
        None => Ok(Manifest {
            schema_version: 1,
            projects: Vec::new(),
        }),
    }
}
fn parse_manifest(bytes: &[u8]) -> io::Result<Manifest> {
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err(fail("payload_limit"));
    }
    let value: Manifest = serde_json::from_slice(bytes).map_err(|_| fail("invalid_document"))?;
    if value.schema_version > 1 {
        return Err(fail("unsupported_schema"));
    }
    if value.schema_version != 1 || value.projects.len() > MAX_PROJECTS {
        return Err(fail("invalid_document"));
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &value.projects {
        let id = &entry.id;
        normalize_absolute_path(&entry.root_path).map_err(contract)?;
        uuid(id)?;
        if !seen.insert(id) {
            return Err(fail("invalid_document"));
        }
    }
    Ok(value)
}
pub(super) fn write_manifest(tx: &Transaction<'_>, value: &Manifest) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| fail("invalid_document"))?;
    tx.write("manifest.json", &bytes, |bytes| {
        parse_manifest(bytes).map(|_| ())
    })
}
pub(super) fn settings(tx: &Transaction<'_>) -> io::Result<Settings> {
    match tx.read("settings.json")? {
        Some(bytes) => parse_settings(&bytes).map_err(contract),
        None => Ok(Settings {
            schema_version: 1,
            last_project_id: None,
        }),
    }
}
pub(super) fn select(tx: &Transaction<'_>, id: Option<String>) -> io::Result<()> {
    settings(tx)?;
    let bytes = serialize_settings(&Settings {
        schema_version: 1,
        last_project_id: id,
    })
    .map_err(contract)?;
    tx.write("settings.json", &bytes, |bytes| {
        parse_settings(bytes).map(|_| ()).map_err(contract)
    })
}
pub(super) fn uuid(id: &str) -> io::Result<()> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
    {
        return Err(fail("invalid_document"));
    }
    Ok(())
}
pub(super) fn known(tx: &Transaction<'_>, id: &str) -> io::Result<String> {
    uuid(id)?;
    manifest(tx)?
        .projects
        .into_iter()
        .find(|item| item.id == id)
        .map(|item| item.root_path)
        .ok_or_else(|| fail("not_found"))
}
pub(super) fn project(tx: &Transaction<'_>, id: &str) -> io::Result<Project> {
    let root = known(tx, id)?;
    let bytes = tx
        .read(&format!("{id}.project.json"))?
        .ok_or_else(|| fail("not_found"))?;
    let value = parse_project(&bytes).map_err(contract)?;
    if value.id != id || value.root_path != root {
        return Err(fail("invalid_document"));
    }
    Ok(value)
}
pub(super) fn session(tx: &Transaction<'_>, id: &str) -> io::Result<Option<Session>> {
    uuid(id)?;
    tx.read(&format!("{id}.session.json"))?
        .map(|bytes| {
            let value = parse_session(&bytes).map_err(contract)?;
            if value.project_id != id {
                return Err(fail("invalid_document"));
            }
            Ok(value)
        })
        .transpose()
}
pub(super) fn write_project(tx: &Transaction<'_>, value: &Project) -> io::Result<()> {
    let bytes = serialize_project(value).map_err(contract)?;
    tx.write(&format!("{}.project.json", value.id), &bytes, |bytes| {
        let original = parse_project(bytes).map_err(contract)?;
        if original.id != value.id || original.root_path != value.root_path {
            return Err(fail("read_only"));
        }
        Ok(())
    })
}
pub(super) fn metadata(value: &Project) -> Metadata {
    Metadata {
        id: value.id.clone(),
        name: value.name.clone(),
        root_path: value.root_path.clone(),
        revision: value.revision,
    }
}
pub(super) fn expect(actual: u64, expected: u64) -> io::Result<()> {
    if expected > MAX_SAFE_REVISION || actual != expected {
        return Err(fail("revision_conflict"));
    }
    Ok(())
}
pub(super) fn next(revision: u64) -> io::Result<u64> {
    revision
        .checked_add(1)
        .filter(|value| *value <= MAX_SAFE_REVISION)
        .ok_or_else(|| fail("revision_conflict"))
}
