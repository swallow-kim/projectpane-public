use super::{
    runtime::DesktopState,
    startup::{StartupResponse, StartupState},
};
use projectpane_core::{
    domain::{ActivationReply, Fault, Identity, ListingView, LocateReply, Reply},
    persistence::{EventKind, LogEvent},
    services::Broker,
};

#[tauri::command]
pub async fn startup_config(
    state: tauri::State<'_, StartupState>,
) -> Result<StartupResponse, Fault> {
    state.response().await
}

#[tauri::command]
pub async fn start_listing(
    broker: tauri::State<'_, Broker>,
    desktop: tauri::State<'_, DesktopState>,
    identity: Identity,
    path: String,
    view: Option<ListingView>,
) -> Result<Reply, Fault> {
    let pane = identity.pane;
    let result = broker
        .start_visible_view(identity, path, view.unwrap_or_default())
        .await;
    log_result(
        &desktop,
        pane,
        EventKind::ListingStarted,
        EventKind::ListingFailed,
        &result,
    );
    result
}

#[tauri::command]
pub async fn set_listing_view(
    broker: tauri::State<'_, Broker>,
    identity: Identity,
    view: ListingView,
) -> Result<Reply, Fault> {
    broker.set_view(identity, view).await
}

#[tauri::command]
pub async fn locate_entry(
    broker: tauri::State<'_, Broker>,
    identity: Identity,
    entry_id: String,
) -> Result<LocateReply, Fault> {
    broker.locate(identity, entry_id).await
}

#[tauri::command]
pub async fn activate_entry(
    broker: tauri::State<'_, Broker>,
    identity: Identity,
    entry_id: String,
) -> Result<ActivationReply, Fault> {
    broker.activate(identity, entry_id).await
}

#[tauri::command]
pub async fn read_listing(
    broker: tauri::State<'_, Broker>,
    desktop: tauri::State<'_, DesktopState>,
    identity: Identity,
    offset: Option<usize>,
) -> Result<Reply, Fault> {
    let pane = identity.pane;
    let result = match offset {
        Some(offset) => broker.read_visible(identity, offset).await,
        None => broker.progress(identity).await,
    };
    if result.is_err() {
        log_result(
            &desktop,
            pane,
            EventKind::ListingStarted,
            EventKind::ListingFailed,
            &result,
        );
    }
    result
}

#[tauri::command]
pub async fn cancel_listing(
    broker: tauri::State<'_, Broker>,
    desktop: tauri::State<'_, DesktopState>,
    identity: Identity,
) -> Result<(), Fault> {
    let result = broker.cancel(&identity).await;
    if let Err(error) = &result
        && let Some(logger) = &desktop.logger
    {
        logger.record(LogEvent {
            event: EventKind::CancelFailed,
            pane: u8::try_from(identity.pane).ok().filter(|pane| *pane < 4),
            code: Some(error.code),
        });
    }
    result
}

fn log_result<T>(
    desktop: &DesktopState,
    pane: usize,
    success: EventKind,
    failure: EventKind,
    result: &Result<T, Fault>,
) {
    if let Some(logger) = &desktop.logger {
        let (event, code) = match result {
            Ok(_) => (success, None),
            Err(error) => (failure, Some(error.code)),
        };
        logger.record(LogEvent {
            event,
            pane: u8::try_from(pane).ok().filter(|pane| *pane < 4),
            code,
        });
    }
}
