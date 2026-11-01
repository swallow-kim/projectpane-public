use super::*;
use crate::domain::{ActivationKind, Identity};
use std::fs;

async fn check_completion_write_failure(pending: bool) {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let first = service
        .prepare(folder(files.path(), "completed"), vec![])
        .await
        .unwrap();
    service.submit(first.id.clone()).await.unwrap();
    if pending {
        let second = service
            .prepare(folder(files.path(), "must-not-run"), vec![])
            .await
            .unwrap();
        service.submit(second.id).await.unwrap();
    }
    let journal = data.path().join("foundation/operations-v1");
    let obstruction = journal.join(format!("{}.result.json", first.id));
    fs::create_dir(&obstruction).unwrap();
    service.request_close(false).await.unwrap();
    assert_eq!(
        service
            .acknowledge_unsaved_results()
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidState
    );

    let mut driver = tokio::spawn(service.clone().run(0));
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), &mut driver).await;
    let state = service.state().await.unwrap();
    let report = service.report(first.id.clone()).await.unwrap();
    if result.is_err() {
        driver.abort();
        assert!(driver.await.unwrap_err().is_cancelled());
        service.lane.shutdown().await.unwrap();
    }
    assert_eq!(state.close_state, OperationCloseState::ResultsUnsaved);
    assert_eq!(service.keep_open().await.unwrap_err().code, FaultCode::Busy);
    service.acknowledge_unsaved_results().await.unwrap();
    let acknowledged = service.state().await.unwrap();
    assert_eq!(acknowledged.close_state, OperationCloseState::Stopped);
    assert!(!acknowledged.available);
    assert_eq!(
        serde_json::to_value(&acknowledged.operations).unwrap(),
        serde_json::to_value(&state.operations).unwrap()
    );
    drop(service);
    fs::remove_dir(obstruction).unwrap();

    let recovered = OperationService::open(data.path(), None).unwrap();
    let records = recovered.state().await.unwrap().operations;
    recovered.request_close(false).await.unwrap();
    recovered.clone().run(0).await.unwrap();
    assert_eq!(records.len(), if pending { 2 } else { 1 });
    assert!(
        records
            .iter()
            .all(|record| record.status == OperationStatus::InterruptedUnknown)
    );
    assert_eq!(fs::read_dir(files.path()).unwrap().count(), 1);
    assert!(files.path().join("completed").is_dir());
    assert!(!files.path().join("must-not-run").exists());
    assert_eq!(report.unwrap().status(), OperationStatus::Succeeded);
    assert!(!state.available);
    assert!(state.progress.is_none());
    assert!(
        state
            .operations
            .iter()
            .any(|record| record.id == first.id && !record.journal_saved)
    );
    assert_eq!(
        result.unwrap().unwrap().unwrap_err().code,
        FaultCode::IoError
    );
}

#[tokio::test]
async fn completion_write_failure_requires_acknowledgement_with_pending_work() {
    check_completion_write_failure(true).await;
}

#[tokio::test]
async fn completion_write_failure_requires_acknowledgement_without_pending_work() {
    check_completion_write_failure(false).await;
}

#[tokio::test]
async fn failed_native_shutdown_keeps_unsaved_exit_unavailable() {
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    service.lane.shutdown().await.unwrap();
    service
        .access(|state| {
            state.queue.faulted = true;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        service.clone().run(0).await.unwrap_err().code,
        FaultCode::WorkerUnavailable
    );
    assert_eq!(
        service.state().await.unwrap().close_state,
        OperationCloseState::Blocked
    );
    assert_eq!(
        service
            .acknowledge_unsaved_results()
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidState
    );
}

#[tokio::test]
async fn unsaved_acknowledgement_cannot_bypass_unconfirmed_native_shutdown() {
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    for close in [
        OperationCloseState::Open,
        OperationCloseState::Deciding,
        OperationCloseState::Waiting,
        OperationCloseState::Stopping,
        OperationCloseState::Blocked,
        OperationCloseState::Stopped,
    ] {
        service
            .access(move |state| {
                state.close = close;
                state.queue.faulted = true;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .acknowledge_unsaved_results()
                .await
                .unwrap_err()
                .code,
            FaultCode::InvalidState
        );
        assert_eq!(service.state().await.unwrap().close_state, close);
    }
    service.lane.shutdown().await.unwrap();
}

fn folder(root: &Path, name: &str) -> PrepareOperation {
    PrepareOperation {
        project_id: None,
        action: OperationAction::NewFolder {
            parent: root.to_str().unwrap().into(),
            name: name.into(),
        },
    }
}

#[tokio::test]
async fn close_decision_waits_for_choice_even_after_queued_work_finishes() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let preview = service
        .prepare(folder(files.path(), "decision"), vec![])
        .await
        .unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    assert_eq!(
        service.ask_close().await.unwrap(),
        OperationCloseState::Deciding
    );
    let driver = tokio::spawn(service.clone().run(0));
    let completed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if service.report(preview.id.clone()).await.unwrap().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    let before_choice = service.state().await.unwrap();
    service.request_close(false).await.unwrap();
    driver.await.unwrap().unwrap();
    assert!(completed.is_ok());
    assert_eq!(before_choice.close_state, OperationCloseState::Deciding);
    assert!(files.path().join("decision").is_dir());
}

#[tokio::test]
async fn actual_runtime_persists_before_dispatch_cancels_queue_and_closes_after_native_completion()
{
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let first = service
        .prepare(folder(files.path(), "완료"), vec![])
        .await
        .unwrap();
    let cancelled = service
        .prepare(folder(files.path(), "취소"), vec![])
        .await
        .unwrap();
    service.submit(first.id.clone()).await.unwrap();
    service.submit(first.id.clone()).await.unwrap();
    service.submit(cancelled.id.clone()).await.unwrap();
    assert_eq!(fs::read_dir(files.path()).unwrap().count(), 0);
    assert!(
        data.path()
            .join("foundation/operations-v1")
            .join(format!("{}.intent.json", first.id))
            .exists()
    );
    service.cancel(cancelled.id.clone()).await.unwrap();
    assert_eq!(
        service.request_close(false).await.unwrap(),
        OperationCloseState::Waiting
    );
    assert_eq!(
        service
            .prepare(folder(files.path(), "닫는중"), vec![])
            .await
            .unwrap_err()
            .code,
        FaultCode::Busy
    );
    service.clone().run(0).await.unwrap();
    let state = service.state().await.unwrap();
    assert_eq!(state.close_state, OperationCloseState::Stopped);
    assert!(state.progress.is_none());
    assert!(files.path().join("완료").is_dir());
    assert!(!files.path().join("취소").exists());
    assert_eq!(
        service
            .report(first.id.clone())
            .await
            .unwrap()
            .unwrap()
            .status(),
        OperationStatus::Succeeded
    );
    assert_eq!(
        service
            .report(cancelled.id)
            .await
            .unwrap()
            .unwrap()
            .status(),
        OperationStatus::Cancelled
    );
    drop(service);
    let recovered = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        recovered.submit(first.id).await.unwrap().status,
        OperationStatus::Succeeded
    );
    recovered.request_close(false).await.unwrap();
    recovered.clone().run(0).await.unwrap();
    assert_eq!(fs::read_dir(files.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn dropping_the_submit_caller_does_not_own_the_native_execution() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let preview = service
        .prepare(folder(files.path(), "retained"), vec![])
        .await
        .unwrap();
    let client_service = service.clone();
    let (submitted, ready) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn(async move {
        client_service.submit(preview.id).await.unwrap();
        submitted.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    ready.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    service.request_close(false).await.unwrap();
    service.keep_open().await.unwrap();
    assert!(service.state().await.unwrap().available);
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert!(files.path().join("retained").is_dir());
}

#[tokio::test]
async fn preparation_rechecks_resolved_kind_and_execution_rejects_replaced_source() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("original.txt");
    fs::write(&path, b"original").unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let request = PrepareOperation {
        project_id: None,
        action: OperationAction::Rename {
            selection: OperationSelection {
                identity: Identity {
                    pane: 0,
                    tab_id: "fixture".into(),
                    request_id: "request".into(),
                    generation: 1,
                },
                entry_ids: vec!["fixture-entry".into()],
            },
            name: "renamed.txt".into(),
        },
    };
    assert!(service.prepare(request.clone(), vec![]).await.is_err());
    let mut target = ActivationTarget {
        path: path.to_str().unwrap().into(),
        kind: ActivationKind::Folder,
    };
    assert_eq!(
        service
            .prepare(request.clone(), vec![target.clone()])
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    target.kind = ActivationKind::File;
    let preview = service.prepare(request, vec![target]).await.unwrap();
    fs::rename(&path, files.path().join("held.txt")).unwrap();
    fs::write(&path, b"replacement").unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    let report = service.report(preview.id.clone()).await.unwrap().unwrap();
    assert_eq!(report.status(), OperationStatus::Failed);
    assert_eq!(
        serde_json::to_value(&report).unwrap()["failure"]["code"],
        "stale_request"
    );
    drop(service);
    let recovered = OperationService::open(data.path(), None).unwrap();
    let report = recovered.report(preview.id.clone()).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&report).unwrap()["failure"]["code"],
        "stale_request"
    );
    assert_eq!(
        recovered.submit(preview.id).await.unwrap().status,
        OperationStatus::Failed
    );
    recovered.request_close(false).await.unwrap();
    recovered.clone().run(0).await.unwrap();
    assert_eq!(fs::read(path).unwrap(), b"replacement");
    assert_eq!(
        fs::read(files.path().join("held.txt")).unwrap(),
        b"original"
    );
    assert!(!files.path().join("renamed.txt").exists());
}
