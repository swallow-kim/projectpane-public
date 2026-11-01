use super::{documents::*, *};
use crate::{domain::projects::*, persistence::projects::Transaction};
use std::io;

impl ProjectService {
    pub fn list(&self) -> Result<Catalog, ProjectError> {
        self.store
            .transaction(|tx| {
                let manifest = manifest(tx)?;
                let mut warnings = Vec::new();
                let mut projects = Vec::new();
                for entry in &manifest.projects {
                    let id = &entry.id;
                    match read_project(tx, id) {
                        Ok((value, repaired)) => {
                            projects.push(metadata(&value));
                            if repaired {
                                warnings.push(warning("project_recovered_read_only", id));
                            }
                        }
                        Err(error) => warnings.push(warning(ProjectError::from(error).code, id)),
                    }
                }
                let last_project_id = match settings(tx) {
                    Ok(settings) => settings
                        .last_project_id
                        .filter(|id| projects.iter().any(|project| &project.id == id)),
                    Err(error) => {
                        warnings.push(Warning {
                            code: ProjectError::from(error).code.into(),
                            project_id: None,
                        });
                        None
                    }
                };
                let catalog = Catalog {
                    projects,
                    last_project_id,
                    warnings,
                };
                bounded(&catalog)?;
                Ok(catalog)
            })
            .map_err(Into::into)
    }
    pub fn load(&self, request: LoadRequest) -> Result<Loaded, ProjectError> {
        self.store
            .transaction(|tx| {
                known(tx, &request.project_id)?;
                let (project, project_repaired) = read_project(tx, &request.project_id)?;
                let mut loaded = Loaded {
                    project: metadata(&project),
                    workspace: project.preset,
                    session_revision: 0,
                    source: Source::Preset,
                    read_only: project_repaired,
                    warnings: Vec::new(),
                };
                if project_repaired {
                    loaded
                        .warnings
                        .push(warning("project_recovered_read_only", &project.id));
                }
                match read_session(tx, &project.id) {
                    Ok(Some((session, repaired))) => {
                        loaded.session_revision = session.revision;
                        loaded.read_only |= repaired;
                        if repaired {
                            loaded
                                .warnings
                                .push(warning("session_recovered_read_only", &project.id));
                        }
                        // The manifest root is immutable and was checked by read_project.
                        // Rename/preset revisions therefore leave older sessions meaningful;
                        // the next session save still requires the current project revision.
                        if session.project_revision > project.revision {
                            loaded.read_only = true;
                            loaded
                                .warnings
                                .push(warning("session_revision_mismatch", &project.id));
                        } else if request.source == Source::Session {
                            loaded.workspace = session.workspace;
                            loaded.source = Source::Session;
                        }
                    }
                    Ok(None) => {
                        if request.source == Source::Session {
                            loaded
                                .warnings
                                .push(warning("session_missing", &project.id));
                        }
                    }
                    Err(error) => {
                        loaded.read_only = true;
                        loaded
                            .warnings
                            .push(warning(ProjectError::from(error).code, &project.id));
                    }
                }
                bounded(&loaded)?;
                if select(tx, Some(project.id.clone())).is_err() {
                    loaded
                        .warnings
                        .push(warning("selection_save_failed", &project.id));
                }
                Ok(loaded)
            })
            .map_err(Into::into)
    }
}

pub(super) fn bounded(value: &impl serde::Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| api::fail("invalid_document"))?;
    // Reserve space for one possible post-write selection warning.
    if bytes.len() > MAX_PROJECT_BYTES - 256 {
        return Err(api::fail("payload_limit"));
    }
    Ok(())
}
fn warning(code: &str, id: &str) -> Warning {
    Warning {
        code: code.into(),
        project_id: Some(id.into()),
    }
}
fn read_project(tx: &Transaction<'_>, id: &str) -> io::Result<(Project, bool)> {
    let root = known(tx, id)?;
    let name = format!("{id}.project.json");
    let bytes = tx.read(&name)?.ok_or_else(|| api::fail("not_found"))?;
    let parsed = parse_project(&bytes);
    match parsed {
        Ok(project) if project.id == id && project.root_path == root => {
            return Ok((project, false));
        }
        Ok(_) => return Err(api::fail("invalid_document")),
        Err(ProjectContractError::UnsupportedSchema) => {
            return Err(api::fail("unsupported_schema"));
        }
        Err(_) => {}
    }
    // Typed parsing still rejects duplicate/unknown properties before any repair.
    if let Ok(mut project) = serde_json::from_slice::<Project>(&bytes) {
        let value =
            serde_json::to_value(&project.preset).map_err(|_| api::fail("invalid_document"))?;
        if let Ok(recovered) = recover_workspace(&value) {
            project.preset = recovered.workspace;
            if project.id == id && project.root_path == root && serialize_project(&project).is_ok()
            {
                return Ok((project, true));
            }
        }
    }
    if let Some(backup) = tx.read_backup(&name)? {
        let project = parse_project(&backup).map_err(contract)?;
        if project.id == id && project.root_path == root {
            return Ok((project, true));
        }
    }
    Err(api::fail("invalid_document"))
}
fn read_session(tx: &Transaction<'_>, id: &str) -> io::Result<Option<(Session, bool)>> {
    let name = format!("{id}.session.json");
    let Some(bytes) = tx.read(&name)? else {
        return Ok(None);
    };
    match parse_session(&bytes) {
        Ok(session) if session.project_id == id => return Ok(Some((session, false))),
        Ok(_) => return Err(api::fail("invalid_document")),
        Err(ProjectContractError::UnsupportedSchema) => {
            return Err(api::fail("unsupported_schema"));
        }
        Err(_) => {}
    }
    if let Ok(mut session) = serde_json::from_slice::<Session>(&bytes) {
        let value =
            serde_json::to_value(&session.workspace).map_err(|_| api::fail("invalid_document"))?;
        if let Ok(recovered) = recover_workspace(&value) {
            session.workspace = recovered.workspace;
            if session.project_id == id && serialize_session(&session).is_ok() {
                return Ok(Some((session, true)));
            }
        }
    }
    if let Some(backup) = tx.read_backup(&name)? {
        let session = parse_session(&backup).map_err(contract)?;
        if session.project_id == id {
            return Ok(Some((session, true)));
        }
    }
    Err(api::fail("invalid_document"))
}
