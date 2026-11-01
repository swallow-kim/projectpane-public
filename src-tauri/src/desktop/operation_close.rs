use super::{
    operation_runtime::{DesktopOperations, main_window},
    projects,
};
use projectpane_core::{
    domain::{Fault, FaultCode},
    services::operations::OperationCloseState,
};
use serde::{Deserialize, Serialize};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseChoice {
    Wait,
    Cancel,
    Continue,
    ExitWithoutResults,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseNotice {
    close_state: OperationCloseState,
}

pub fn exit_allowed(app: &AppHandle) -> bool {
    app.try_state::<DesktopOperations>()
        .is_some_and(|state| state.exit_allowed.load(Ordering::Acquire))
}

fn finish(app: &AppHandle) {
    let state = app.state::<DesktopOperations>();
    state.exit_allowed.store(true, Ordering::Release);
    let _ = app.emit_to(
        "main",
        "projectpane-operation-close",
        CloseNotice {
            close_state: OperationCloseState::Stopped,
        },
    );
    projects::request_close(app);
}

pub fn request_close(app: &AppHandle) {
    if exit_allowed(app) {
        projects::request_close(app);
        return;
    }
    let state = app.state::<DesktopOperations>();
    if state.closing.swap(true, Ordering::AcqRel) {
        return;
    }
    state.shell_menu.cancel();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<DesktopOperations>();
        if state.shell_menu.active() {
            let _ = app.emit_to(
                "main",
                "projectpane-operation-close",
                CloseNotice {
                    close_state: OperationCloseState::Waiting,
                },
            );
            while state.shell_menu.active() && state.closing.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !state.closing.load(Ordering::Acquire) {
                return;
            }
        }
        let service = match state.service().await {
            Ok(service) => service,
            Err(_) => {
                finish(&app);
                return;
            }
        };
        if service.ask_close().await.is_err() {
            let _ = app.emit_to(
                "main",
                "projectpane-operation-close",
                CloseNotice {
                    close_state: OperationCloseState::Blocked,
                },
            );
            return;
        }
        let mut previous = None;
        while state.closing.load(Ordering::Acquire) {
            let close = service
                .state()
                .await
                .map_or(OperationCloseState::Blocked, |status| status.close_state);
            if close == OperationCloseState::Stopped {
                finish(&app);
                return;
            }
            if previous != Some(close) {
                let _ = app.emit_to(
                    "main",
                    "projectpane-operation-close",
                    CloseNotice { close_state: close },
                );
                previous = Some(close);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });
}

#[tauri::command]
pub async fn operation_close_choice(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopOperations>,
    choice: CloseChoice,
) -> Result<(), Fault> {
    main_window(&window)?;
    if !state.closing.load(Ordering::Acquire) {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    let service = state.service().await?;
    match choice {
        CloseChoice::Wait => {
            service.request_close(false).await?;
        }
        CloseChoice::Cancel => {
            service.request_close(true).await?;
        }
        CloseChoice::Continue => {
            if !matches!(
                service.state().await?.close_state,
                OperationCloseState::Blocked | OperationCloseState::ResultsUnsaved
            ) {
                service.keep_open().await?;
            }
            state.closing.store(false, Ordering::Release);
        }
        CloseChoice::ExitWithoutResults => {
            service.acknowledge_unsaved_results().await?;
        }
    }
    Ok(())
}
