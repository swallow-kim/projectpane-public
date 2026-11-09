//! Bounded copy-only desktop commands. Every filesystem call runs off the UI thread.
use super::operation_runtime::{DesktopOperations, main_window};
use projectpane_core::{
    domain::{Fault, FaultCode, operations::OperationSelection},
    platform::explorer,
    services::{Broker, copy_jobs::*},
};
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tauri::WebviewWindow;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Id {
    pub id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Append {
    id: String,
    selection: Option<OperationSelection>,
    paths: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    id: String,
    offset: u64,
    limit: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportAction {
    Clipboard,
    Drag,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    id: String,
    action: ExportAction,
}
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Fault> + Send + 'static,
) -> Result<T, Fault> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?
}
async fn ready(state: &DesktopOperations) -> Result<std::sync::Arc<CopyJobs>, Fault> {
    if state.closing.load(Ordering::Acquire) {
        return Err(Fault::new(FaultCode::Busy));
    }
    copies(state).await
}
async fn copies(state: &DesktopOperations) -> Result<std::sync::Arc<CopyJobs>, Fault> {
    let _ = state.service().await;
    state
        .copies
        .get()
        .cloned()
        .unwrap_or_else(|| Err(Fault::new(FaultCode::InvalidState)))
}
#[tauri::command]
pub async fn begin_copy_capture(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
) -> Result<Capture, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    blocking(move || jobs.begin_capture()).await
}
#[tauri::command]
pub async fn append_copy_capture(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    broker: tauri::State<'_, Broker>,
    request: Append,
) -> Result<Capture, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    let paths = match (request.selection, request.paths) {
        (Some(selection), None) => {
            selection.validate()?;
            if selection.entry_ids.len() > CHUNK_ITEMS {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            let identity = &selection.identity;
            let broker = &*broker;
            resolve_bounded(selection.entry_ids, |ids| {
                broker.resolve_activations(identity, ids)
            })
            .await?
            .into_iter()
            .map(|target| target.path)
            .collect()
        }
        (None, Some(paths)) => explorer::normalize_import_paths(paths)?,
        _ => return Err(Fault::new(FaultCode::InvalidRequest)),
    };
    blocking(move || jobs.append_bounded(&request.id, paths)).await
}
#[tauri::command]
pub async fn seal_copy_capture(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Id,
) -> Result<Capture, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    blocking(move || jobs.seal(&request.id)).await
}
#[tauri::command]
pub async fn discard_copy_capture(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Id,
) -> Result<(), Fault> {
    main_window(&window)?;
    let jobs = copies(&state).await?;
    blocking(move || jobs.discard_capture(&request.id)).await
}
#[tauri::command]
pub async fn capture_copy_clipboard(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
) -> Result<Capture, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    blocking(move || {
        let capture = jobs.begin_capture()?;
        let result = explorer::visit_clipboard_paths(|path| {
            jobs.append(&capture.id, vec![path]).map(|_| ())
        });
        match result {
            Ok(()) => jobs.seal(&capture.id),
            Err(error) => {
                let _ = jobs.discard_capture(&capture.id);
                Err(error)
            }
        }
    })
    .await
}
#[tauri::command]
pub async fn begin_copy_job(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: BeginJob,
) -> Result<Snapshot, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    blocking(move || jobs.begin(request)).await
}
#[tauri::command]
pub async fn get_copy_job(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Id,
) -> Result<Snapshot, Fault> {
    main_window(&window)?;
    let jobs = copies(&state).await?;
    blocking(move || jobs.snapshot(&request.id)).await
}
#[tauri::command]
pub async fn list_copy_jobs(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
) -> Result<Vec<Snapshot>, Fault> {
    main_window(&window)?;
    let jobs = copies(&state).await?;
    blocking(move || jobs.list()).await
}
#[tauri::command]
pub async fn cancel_copy_job(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Id,
) -> Result<Snapshot, Fault> {
    main_window(&window)?;
    let jobs = copies(&state).await?;
    blocking(move || jobs.cancel(&request.id)).await
}
#[tauri::command]
pub async fn read_copy_job_outcomes(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Page,
) -> Result<OutcomePage, Fault> {
    main_window(&window)?;
    let jobs = copies(&state).await?;
    blocking(move || jobs.outcomes(&request.id, request.offset, request.limit)).await
}
#[tauri::command]
pub async fn export_copy_capture(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: Export,
) -> Result<bool, Fault> {
    main_window(&window)?;
    let jobs = ready(&state).await?;
    let paths = blocking(move || jobs.export_paths(&request.id)).await?;
    match request.action {
        ExportAction::Clipboard => {
            blocking(move || {
                explorer::copy_to_clipboard(paths)?;
                Ok(true)
            })
            .await
        }
        ExportAction::Drag => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            window
                .run_on_main_thread(move || {
                    let _ = tx.send(explorer::drag_copy(paths));
                })
                .map_err(|_| Fault::new(FaultCode::IoError))?;
            rx.await.map_err(|_| Fault::new(FaultCode::IoError))?
        }
    }
}

// Only read-only broker resolution is retried, never a filesystem mutation.
async fn resolve_bounded<T, F, Fut>(ids: Vec<String>, mut resolve: F) -> Result<Vec<T>, Fault>
where
    F: FnMut(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<T>, Fault>>,
{
    let mut targets = Vec::new();
    let mut offset = 0;
    let mut size = CHUNK_ITEMS;
    while offset < ids.len() {
        let end = (offset + size).min(ids.len());
        match resolve(ids[offset..end].to_vec()).await {
            Ok(chunk) => {
                if chunk.len() != end - offset {
                    return Err(Fault::new(FaultCode::ProtocolError));
                }
                targets.extend(chunk);
                offset = end;
            }
            Err(error) if error.code == FaultCode::PayloadLimit && end - offset > 1 => {
                size = (end - offset) / 2
            }
            Err(error) => return Err(error),
        }
    }
    Ok(targets)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn oversized_read_only_activation_packets_split_without_reordering() {
        let ids = (0..128).map(|n| n.to_string()).collect::<Vec<_>>();
        let actual = resolve_bounded(ids.clone(), |chunk| async move {
            if chunk.len() > 3 {
                Err(Fault::new(FaultCode::PayloadLimit))
            } else {
                Ok(chunk)
            }
        })
        .await
        .unwrap();
        assert_eq!(actual, ids);
    }
    #[tokio::test]
    async fn activation_does_not_retry_other_failures() {
        let mut calls = 0;
        let result = resolve_bounded(vec!["one".into(), "two".into()], |_: Vec<String>| {
            calls += 1;
            std::future::ready(Err::<Vec<String>, _>(Fault::new(FaultCode::StaleRequest)))
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
}
