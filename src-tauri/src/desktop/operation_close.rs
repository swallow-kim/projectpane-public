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
    app.try_state::<DesktopOperations>().is_some_and(|state| {
        state.exit_allowed.load(Ordering::Acquire)
            && !state
                .copies
                .get()
                .is_some_and(|r| r.as_ref().is_ok_and(|copies| copies.active()))
    })
}

fn finish(app: &AppHandle) {
    if app
        .state::<DesktopOperations>()
        .copies
        .get()
        .is_some_and(|r| r.as_ref().is_ok_and(|c| c.active()))
    {
        return;
    }
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
            let started = std::time::Instant::now();
            let _ = app.emit_to(
                "main",
                "projectpane-operation-close",
                CloseNotice {
                    close_state: OperationCloseState::Waiting,
                },
            );
            while state.shell_menu.active() && state.closing.load(Ordering::Acquire) {
                if super::updates::pending(&app) && started.elapsed() >= Duration::from_secs(5) {
                    super::updates::fail_pending(&app, "shell_menu_busy");
                    state.closing.store(false, Ordering::Release);
                    let _ = app.emit_to(
                        "main",
                        "projectpane-operation-close",
                        CloseNotice {
                            close_state: OperationCloseState::Blocked,
                        },
                    );
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !state.closing.load(Ordering::Acquire) {
                return;
            }
        }
        let _ = state.service().await;
        // Do not stop the legacy lane (which cannot reopen after Stopped) until
        // copy jobs have settled. The close dialog can cancel or continue meanwhile.
        if let Some(Ok(copies)) = state.copies.get() {
            let _ = copies.close(false);
            if copies.active() {
                let _ = app.emit_to(
                    "main",
                    "projectpane-operation-close",
                    CloseNotice {
                        close_state: OperationCloseState::Deciding,
                    },
                );
                while copies.active() && state.closing.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if !state.closing.load(Ordering::Acquire) {
                    return;
                }
            }
        }
        let service = match state.service().await {
            Ok(service) => service,
            Err(_) => {
                if super::updates::pending(&app) {
                    super::updates::fail_pending(&app, "operations_unconfirmed");
                    state.closing.store(false, Ordering::Release);
                    return;
                }
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
    let service = match state.service().await {
        Ok(service) => service,
        Err(error) => {
            if let Some(Ok(copies)) = state.copies.get() {
                if copy_close_fallback(copies, choice)? {
                    super::updates::cancel(window.app_handle());
                    state.closing.store(false, Ordering::Release);
                }
                return Ok(());
            }
            return Err(error);
        }
    };
    match choice {
        CloseChoice::Wait => {
            if let Some(Ok(copies)) = state.copies.get() {
                copies.close(false)?;
            }
            if state
                .copies
                .get()
                .is_some_and(|r| r.as_ref().is_ok_and(|c| c.active()))
            {
                return Ok(());
            }
            service.request_close(false).await?;
        }
        CloseChoice::Cancel => {
            if let Some(Ok(copies)) = state.copies.get() {
                copies.close(true)?;
            }
            if state
                .copies
                .get()
                .is_some_and(|r| r.as_ref().is_ok_and(|c| c.active()))
            {
                return Ok(());
            }
            service.request_close(true).await?;
        }
        CloseChoice::Continue => {
            if let Some(Ok(copies)) = state.copies.get() {
                copies.keep_open()?;
            }
            super::updates::cancel(window.app_handle());
            if !matches!(
                service.state().await?.close_state,
                OperationCloseState::Blocked | OperationCloseState::ResultsUnsaved
            ) {
                service.keep_open().await?;
            }
            state.closing.store(false, Ordering::Release);
        }
        CloseChoice::ExitWithoutResults => {
            if state
                .copies
                .get()
                .is_some_and(|r| r.as_ref().is_ok_and(|c| c.active()))
            {
                return Err(Fault::new(FaultCode::Busy));
            }
            if super::updates::pending(window.app_handle()) {
                return Err(Fault::new(FaultCode::InvalidState));
            }
            service.acknowledge_unsaved_results().await?;
        }
    }
    Ok(())
}

// Copy jobs remain controllable even if the independent legacy journal is corrupt.
fn copy_close_fallback(
    copies: &projectpane_core::services::copy_jobs::CopyJobs,
    choice: CloseChoice,
) -> Result<bool, Fault> {
    match choice {
        CloseChoice::Wait => {
            copies.close(false)?;
            Ok(false)
        }
        CloseChoice::Cancel => {
            copies.close(true)?;
            Ok(false)
        }
        CloseChoice::Continue => {
            copies.keep_open()?;
            Ok(true)
        }
        CloseChoice::ExitWithoutResults if copies.active() => Err(Fault::new(FaultCode::Busy)),
        CloseChoice::ExitWithoutResults => Ok(false),
    }
}
#[cfg(test)]
mod copy_close_tests {
    use super::*;
    #[test]
    fn legacy_failure_fallback_can_cancel_wait_and_continue_copies() {
        let root = tempfile::tempdir().unwrap();
        let copies =
            projectpane_core::services::copy_jobs::CopyJobs::open(root.path(), None).unwrap();
        assert!(!copy_close_fallback(&copies, CloseChoice::Cancel).unwrap());
        assert!(copies.begin_capture().is_err());
        assert!(copy_close_fallback(&copies, CloseChoice::Continue).unwrap());
        assert!(copies.begin_capture().is_ok());
        assert!(!copy_close_fallback(&copies, CloseChoice::Wait).unwrap());
        assert!(copies.begin_capture().is_err());
    }
}
