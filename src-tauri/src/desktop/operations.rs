use super::operation_runtime::{DesktopOperations, main_window};
use projectpane_core::{
    domain::{
        Fault, FaultCode,
        operations::{
            CaptureTransfer, OperationReport, PrepareOperation, PrepareTransfer, TransferPreview,
        },
    },
    services::{
        Broker,
        operations::{OperationPreview, OperationRuntimeState, OperationSnapshot},
    },
};
use serde::Deserialize;
use tauri::WebviewWindow;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationId {
    id: String,
}
impl OperationId {
    fn validate(&self) -> Result<(), Fault> {
        if self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn get_operation_state(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
) -> Result<OperationRuntimeState, Fault> {
    main_window(&window)?;
    state.service().await?.state().await
}

#[tauri::command]
pub async fn prepare_operation(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    broker: tauri::State<'_, Broker>,
    request: PrepareOperation,
) -> Result<OperationPreview, Fault> {
    main_window(&window)?;
    request.validate()?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    let mut targets = Vec::new();
    if let Some(selection) = request.selection() {
        for id in &selection.entry_ids {
            targets.push(
                broker
                    .resolve_activation(&selection.identity, id.clone())
                    .await?,
            );
        }
    }
    state.service().await?.prepare(request, targets).await
}

#[tauri::command]
pub async fn execute_operation(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: OperationId,
) -> Result<OperationSnapshot, Fault> {
    main_window(&window)?;
    request.validate()?;
    state.service().await?.submit(request.id).await
}

#[tauri::command]
pub async fn capture_transfer(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    broker: tauri::State<'_, Broker>,
    request: CaptureTransfer,
) -> Result<TransferPreview, Fault> {
    main_window(&window)?;
    request.validate()?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    let mut targets = Vec::with_capacity(request.selection.entry_ids.len());
    for id in &request.selection.entry_ids {
        targets.push(
            broker
                .resolve_activation(&request.selection.identity, id.clone())
                .await?,
        );
    }
    state
        .service()
        .await?
        .capture_transfer(request, targets)
        .await
}

#[tauri::command]
pub async fn prepare_transfer(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: PrepareTransfer,
) -> Result<OperationPreview, Fault> {
    main_window(&window)?;
    request.validate()?;
    let _permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    state.service().await?.prepare_transfer(request).await
}

#[tauri::command]
pub async fn cancel_operation(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: OperationId,
) -> Result<OperationSnapshot, Fault> {
    main_window(&window)?;
    request.validate()?;
    state.service().await?.cancel(request.id).await
}

#[tauri::command]
pub async fn discard_operation(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: OperationId,
) -> Result<(), Fault> {
    main_window(&window)?;
    request.validate()?;
    state.service().await?.discard(request.id).await
}

#[tauri::command]
pub async fn get_operation_report(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: OperationId,
) -> Result<Option<OperationReport>, Fault> {
    main_window(&window)?;
    request.validate()?;
    state.service().await?.report(request.id).await
}

#[tauri::command]
pub async fn forget_operation(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    request: OperationId,
) -> Result<String, Fault> {
    main_window(&window)?;
    request.validate()?;
    state.service().await?.forget(request.id.clone()).await?;
    Ok(request.id)
}
