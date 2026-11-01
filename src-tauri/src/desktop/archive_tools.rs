use super::operation_runtime::{DesktopOperations, main_window};
use projectpane_core::{
    domain::{
        Fault, FaultCode,
        operations::{OperationSelection, OperationStatus},
    },
    platform::archive_tools::{self, AvailableTool, Tool},
    services::{Broker, operations::OperationCloseState},
};
use tauri::WebviewWindow;
#[tauri::command]
pub async fn list_archive_tools(window: WebviewWindow) -> Result<Vec<AvailableTool>, Fault> {
    main_window(&window)?;
    tokio::task::spawn_blocking(archive_tools::available)
        .await
        .map_err(|_| Fault::new(FaultCode::IoError))
}
#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "Tauri injects window, broker and operation state in addition to the narrow archive arguments"
)]
pub async fn launch_archive_tool(
    window: WebviewWindow,
    broker: tauri::State<'_, Broker>,
    state: tauri::State<'_, DesktopOperations>,
    selection: OperationSelection,
    tool: Tool,
    parent: String,
    name: String,
    extract: bool,
) -> Result<bool, Fault> {
    main_window(&window)?;
    selection.validate()?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    let runtime = state.service().await?.state().await?;
    if state.closing.load(std::sync::atomic::Ordering::Acquire)
        || !runtime.available
        || runtime.close_state != OperationCloseState::Open
        || runtime.operations.iter().any(|o| {
            matches!(
                o.status,
                OperationStatus::Queued
                    | OperationStatus::Running
                    | OperationStatus::CancelRequested
                    | OperationStatus::InterruptedUnknown
            )
        })
    {
        return Err(Fault::new(FaultCode::Busy));
    }
    let mut paths = vec![];
    for id in selection.entry_ids {
        paths.push(
            broker
                .resolve_activation(&selection.identity, id)
                .await?
                .path,
        );
    }
    tokio::task::spawn_blocking(move || {
        let _permit = _permit;
        archive_tools::launch(tool, paths, parent, name, extract)?;
        Ok(true)
    })
    .await
    .map_err(|_| Fault::new(FaultCode::IoError))?
}
