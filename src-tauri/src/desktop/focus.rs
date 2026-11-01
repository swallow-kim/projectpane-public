use super::runtime::{DesktopState, record};
use projectpane_core::persistence::EventKind;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::{
    sync::oneshot,
    time::{Instant, sleep, timeout_at},
};

const CONFIRM_LIMIT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn focus_existing(app: &AppHandle) {
    let app = app.clone();
    let started = Instant::now();
    let logger = app
        .try_state::<DesktopState>()
        .and_then(|state| state.logger.clone());
    record(logger.as_ref(), EventKind::DuplicateInstance, None);
    tauri::async_runtime::spawn(async move {
        let focused = confirm(&app, started).await;
        let event = if focused {
            EventKind::InstanceFocused
        } else {
            EventKind::InstanceFocusFailed
        };
        record(logger.as_ref(), event, None);
    });
}

async fn confirm(app: &AppHandle, started: Instant) -> bool {
    let deadline = started + CONFIRM_LIMIT;
    if on_main(app, deadline, activate).await.is_err() {
        return false;
    }
    loop {
        if Instant::now() >= deadline {
            return false;
        }
        let observed = on_main(app, deadline, is_focused).await;
        match poll_decision(started.elapsed(), observed) {
            PollDecision::Focused => return true,
            PollDecision::Failed => return false,
            PollDecision::Wait(delay) => sleep(delay).await,
        }
    }
}

async fn on_main<T: Send + 'static>(
    app: &AppHandle,
    deadline: Instant,
    operation: fn(&WebviewWindow) -> Result<T, ()>,
) -> Result<T, ()> {
    let (sender, receiver) = oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if !dispatch_allowed(
            deadline.saturating_duration_since(Instant::now()),
            sender.is_closed(),
        ) {
            return;
        }
        let result = handle
            .get_webview_window("main")
            .ok_or(())
            .and_then(|window| operation(&window));
        let _ = sender.send(result);
    })
    .map_err(|_| ())?;
    timeout_at(deadline, receiver)
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?
}

fn activate(window: &WebviewWindow) -> Result<(), ()> {
    window.show().map_err(|_| ())?;
    window.unminimize().map_err(|_| ())?;
    #[cfg(windows)]
    {
        let root = root_window(window)?;
        // SAFETY: FFI boundary (#8). This main-thread call passes only an OS-owned
        // opaque HWND, validated below; Rust neither dereferences nor owns its memory.
        let _accepted =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(root) };
        // Activation acceptance is not proof of focus; the bounded observer decides.
        Ok(())
    }
    #[cfg(not(windows))]
    window.set_focus().map_err(|_| ())
}

fn is_focused(window: &WebviewWindow) -> Result<bool, ()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, IsIconic, IsWindowVisible,
        };
        let root = root_window(window)?;
        // SAFETY: FFI boundary (#8). Win32 queries receive an opaque validated HWND,
        // retain no Rust pointers, and return scalar state; no memory is dereferenced.
        Ok(unsafe {
            IsWindowVisible(root) != 0 && IsIconic(root) == 0 && GetForegroundWindow() == root
        })
    }
    #[cfg(not(windows))]
    Ok(window.is_visible().map_err(|_| ())?
        && !window.is_minimized().map_err(|_| ())?
        && window.is_focused().map_err(|_| ())?)
}

#[cfg(windows)]
fn root_window(window: &WebviewWindow) -> Result<windows_sys::Win32::Foundation::HWND, ()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetWindowThreadProcessId,
    };
    let handle = window.hwnd().map_err(|_| ())?.0;
    if handle.is_null() {
        return Err(());
    }
    let mut owner = 0_u32;
    // SAFETY: FFI boundary (#8). The HWND comes from Tauri on its main thread and is
    // never dereferenced. owner is initialized, aligned, writable for this call only;
    // Win32 retains no pointer. Invalid/destroyed handles fail the null/owner checks.
    let (root, thread) = unsafe {
        let root = GetAncestor(handle, GA_ROOT);
        let thread = if root.is_null() {
            0
        } else {
            GetWindowThreadProcessId(root, &mut owner)
        };
        (root, thread)
    };
    if root.is_null() || thread == 0 || owner != std::process::id() {
        Err(())
    } else {
        Ok(root)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PollDecision {
    Wait(Duration),
    Focused,
    Failed,
}

const fn dispatch_allowed(remaining: Duration, receiver_closed: bool) -> bool {
    !receiver_closed && !remaining.is_zero()
}

fn poll_decision(elapsed: Duration, focused: Result<bool, ()>) -> PollDecision {
    if elapsed >= CONFIRM_LIMIT {
        return PollDecision::Failed;
    }
    match focused {
        Ok(true) => PollDecision::Focused,
        Ok(false) => PollDecision::Wait(POLL_INTERVAL.min(CONFIRM_LIMIT - elapsed)),
        Err(()) => PollDecision::Failed,
    }
}

#[cfg(test)]
#[path = "focus_tests.rs"]
mod tests;
