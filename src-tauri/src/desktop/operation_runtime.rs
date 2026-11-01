use projectpane_core::{
    domain::{Fault, FaultCode},
    services::operations::OperationService,
};
use std::sync::{Arc, atomic::AtomicBool};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::{Semaphore, watch};

pub struct DesktopOperations {
    ready: watch::Receiver<Option<Result<Arc<OperationService>, Fault>>>,
    pub admission: Arc<Semaphore>,
    pub closing: AtomicBool,
    pub exit_allowed: AtomicBool,
    pub shell_menu: Arc<projectpane_core::platform::explorer::context_menu::MenuSession>,
}

impl DesktopOperations {
    pub fn initialize(app: &AppHandle, window: &WebviewWindow) -> Self {
        let (sender, ready) = watch::channel(None);
        let root = app.path().app_local_data_dir();
        let override_path = std::env::var_os("PROJECTPANE_DATA_DIR");
        let owner = window.hwnd().map(|handle| handle.0 as usize);
        tauri::async_runtime::spawn(async move {
            let result = tauri::async_runtime::spawn_blocking(move || {
                let owner = owner.map_err(|_| Fault::new(FaultCode::InvalidState))?;
                let root = root.map_err(|_| Fault::new(FaultCode::InvalidPath))?;
                OperationService::open(&root, override_path.as_deref())
                    .map(|service| (service, owner))
            })
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))
            .and_then(|result| result);
            match result {
                Ok((service, owner)) => {
                    sender.send_replace(Some(Ok(Arc::clone(&service))));
                    if service.run(owner).await.is_err() {
                        eprintln!("operation_driver_stopped_with_error");
                    }
                }
                Err(error) => {
                    sender.send_replace(Some(Err(error)));
                }
            }
        });
        Self {
            ready,
            admission: Arc::new(Semaphore::new(1)),
            closing: AtomicBool::new(false),
            exit_allowed: AtomicBool::new(false),
            shell_menu: Arc::default(),
        }
    }

    pub async fn service(&self) -> Result<Arc<OperationService>, Fault> {
        let mut ready = self.ready.clone();
        loop {
            if let Some(result) = ready.borrow().clone() {
                return result;
            }
            ready
                .changed()
                .await
                .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        }
    }
}

pub fn main_window(window: &WebviewWindow) -> Result<(), Fault> {
    if window.label() != "main" {
        return Err(Fault::new(FaultCode::AccessDenied));
    }
    Ok(())
}
