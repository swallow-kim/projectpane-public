//! A durable redo intent makes the multi-document root change recoverable.
//! All entry points recover under the same catalog lock before exposing state.
use super::{documents::*, *};
use crate::{domain::projects::*, persistence::projects::Transaction};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    schema_version: u32,
    id: String,
    revision: u64,
    old_root: String,
    new_root: String,
    name: String,
    session_revision: Option<u64>,
}
const JOURNAL: &str = "root-edit.json";
fn parse(bytes: &[u8]) -> io::Result<Option<Intent>> {
    let value: Option<Intent> =
        serde_json::from_slice(bytes).map_err(|_| api::fail("invalid_document"))?;
    if let Some(value) = &value {
        uuid(&value.id)?;
        next(value.revision)?;
        if let Some(revision) = value.session_revision {
            next(revision)?;
        }
        if value.schema_version != 1
            || normalize_absolute_path(&value.old_root).is_err()
            || normalize_absolute_path(&value.new_root).map_err(contract)? != value.new_root
        {
            return Err(api::fail("invalid_document"));
        }
    }
    Ok(value)
}
fn journal(tx: &Transaction<'_>, intent: Option<&Intent>) -> io::Result<()> {
    let bytes = serde_json::to_vec(&intent).map_err(|_| api::fail("invalid_document"))?;
    tx.write(JOURNAL, &bytes, |bytes| parse(bytes).map(|_| ()))
}
fn rebase(workspace: &mut Workspace, old: &str, new: &str) -> io::Result<()> {
    for pane in &mut workspace.panes {
        for tab in &mut pane.tabs {
            if let Some(path) = resolve_path_ref(old, &tab.path).map_err(contract)? {
                // Keep exact case (notably Linux paths through WSL UNC). The general
                // to_path_ref helper is case-insensitive and unsuitable here.
                tab.path = if path == new {
                    PathRef::Relative {
                        value: String::new(),
                    }
                } else if let Some(relative) =
                    path.strip_prefix(&format!("{}\\", new.trim_end_matches('\\')))
                {
                    PathRef::Relative {
                        value: relative.into(),
                    }
                } else {
                    PathRef::Absolute { value: path }
                };
            }
        }
    }
    Ok(())
}
fn prepare(
    tx: &Transaction<'_>,
    intent: &Intent,
) -> io::Result<(Project, Option<Session>, Manifest)> {
    let mut catalog = manifest(tx)?;
    let entry = catalog
        .projects
        .iter_mut()
        .find(|p| p.id == intent.id)
        .ok_or_else(|| api::fail("not_found"))?;
    if entry.root_path != intent.old_root && entry.root_path != intent.new_root {
        return Err(api::fail("revision_conflict"));
    }
    let bytes = tx
        .read(&format!("{}.project.json", intent.id))?
        .ok_or_else(|| api::fail("not_found"))?;
    let mut value = parse_project(&bytes).map_err(contract)?;
    if value.id != intent.id {
        return Err(api::fail("invalid_document"));
    }
    let new_revision = next(intent.revision)?;
    if value.revision == intent.revision && value.root_path == intent.old_root {
        rebase(&mut value.preset, &intent.old_root, &intent.new_root)?;
        value.root_path.clone_from(&intent.new_root);
        value.name.clone_from(&intent.name);
        value.revision = new_revision;
    } else if value.revision != new_revision
        || value.root_path != intent.new_root
        || value.name != intent.name
    {
        return Err(api::fail("revision_conflict"));
    }
    let mut saved = session(tx, &intent.id)?;
    match (&mut saved, intent.session_revision) {
        (None, None) => {}
        (Some(saved), Some(previous)) => {
            if saved.revision == previous && saved.project_revision <= intent.revision {
                rebase(&mut saved.workspace, &intent.old_root, &intent.new_root)?;
                saved.revision = next(previous)?;
                saved.project_revision = new_revision;
            } else if saved.revision != next(previous)? || saved.project_revision != new_revision {
                return Err(api::fail("revision_conflict"));
            }
        }
        _ => return Err(api::fail("revision_conflict")),
    }
    entry.root_path.clone_from(&intent.new_root);
    // Preflight all resulting sizes and validation before writing the intent.
    serialize_project(&value).map_err(contract)?;
    if let Some(saved) = &saved {
        serialize_session(saved).map_err(contract)?;
    }
    loading::bounded(&loaded(&value, saved.as_ref()))?;
    if serde_json::to_vec(&catalog)
        .map_err(|_| api::fail("invalid_document"))?
        .len()
        > MAX_PROJECT_BYTES
    {
        return Err(api::fail("payload_limit"));
    }
    Ok((value, saved, catalog))
}
fn loaded(value: &Project, saved: Option<&Session>) -> Loaded {
    Loaded {
        project: metadata(value),
        workspace: saved.map_or_else(|| value.preset.clone(), |s| s.workspace.clone()),
        session_revision: saved.map_or(0, |s| s.revision),
        source: if saved.is_some() {
            Source::Session
        } else {
            Source::Preset
        },
        read_only: false,
        warnings: Vec::new(),
    }
}
pub(super) fn recover(tx: &Transaction<'_>) -> io::Result<()> {
    let Some(bytes) = tx.read(JOURNAL)? else {
        return Ok(());
    };
    let Some(intent) = parse(&bytes)? else {
        return Ok(());
    };
    let (value, saved, catalog) = prepare(tx, &intent)?;
    // Rotate both live and backup generations. A pre-edit session backup cannot
    // safely be interpreted under the new root. Replaying is idempotent even if
    // termination happens between either rotation or clearing the intent.
    for _ in 0..2 {
        if let Some(saved) = &saved {
            let bytes = serialize_session(saved).map_err(contract)?;
            tx.write(&format!("{}.session.json", intent.id), &bytes, |bytes| {
                let original = parse_session(bytes).map_err(contract)?;
                if original.project_id != intent.id {
                    return Err(api::fail("invalid_document"));
                }
                Ok(())
            })?;
        }
        write_project(tx, &value)?;
        write_manifest(tx, &catalog)?;
    }
    journal(tx, None)
}
impl ProjectService {
    pub fn edit(&self, request: EditRequest) -> Result<Loaded, ProjectError> {
        self.transaction(|tx| {
            let value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_revision)?;
            let saved = session(tx, &value.id)?;
            if saved
                .as_ref()
                .is_some_and(|s| s.project_revision > value.revision)
            {
                return Err(api::fail("revision_conflict"));
            }
            let intent = Intent {
                schema_version: 1,
                id: value.id,
                revision: value.revision,
                old_root: value.root_path,
                new_root: normalize_absolute_path(&request.root_path).map_err(contract)?,
                name: request.name,
                session_revision: saved.map(|s| s.revision),
            };
            let (updated, saved, _) = prepare(tx, &intent)?;
            // Existing corrupt backups must not leave a new pending transaction
            // that blocks the catalog. Validate them before committing intent.
            if let Some(bytes) = tx.read_backup(&format!("{}.project.json", intent.id))?
                && parse_project(&bytes).map_err(contract)?.id != intent.id
            {
                return Err(api::fail("invalid_document"));
            }
            if let Some(bytes) = tx.read_backup(&format!("{}.session.json", intent.id))?
                && parse_session(&bytes).map_err(contract)?.project_id != intent.id
            {
                return Err(api::fail("invalid_document"));
            }
            if let Some(bytes) = tx.read_backup("manifest.json")? {
                parse_manifest(&bytes)?;
            }
            journal(tx, Some(&intent))?;
            recover(tx)?;
            Ok(loaded(&updated, saved.as_ref()))
        })
        .map_err(Into::into)
    }
}

#[cfg(all(test, windows))]
mod tests;
