#![cfg(all(windows, not(miri)))]
use projectpane_core::{domain::FaultCode, services::watch::WatchHub};
use std::{path::PathBuf, time::Duration};

fn hub() -> WatchHub {
    WatchHub::new(PathBuf::from(env!("CARGO_BIN_EXE_projectpane-read-worker")))
}
async fn tick() {
    tokio::time::sleep(Duration::from_millis(410)).await;
}

#[tokio::test]
async fn shared_live_paths_reconfigure_clear_and_close_without_queueing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_str().unwrap().to_owned();
    let hub = hub();
    assert_eq!(
        hub.poll(vec![path.clone(), path.clone()])
            .await
            .unwrap()
            .changed,
        3
    );
    assert_eq!(
        hub.poll(vec![path.clone()]).await.unwrap_err().code,
        FaultCode::Busy
    );
    std::fs::write(root.path().join("changed.txt"), b"new").unwrap();
    tick().await;
    assert_eq!(
        hub.poll(vec![path.clone(), path.clone()])
            .await
            .unwrap()
            .changed,
        3
    );
    tick().await;
    assert_eq!(hub.poll(vec![]).await.unwrap().changed, 0);
    tick().await;
    assert_eq!(hub.poll(vec![path.clone()]).await.unwrap().changed, 1);
    hub.close();
    assert_eq!(
        hub.poll(vec![path]).await.unwrap_err().code,
        FaultCode::WorkerUnavailable
    );
}

#[tokio::test]
async fn worker_failure_is_terminal_without_automatic_restart() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_str().unwrap().to_owned();
    let executable = root.path().join("recovered-worker.exe");
    let hub = WatchHub::new(executable.clone());
    assert!(hub.poll(vec![path.clone()]).await.is_err());
    std::fs::copy(env!("CARGO_BIN_EXE_projectpane-read-worker"), &executable).unwrap();
    tick().await;
    assert_eq!(
        hub.poll(vec![path.clone()]).await.unwrap_err().code,
        FaultCode::WorkerUnavailable
    );
    let recovered = WatchHub::new(executable);
    assert_eq!(recovered.poll(vec![path]).await.unwrap().changed, 1);
    recovered.close();
}

#[tokio::test]
async fn missing_subscription_is_isolated_and_not_reopened_when_other_paths_change() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_str().unwrap().to_owned();
    let missing_path = root.path().join("missing");
    let missing = missing_path.to_str().unwrap().to_owned();
    let hub = hub();
    let first = hub
        .poll(vec![path.clone(), missing.clone(), missing.clone()])
        .await
        .unwrap();
    assert_eq!((first.changed, first.unavailable), (1, 6));
    std::fs::create_dir(&missing_path).unwrap();
    tick().await;
    let next = hub.poll(vec![missing.clone(), path.clone()]).await.unwrap();
    assert_eq!((next.changed, next.unavailable), (2, 1));
    std::fs::write(root.path().join("healthy.txt"), b"still live").unwrap();
    tick().await;
    let next = hub.poll(vec![missing.clone(), path.clone()]).await.unwrap();
    assert_eq!((next.changed, next.unavailable), (2, 1));
    tick().await;
    hub.poll(vec![path]).await.unwrap();
    tick().await;
    let retried = hub.poll(vec![missing]).await.unwrap();
    assert_eq!((retried.changed, retried.unavailable), (1, 0));
    hub.close();
}
