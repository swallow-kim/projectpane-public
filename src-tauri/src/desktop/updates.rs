//! Updater transport is backend-only. No client token, arbitrary URL or install path IPC.
use super::{operation_close, update_installation, update_policy};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current_version: String,
    installed: bool,
    phase: &'static str,
    version: Option<String>,
    notes: Option<String>,
    error: Option<&'static str>,
    received: u64,
    total: Option<u64>,
    last_check: Option<u64>,
    previous_install: Option<&'static str>,
}
struct Staged {
    update: Update,
    bytes: Vec<u8>,
}
pub struct DesktopUpdates {
    state: Mutex<UpdateStatus>,
    candidate: Mutex<Option<Update>>,
    staged: Mutex<Option<Staged>>,
    admission: AsyncMutex<()>,
    pending: AtomicBool,
    cancel_download: tokio::sync::watch::Sender<bool>,
    root: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    from: String,
    to: String,
}
impl DesktopUpdates {
    pub fn initialize(app: &AppHandle) -> Self {
        let root = std::env::var_os("PROJECTPANE_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| app.path().app_local_data_dir().ok());
        let last_check = root
            .as_ref()
            .and_then(|root| std::fs::read(root.join("update-check.json")).ok())
            .and_then(|data| serde_json::from_slice::<u64>(&data).ok());
        let previous_install = root.as_ref().and_then(|root| {
            let path = root.join("update-install.json");
            match std::fs::read(&path) {
                Ok(data) => Some(match serde_json::from_slice::<Receipt>(&data) {
                    Ok(receipt) if receipt.to == env!("CARGO_PKG_VERSION") => "completed",
                    _ => "unconfirmed",
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some("unconfirmed"),
            }
        });
        Self {
            state: Mutex::new(UpdateStatus {
                current_version: env!("CARGO_PKG_VERSION").into(),
                installed: update_installation::installed(),
                phase: "idle",
                version: None,
                notes: None,
                error: None,
                received: 0,
                total: None,
                last_check,
                previous_install,
            }),
            candidate: Mutex::new(None),
            staged: Mutex::new(None),
            admission: AsyncMutex::new(()),
            pending: AtomicBool::new(false),
            cancel_download: tokio::sync::watch::channel(false).0,
            root,
        }
    }
    fn status(&self) -> Result<UpdateStatus, &'static str> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| "unavailable")
    }
    fn phase(&self, phase: &'static str, error: Option<&'static str>) {
        if let Ok(mut state) = self.state.lock() {
            state.phase = phase;
            state.error = error;
        }
    }
    fn fail(&self, code: &'static str) -> &'static str {
        self.phase("error", Some(code));
        code
    }
}
fn main(window: &WebviewWindow) -> Result<(), &'static str> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("unavailable")
    }
}
pub fn pending(app: &AppHandle) -> bool {
    app.try_state::<DesktopUpdates>()
        .is_some_and(|state| state.pending.load(Ordering::Acquire))
}
pub fn cancel(app: &AppHandle) {
    if let Some(state) = app.try_state::<DesktopUpdates>()
        && state.pending.swap(false, Ordering::AcqRel)
    {
        state.phase("ready", None);
    }
}
pub fn fail_pending(app: &AppHandle, error: &'static str) {
    if let Some(state) = app.try_state::<DesktopUpdates>()
        && state.pending.swap(false, Ordering::AcqRel)
    {
        state.fail(error);
    }
}
#[tauri::command]
pub fn update_status(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopUpdates>,
) -> Result<UpdateStatus, &'static str> {
    main(&window)?;
    state.status()
}
#[tauri::command]
pub async fn check_update(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopUpdates>,
    manual: bool,
) -> Result<UpdateStatus, &'static str> {
    main(&window)?;
    let _permit = state.admission.try_lock().map_err(|_| "busy")?;
    if state.pending.load(Ordering::Acquire) {
        return Err("busy");
    }
    if state.staged.lock().map_err(|_| "unavailable")?.is_some() {
        return state.status();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock")?
        .as_secs();
    if !manual && !update_policy::automatic_due(state.status()?.last_check, now) {
        return state.status();
    }
    // Cache an attempted automatic request, including an offline failure. Manual checks always bypass it.
    if let Some(root) = &state.root {
        let _ = std::fs::create_dir_all(root);
        let _ = std::fs::write(root.join("update-check.json"), now.to_string());
    }
    {
        let mut status = state.state.lock().map_err(|_| "unavailable")?;
        status.last_check = Some(now);
        status.phase = "checking";
        status.error = None;
        status.version = None;
        status.notes = None;
    }
    *state.candidate.lock().map_err(|_| "unavailable")? = None;
    let updater = window
        .app_handle()
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| state.fail("configuration"))?;
    let candidate = updater
        .check()
        .await
        .map_err(|_| state.fail("check_failed"))?;
    if let Some(mut update) = candidate {
        if !update_policy::accepted_release(
            env!("CARGO_PKG_VERSION"),
            &update.version,
            update.download_url.as_str(),
        ) {
            return Err(state.fail("invalid_release"));
        }
        update.timeout = Some(Duration::from_secs(180));
        let mut status = state.state.lock().map_err(|_| "unavailable")?;
        status.version = Some(update.version.clone());
        status.notes = Some(
            update
                .body
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(12000)
                .collect(),
        );
        status.phase = "available";
        *state.candidate.lock().map_err(|_| "unavailable")? = Some(update);
    } else {
        state.phase("current", None);
    }
    state.status()
}
#[tauri::command]
pub async fn download_update(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopUpdates>,
) -> Result<UpdateStatus, &'static str> {
    main(&window)?;
    let _permit = state.admission.try_lock().map_err(|_| "busy")?;
    if state.pending.load(Ordering::Acquire) {
        return Err("busy");
    }
    if !update_installation::installed() {
        return Err("portable");
    }
    let update = state
        .candidate
        .lock()
        .map_err(|_| "unavailable")?
        .clone()
        .ok_or("check_required")?;
    if !update_policy::accepted_release(
        env!("CARGO_PKG_VERSION"),
        &update.version,
        update.download_url.as_str(),
    ) {
        return Err("invalid_release");
    }
    state.cancel_download.send_replace(false);
    let mut cancelled = state.cancel_download.subscribe();
    state.phase("downloading", None);
    let received = AtomicU64::new(0);
    let exceeded = tokio::sync::Notify::new();
    let download = update.download(
        |chunk, total| {
            let count = received
                .fetch_add(chunk as u64, Ordering::Relaxed)
                .saturating_add(chunk as u64);
            if count > update_policy::MAX_DOWNLOAD
                || total.is_some_and(|total| total > update_policy::MAX_DOWNLOAD)
            {
                exceeded.notify_one();
            }
            if let Ok(mut status) = state.state.lock() {
                status.received = count;
                status.total = total;
            }
        },
        || {},
    );
    let bytes = tokio::select! {
        biased;
        _ = cancelled.changed() => { state.phase("available", None); return state.status(); },
        _ = exceeded.notified() => return Err(state.fail("download_limit")),
        result = download => result.map_err(|_| state.fail("download_or_signature"))?,
    };
    if bytes.len() as u64 > update_policy::MAX_DOWNLOAD {
        return Err(state.fail("download_limit"));
    }
    *state.staged.lock().map_err(|_| "unavailable")? = Some(Staged { update, bytes });
    state.phase("ready", None);
    state.status()
}
#[tauri::command]
pub fn cancel_update_download(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopUpdates>,
) -> Result<(), &'static str> {
    main(&window)?;
    if state.status()?.phase != "downloading" {
        return Err("not_downloading");
    }
    state.cancel_download.send_replace(true);
    Ok(())
}
#[tauri::command]
pub async fn install_update(
    window: WebviewWindow,
    state: tauri::State<'_, DesktopUpdates>,
) -> Result<UpdateStatus, &'static str> {
    main(&window)?;
    let _permit = state.admission.try_lock().map_err(|_| "busy")?;
    if !update_installation::installed() {
        return Err("portable");
    }
    if state.staged.lock().map_err(|_| "unavailable")?.is_none() {
        return Err("download_required");
    }
    let operations = window.state::<super::operation_runtime::DesktopOperations>();
    if operations.closing.load(Ordering::Acquire) || state.pending.swap(true, Ordering::AcqRel) {
        return Err("busy");
    }
    state.phase("preparing", None);
    operation_close::request_close(window.app_handle());
    state.status()
}
#[tauri::command]
pub fn open_update_page(window: WebviewWindow) -> Result<(), &'static str> {
    main(&window)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let url: Vec<u16> = update_policy::RELEASES
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err("open_failed");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("unsupported")
    }
}
pub fn after_save(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = super::update_handoff::run(LiveHandoff { app: app.clone() }).await {
            fail_pending(&app, error);
        }
    });
}
struct LiveHandoff {
    app: AppHandle,
}
impl super::update_handoff::Handoff for LiveHandoff {
    async fn perform(&mut self, step: super::update_handoff::Step) -> Result<(), &'static str> {
        use super::update_handoff::Step;
        let app = &self.app;
        match step {
            Step::Verify => {
                if !pending(app)
                    || !operation_close::exit_allowed(app)
                    || !super::projects::exit_allowed(app)
                {
                    return Err("save_failed");
                }
                if !update_installation::installed() {
                    return Err("portable");
                }
                let state = app.state::<DesktopUpdates>();
                let service = app
                    .state::<super::operation_runtime::DesktopOperations>()
                    .service()
                    .await
                    .map_err(|_| "operations_unconfirmed")?;
                let status = service
                    .state()
                    .await
                    .map_err(|_| "operations_unconfirmed")?;
                if status.close_state
                    != projectpane_core::services::operations::OperationCloseState::Stopped
                {
                    return Err("operations_unconfirmed");
                }
                let _ = state;
            }
            Step::SaveWindow => {
                app.state::<super::runtime::DesktopState>()
                    .save_for_update(app.get_webview_window("main").as_ref())
                    .map_err(|_| "save_failed")?;
            }
            Step::StopWorkers => {
                tokio::time::timeout(Duration::from_secs(12), async {
                    app.state::<projectpane_core::services::Broker>()
                        .shutdown()
                        .await
                        .map_err(|_| "workers_unconfirmed")?;
                    #[cfg(windows)]
                    app.state::<projectpane_core::services::watch::WatchHub>()
                        .shutdown()
                        .await
                        .map_err(|_| "workers_unconfirmed")?;
                    Ok::<(), &'static str>(())
                })
                .await
                .map_err(|_| "workers_unconfirmed")??;
                if !update_installation::workers_closed() {
                    return Err("workers_unconfirmed");
                }
            }
            Step::WriteReceipt => {
                let state = app.state::<DesktopUpdates>();
                let root = state.root.as_ref().ok_or("save_failed")?;
                std::fs::create_dir_all(root).map_err(|_| "save_failed")?;
                let version = state.status()?.version.ok_or("download_required")?;
                let receipt = serde_json::to_vec(&Receipt {
                    from: env!("CARGO_PKG_VERSION").into(),
                    to: version,
                })
                .map_err(|_| "save_failed")?;
                use std::io::Write;
                let mut file = std::fs::File::create(root.join("update-install.json"))
                    .map_err(|_| "save_failed")?;
                file.write_all(&receipt)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| "save_failed")?;
            }
            Step::Install => {
                let state = app.state::<DesktopUpdates>();
                let staged = state
                    .staged
                    .lock()
                    .map_err(|_| "unavailable")?
                    .take()
                    .ok_or("download_required")?;
                state.phase("installing", None);
                // No asynchronous work in the plugin's non-vetoable on_before_exit hook.
                tauri::async_runtime::spawn_blocking(move || staged.update.install(staged.bytes))
                    .await
                    .map_err(|_| "install_failed")?
                    .map_err(|_| "install_failed")?;
            }
        }
        Ok(())
    }
}
