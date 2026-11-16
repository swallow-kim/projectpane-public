use projectpane_core::{
    domain::{Fault, FaultCode},
    platform::project_roots::{RootClassification, classify_project_roots as classify},
};
use serde::Deserialize;
use tauri::WebviewWindow;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassifyRootsRequest {
    paths: Vec<String>,
}

// One bounded read-only classification at a time. The permit remains held by
// the blocking job even if its caller stops awaiting the result.
static ADMISSION: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[tauri::command]
pub async fn classify_project_roots(
    window: WebviewWindow,
    request: ClassifyRootsRequest,
) -> Result<Vec<RootClassification>, Fault> {
    if window.label() != "main" {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let permit = ADMISSION
        .try_acquire()
        .map_err(|_| Fault::new(FaultCode::Busy))?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        classify(request.paths)
    })
    .await
    .map_err(|_| Fault::new(FaultCode::IoError))?
}
