use super::{geometry, startup::Warning};
use projectpane_core::{
    domain::FaultCode,
    persistence::{
        BoundedLogger, EventKind, LogEvent, StateError, WindowBounds, WindowStateStore,
        resolve_data_root,
    },
};
use std::{
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewWindow};

pub struct DesktopState {
    pub logger: Option<BoundedLogger>,
    store: Option<WindowStateStore>,
    normal: Mutex<Option<WindowBounds>>,
    pub ready: AtomicBool,
    stopped: AtomicBool,
}

impl DesktopState {
    pub fn initialize(
        app: &AppHandle,
        warnings: &mut Vec<Warning>,
    ) -> (Self, Option<WindowBounds>) {
        let override_path = std::env::var_os("PROJECTPANE_DATA_DIR");
        let root = match override_path.as_deref() {
            Some(value) => resolve_data_root(Path::new(""), Some(value)),
            None => app
                .path()
                .app_local_data_dir()
                .map_err(|_| StateError::InvalidPath)
                .and_then(|path| resolve_data_root(&path, None)),
        };
        let mut saved = None;
        let (logger, store) = match root {
            Ok(root) => {
                let logger = BoundedLogger::start(root.clone()).ok();
                if logger.is_none() {
                    warnings.push(Warning::LogUnavailable);
                }
                let mut store = WindowStateStore::new(root);
                match store.load() {
                    Ok(bounds) => {
                        saved = bounds;
                        if bounds.is_some() {
                            record(logger.as_ref(), EventKind::WindowStateLoaded, None);
                        }
                    }
                    Err(StateError::InvalidState) => {
                        warnings.push(Warning::StateInvalid);
                        record(
                            logger.as_ref(),
                            EventKind::WindowStateInvalid,
                            Some(FaultCode::InvalidState),
                        );
                    }
                    Err(_) => {
                        warnings.push(Warning::StateUnavailable);
                        record(
                            logger.as_ref(),
                            EventKind::WindowStateUnavailable,
                            Some(FaultCode::IoError),
                        );
                    }
                }
                (logger, Some(store))
            }
            Err(_) => {
                warnings.extend([Warning::StateUnavailable, Warning::LogUnavailable]);
                (None, None)
            }
        };
        record(logger.as_ref(), EventKind::AppStarted, None);
        (
            Self {
                logger,
                store,
                normal: Mutex::new(None),
                ready: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
            },
            saved,
        )
    }

    pub fn seed(&self, bounds: Option<WindowBounds>) {
        if let Ok(mut normal) = self.normal.lock() {
            *normal = bounds;
        }
        self.ready.store(true, Ordering::Release);
    }

    pub fn capture(&self, window: &WebviewWindow) {
        if !self.ready.load(Ordering::Acquire) || self.stopped.load(Ordering::Acquire) {
            return;
        }
        let Ok(minimized) = window.is_minimized() else {
            return;
        };
        if minimized {
            return;
        }
        let Ok(maximized) = window.is_maximized() else {
            return;
        };
        let bounds = if maximized {
            None
        } else {
            geometry::normal_bounds(window).ok().flatten()
        };
        if let Ok(mut normal) = self.normal.lock() {
            if let Some(bounds) = bounds {
                *normal = Some(bounds);
            }
            if let Some(bounds) = normal.as_mut() {
                bounds.maximized = maximized;
            }
        }
    }

    pub fn save_for_update(&self, window: Option<&WebviewWindow>) -> Result<(), ()> {
        let window = window.ok_or(())?;
        self.capture(window);
        let bounds = self.normal.lock().map_err(|_| ())?.ok_or(())?;
        self.store.as_ref().ok_or(())?.save(bounds).map_err(|_| ())
    }

    pub fn stop(&self, window: Option<&WebviewWindow>) {
        if let Some(window) = window {
            self.capture(window);
        }
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        let bounds = self.normal.lock().ok().and_then(|normal| *normal);
        if let (Some(store), Some(bounds)) = (&self.store, bounds) {
            match store.save(bounds) {
                Ok(()) => record(self.logger.as_ref(), EventKind::WindowStateSaved, None),
                Err(StateError::InvalidState | StateError::Disabled) => record(
                    self.logger.as_ref(),
                    EventKind::WindowStateInvalid,
                    Some(FaultCode::InvalidState),
                ),
                Err(_) => record(
                    self.logger.as_ref(),
                    EventKind::WindowStateUnavailable,
                    Some(FaultCode::IoError),
                ),
            }
        }
        record(self.logger.as_ref(), EventKind::AppStopped, None);
        if let Some(logger) = &self.logger
            && logger.shutdown(Duration::from_secs(2)).is_err()
        {
            eprintln!("log_shutdown_failed");
        }
    }
}

pub fn record(logger: Option<&BoundedLogger>, event: EventKind, code: Option<FaultCode>) {
    if let Some(logger) = logger {
        logger.record(LogEvent {
            event,
            pane: None,
            code,
        });
    }
}
