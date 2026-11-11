//! Desktop IPC contract (camelCase JSON). All CRUD/load calls wrap fields in `{request}`.
//! `list_projects()` -> `{projects: Metadata[], lastProjectId, warnings}`.
//! `create_project({name,rootPath,preset})` -> Loaded.
//! `load_project({projectId,source: "preset"|"session"})` -> Loaded.
//! `edit_project({projectId,expectedRevision,name,rootPath})` -> Loaded.
//! `rename_project({projectId,expectedRevision,name})` -> Metadata.
//! `duplicate_project({projectId,expectedRevision,name})` -> Loaded.
//! `remove_project({projectId,expectedRevision})` -> `{removedProjectId}`.
//! `save_project_preset({projectId,expectedRevision,workspace})` -> Metadata.
//! `save_project_session({projectId,expectedProjectRevision,expectedSessionRevision,workspace})`
//! -> `{projectId,projectRevision,sessionRevision}`. Missing session revision is 0.
//! Metadata = `{id,name,rootPath,revision}`; Warning = `{code,projectId: string|null}`.
//! Loaded = `{project: Metadata,workspace,sessionRevision,source,readOnly,warnings}`.
//! Service failures reject with `{code}`; malformed Tauri arguments may reject before dispatch.
//! Recovered documents are read-only; explicit create with the returned workspace saves a copy.
//! Close: main receives `{attempt,timeoutMs:2500}`, flushes, then acknowledges `{attempt,saved}`.
//! Failure/timeout keeps the window; only a matching successful acknowledgement allows exit.
use super::project_close_gate::{CloseGate, Completion};
use projectpane_core::{persistence::projects::ProjectStore, services::projects::*};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

pub struct DesktopProjects {
    service: Option<Arc<ProjectService>>,
    admission: Arc<tokio::sync::Semaphore>,
    closing: Mutex<CloseGate>,
}
impl DesktopProjects {
    pub fn initialize(app: &AppHandle) -> Self {
        let override_path = std::env::var_os("PROJECTPANE_DATA_DIR");
        let store = match override_path.as_deref() {
            Some(path) => ProjectStore::new(Path::new(""), Some(path)),
            None => app
                .path()
                .app_local_data_dir()
                .map_err(|_| std::io::Error::other("data_root_unavailable"))
                .and_then(|path| ProjectStore::new(&path, None)),
        };
        Self {
            service: store.ok().map(ProjectService::new).map(Arc::new),
            admission: Arc::new(tokio::sync::Semaphore::new(1)),
            closing: Mutex::new(CloseGate::default()),
        }
    }
}
async fn execute<T: Send + 'static>(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    operation: impl FnOnce(&ProjectService) -> Result<T, ProjectError> + Send + 'static,
) -> Result<T, ProjectError> {
    if window.label() != "main" {
        return Err(ProjectError {
            code: "unavailable",
        });
    }
    let service = Arc::clone(state.service.as_ref().ok_or(ProjectError {
        code: "unavailable",
    })?);
    let permit = Arc::clone(&state.admission)
        .try_acquire_owned()
        .map_err(|_| ProjectError { code: "busy" })?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        operation(&service)
    })
    .await
    .map_err(|_| ProjectError { code: "io_error" })?
}

#[tauri::command]
pub async fn list_favorites(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
) -> Result<Favorites, ProjectError> {
    execute(window, state, ProjectService::list_favorites).await
}
#[tauri::command]
pub async fn save_favorites(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: SaveFavorites,
) -> Result<Favorites, ProjectError> {
    execute(window, state, |service| service.save_favorites(request)).await
}
#[tauri::command]
pub async fn list_projects(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
) -> Result<Catalog, ProjectError> {
    execute(window, state, ProjectService::list).await
}
#[tauri::command]
pub async fn create_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: CreateRequest,
) -> Result<Loaded, ProjectError> {
    execute(window, state, |service| service.create(request)).await
}
#[tauri::command]
pub async fn load_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: LoadRequest,
) -> Result<Loaded, ProjectError> {
    execute(window, state, |service| service.load(request)).await
}
#[tauri::command]
pub async fn rename_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: NameRequest,
) -> Result<Metadata, ProjectError> {
    execute(window, state, |service| service.rename(request)).await
}
#[tauri::command]
pub async fn duplicate_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: NameRequest,
) -> Result<Loaded, ProjectError> {
    execute(window, state, |service| service.duplicate(request)).await
}
#[tauri::command]
pub async fn remove_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: RemoveRequest,
) -> Result<Removed, ProjectError> {
    execute(window, state, |service| service.remove(request)).await
}
#[tauri::command]
pub async fn save_project_preset(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: PresetRequest,
) -> Result<Metadata, ProjectError> {
    execute(window, state, |service| service.save_preset(request)).await
}
#[tauri::command]
pub async fn save_project_session(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: SessionRequest,
) -> Result<SessionSaved, ProjectError> {
    execute(window, state, |service| service.save_session(request)).await
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseRequest {
    attempt: u32,
    timeout_ms: u32,
    for_update: bool,
}

#[derive(Clone, serde::Serialize)]
struct CloseFailed {
    attempt: u32,
}

pub fn exit_allowed(app: &AppHandle) -> bool {
    app.try_state::<DesktopProjects>()
        .is_some_and(|state| state.closing.lock().is_ok_and(|gate| gate.exit_allowed()))
}

fn complete_close(app: &AppHandle, attempt: u32, saved: bool) -> Result<(), ProjectError> {
    let state = app.state::<DesktopProjects>();
    let completion = state
        .closing
        .lock()
        .map_err(|_| ProjectError {
            code: "unavailable",
        })?
        .complete(attempt, saved);
    match completion {
        Completion::Exit => {
            if super::updates::pending(app) {
                super::updates::after_save(app);
            } else {
                app.exit(0);
            }
        }
        Completion::Keep => {
            super::updates::fail_pending(app, "save_failed");
            eprintln!("project_close_kept_unsaved");
            let _ = app.emit_to("main", "projectpane-close-failed", CloseFailed { attempt });
        }
        Completion::Ignore => {}
    }
    Ok(())
}

pub fn request_close(app: &AppHandle) {
    if !super::operation_close::exit_allowed(app) {
        return;
    }
    if exit_allowed(app) && !super::updates::pending(app) {
        app.exit(0);
        return;
    }
    let state = app.state::<DesktopProjects>();
    let Some(attempt) = state.closing.lock().ok().and_then(|mut gate| gate.begin()) else {
        return;
    };
    if app
        .emit_to(
            "main",
            "projectpane-close-requested",
            CloseRequest {
                attempt,
                timeout_ms: 2500,
                for_update: super::updates::pending(app),
            },
        )
        .is_err()
    {
        eprintln!("project_close_event_failed");
        let _ = complete_close(app, attempt, false);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let _ = complete_close(&app, attempt, false);
    });
}

#[tauri::command]
pub fn project_close_ready(
    window: WebviewWindow,
    attempt: u32,
    saved: bool,
) -> Result<(), ProjectError> {
    if window.label() != "main" {
        return Err(ProjectError {
            code: "unavailable",
        });
    }
    let app = window.app_handle();
    if !super::operation_close::exit_allowed(app) {
        return Err(ProjectError { code: "busy" });
    }
    complete_close(app, attempt, saved)
}

#[tauri::command]
pub async fn edit_project(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopProjects>,
    request: EditRequest,
) -> Result<Loaded, ProjectError> {
    execute(window, state, |service| service.edit(request)).await
}
