use super::*;
use std::sync::Arc;
#[cfg(unix)]
fn stalled_executable(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("stalled-worker");
    fs::write(&path, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_os_call_times_out_while_normal_worker_completes() {
    let dir = tempdir().unwrap();
    let stalled = Broker::new(stalled_executable(dir.path()), Duration::from_millis(120));
    let normal = broker();
    let path = dir.path().to_string_lossy().into_owned();
    let (bad, good) = tokio::join!(
        stalled.start(identity(0, 1), path.clone()),
        normal.start(identity(1, 1), path)
    );
    assert_eq!(bad.unwrap_err().code, FaultCode::Timeout);
    assert!(good.unwrap().batch.complete);
    normal.cancel(&identity(1, 1)).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_interrupts_a_blocked_pipe_and_reaps_the_child() {
    let dir = tempdir().unwrap();
    let broker = Arc::new(Broker::new(
        stalled_executable(dir.path()),
        Duration::from_secs(8),
    ));
    let task_broker = broker.clone();
    let path = dir.path().to_string_lossy().into_owned();
    let pending = tokio::spawn(async move { task_broker.start(identity(0, 1), path).await });
    // Wait until registration (read returns busy while the Open RPC is blocked).
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if broker.read(identity(0, 1), Some(0)).await.unwrap_err().code == FaultCode::Busy {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), broker.cancel(&identity(0, 1)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pending.await.unwrap().unwrap_err().code,
        FaultCode::Cancelled
    );
}
