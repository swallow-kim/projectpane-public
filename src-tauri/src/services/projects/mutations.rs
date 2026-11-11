use super::{documents::*, *};
use crate::{domain::projects::*, persistence::projects::Transaction};
use std::io;

impl ProjectService {
    pub fn create(&self, request: CreateRequest) -> Result<Loaded, ProjectError> {
        self.transaction(|tx| create(tx, request))
            .map_err(Into::into)
    }
    pub fn rename(&self, request: NameRequest) -> Result<Metadata, ProjectError> {
        self.transaction(|tx| {
            let mut value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_revision)?;
            value.name = request.name;
            value.revision = next(value.revision)?;
            write_project(tx, &value)?;
            Ok(metadata(&value))
        })
        .map_err(Into::into)
    }
    pub fn duplicate(&self, request: NameRequest) -> Result<Loaded, ProjectError> {
        self.transaction(|tx| {
            let value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_revision)?;
            create(
                tx,
                CreateRequest {
                    name: request.name,
                    root_path: value.root_path,
                    preset: value.preset,
                },
            )
        })
        .map_err(Into::into)
    }
    pub fn remove(&self, request: RemoveRequest) -> Result<Removed, ProjectError> {
        self.transaction(|tx| {
            let value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_revision)?;
            let mut catalog = manifest(tx)?;
            catalog
                .projects
                .retain(|item| item.id != request.project_id);
            // Clear selection first: a crash can leave an unselected valid project,
            // never a selected ID whose manifest membership has already disappeared.
            if settings(tx)?.last_project_id.as_deref() == Some(&request.project_id) {
                select(tx, None)?;
            }
            write_manifest(tx, &catalog)?;
            // Files are intentional recoverable orphans; the source root is never touched.
            Ok(Removed {
                removed_project_id: request.project_id,
            })
        })
        .map_err(Into::into)
    }
    pub fn save_preset(&self, request: PresetRequest) -> Result<Metadata, ProjectError> {
        self.transaction(|tx| {
            let mut value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_revision)?;
            value.preset = request.workspace;
            value.revision = next(value.revision)?;
            write_project(tx, &value)?;
            Ok(metadata(&value))
        })
        .map_err(Into::into)
    }
    pub fn save_session(&self, request: SessionRequest) -> Result<SessionSaved, ProjectError> {
        self.transaction(|tx| {
            let value = project(tx, &request.project_id)?;
            expect(value.revision, request.expected_project_revision)?;
            let previous = session(tx, &value.id)?;
            let revision = previous.as_ref().map_or(0, |session| session.revision);
            expect(revision, request.expected_session_revision)?;
            if previous.is_some_and(|session| session.project_revision > value.revision) {
                return Err(api::fail("revision_conflict"));
            }
            let saved = Session {
                schema_version: 1,
                project_id: value.id.clone(),
                project_revision: value.revision,
                revision: next(revision)?,
                workspace: request.workspace,
            };
            let bytes = serialize_session(&saved).map_err(contract)?;
            tx.write(&format!("{}.session.json", value.id), &bytes, |bytes| {
                let original = parse_session(bytes).map_err(contract)?;
                if original.project_id != value.id {
                    return Err(api::fail("invalid_document"));
                }
                Ok(())
            })?;
            Ok(SessionSaved {
                project_id: value.id,
                project_revision: value.revision,
                session_revision: saved.revision,
            })
        })
        .map_err(Into::into)
    }
}

fn create(tx: &Transaction<'_>, request: CreateRequest) -> io::Result<Loaded> {
    let mut catalog = manifest(tx)?;
    if catalog.projects.len() >= MAX_PROJECTS {
        return Err(api::fail("catalog_limit"));
    }
    settings(tx)?;
    let id = id::new_id()?;
    // A UUID collision must not adopt an orphan or replace an existing document.
    if tx.read(&format!("{id}.project.json"))?.is_some()
        || tx.read(&format!("{id}.session.json"))?.is_some()
    {
        return Err(api::fail("revision_conflict"));
    }
    let value = Project {
        schema_version: 1,
        id: id.clone(),
        revision: 0,
        name: request.name,
        root_path: request.root_path,
        preset: request.preset,
    };
    let mut loaded = Loaded {
        project: metadata(&value),
        workspace: value.preset.clone(),
        session_revision: 0,
        source: Source::Preset,
        read_only: false,
        warnings: Vec::new(),
    };
    loading::bounded(&loaded)?;
    write_project(tx, &value)?;
    catalog.projects.push(ManifestProject {
        id: id.clone(),
        root_path: value.root_path.clone(),
    });
    write_manifest(tx, &catalog)?;
    if select(tx, Some(id.clone())).is_err() {
        loaded.warnings.push(Warning {
            code: "selection_save_failed".into(),
            project_id: Some(id),
        });
    }
    Ok(loaded)
}
