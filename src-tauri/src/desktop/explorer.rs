use super::operation_runtime::{DesktopOperations, main_window};
use projectpane_core::{
    domain::{
        Fault, FaultCode,
        operations::{OperationSelection, OperationStatus, TransferPreview},
    },
    platform::explorer,
    services::{Broker, operations::OperationCloseState},
};
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tauri::{Emitter, WebviewWindow};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportAction {
    Reveal,
    Properties,
    ContextMenu,
    Clipboard,
    AbsolutePath,
    RelativePath,
    Drag,
}

#[tauri::command]
pub fn clipboard_sequence(window: WebviewWindow) -> Result<u32, Fault> {
    main_window(&window)?;
    explorer::clipboard_sequence()
}

#[tauri::command]
pub async fn export_to_explorer(
    window: WebviewWindow,
    broker: tauri::State<'_, Broker>,
    state: tauri::State<'_, DesktopOperations>,
    selection: OperationSelection,
    action: ExportAction,
    base_path: Option<String>,
) -> Result<bool, Fault> {
    main_window(&window)?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    selection.validate()?;
    if state.closing.load(Ordering::Acquire) {
        return Err(Fault::new(FaultCode::Busy));
    }
    if matches!(action, ExportAction::Properties) && selection.entry_ids.len() != 1 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    if selection.entry_ids.len() > 128 {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let mut paths = Vec::with_capacity(selection.entry_ids.len());
    for id in selection.entry_ids {
        paths.push(
            broker
                .resolve_activation(&selection.identity, id)
                .await?
                .path,
        );
    }
    if matches!(
        action,
        ExportAction::AbsolutePath | ExportAction::RelativePath
    ) {
        let relative_to = if matches!(action, ExportAction::RelativePath) {
            Some(
                base_path
                    .as_deref()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
            )
        } else {
            None
        };
        let text = explorer::path_text::format_paths(&paths, relative_to)?;
        let owner = window
            .hwnd()
            .map_err(|_| Fault::new(FaultCode::InvalidState))?
            .0 as usize;
        return tokio::task::spawn_blocking(move || {
            let _permit = _permit;
            explorer::path_text::copy_text(owner, &text)?;
            Ok(true)
        })
        .await
        .map_err(|_| Fault::new(FaultCode::IoError))?;
    }
    if matches!(action, ExportAction::ContextMenu) {
        let runtime = state.service().await?.state().await?;
        if !runtime.available
            || runtime.close_state != OperationCloseState::Open
            || runtime.operations.iter().any(|operation| {
                matches!(
                    operation.status,
                    OperationStatus::Queued
                        | OperationStatus::Running
                        | OperationStatus::AwaitingUserDecision
                        | OperationStatus::CancelRequested
                        | OperationStatus::InterruptedUnknown
                )
            })
        {
            return Err(Fault::new(FaultCode::Busy));
        }
        let lease = state.shell_menu.begin()?;
        if state.closing.load(Ordering::Acquire) {
            return Err(Fault::new(FaultCode::Busy));
        }
        let owner = window
            .hwnd()
            .map_err(|_| Fault::new(FaultCode::InvalidState))?
            .0 as usize;
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("projectpane-shell-menu".into())
            .spawn(move || {
                // Keep admission even when the requesting webview is gone.
                let _permit = _permit;
                let result = explorer::context_menu::show(lease, owner, paths, |phase| {
                    let _ = window.emit("projectpane-shell-menu", phase);
                });
                let phase = match &result {
                    Ok(true) => explorer::context_menu::MenuPhase::Invoked,
                    Ok(false) => explorer::context_menu::MenuPhase::Cancelled,
                    Err(_) => explorer::context_menu::MenuPhase::Failed,
                };
                let _ = window.emit("projectpane-shell-menu", phase);
                let _ = tx.send(result);
            })
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        return rx
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
    }
    if matches!(action, ExportAction::Drag) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || {
                let _ = tx.send(explorer::drag_copy(paths));
            })
            .map_err(|_| Fault::new(FaultCode::IoError))?;
        return rx.await.map_err(|_| Fault::new(FaultCode::IoError))?;
    }
    tokio::task::spawn_blocking(move || {
        match action {
            ExportAction::Reveal => explorer::reveal(paths)?,
            ExportAction::Properties => explorer::properties(paths.remove(0))?,
            ExportAction::Clipboard => explorer::copy_to_clipboard(paths)?,
            ExportAction::Drag
            | ExportAction::ContextMenu
            | ExportAction::AbsolutePath
            | ExportAction::RelativePath => unreachable!(),
        }
        Ok(true)
    })
    .await
    .map_err(|_| Fault::new(FaultCode::IoError))?
}

#[tauri::command]
pub async fn import_from_explorer(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    paths: Option<Vec<String>>,
) -> Result<TransferPreview, Fault> {
    main_window(&window)?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    let paths = match paths {
        Some(paths) => explorer::normalize_import_paths(paths)?,
        None => tokio::task::spawn_blocking(explorer::clipboard_paths)
            .await
            .map_err(|_| Fault::new(FaultCode::IoError))??,
    };
    // Uses the same native capture, identity revalidation, preview and collision rules as internal transfer.
    state.service().await?.capture_external(paths).await
}
