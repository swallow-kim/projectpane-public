use projectpane_core::domain::Fault;
#[cfg(not(windows))]
use projectpane_core::domain::FaultCode;
#[cfg(windows)]
use projectpane_core::services::watch::{WatchHub, WatchUpdate};
#[cfg(not(windows))]
type WatchUpdate = ();

#[tauri::command]
pub async fn poll_watch(
    #[cfg(windows)] hub: tauri::State<'_, WatchHub>,
    paths: Vec<String>,
) -> Result<WatchUpdate, Fault> {
    #[cfg(windows)]
    {
        hub.poll(paths).await
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err(Fault::new(FaultCode::WorkerUnavailable))
    }
}
