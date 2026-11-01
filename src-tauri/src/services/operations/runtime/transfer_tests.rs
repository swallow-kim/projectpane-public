use super::*;
use crate::domain::{ActivationKind, Identity};
use std::fs;

fn selection(mode: TransferMode) -> CaptureTransfer {
    CaptureTransfer {
        mode,
        selection: OperationSelection {
            identity: Identity {
                pane: 0,
                tab_id: "transfer".into(),
                request_id: "capture".into(),
                generation: 1,
            },
            entry_ids: vec!["entry-1".into()],
        },
    }
}

fn targets(path: &Path) -> Vec<ActivationTarget> {
    vec![ActivationTarget {
        path: path.to_str().unwrap().into(),
        kind: ActivationKind::File,
    }]
}

fn paste(transfer: &TransferPreview, destination: &Path) -> PrepareTransfer {
    PrepareTransfer {
        token: transfer.token.clone(),
        project_id: None,
        destination: destination.to_str().unwrap().into(),
        copy_name: None,
        skip_existing: false,
    }
}

#[tokio::test]
async fn captured_copy_conflict_can_skip_or_use_a_new_name_without_recapture_or_overwrite() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("same.txt");
    let destination = files.path().join("target");
    fs::create_dir(&destination).unwrap();
    fs::write(&source, b"source bytes").unwrap();
    fs::write(destination.join("same.txt"), b"existing bytes").unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let captured = service
        .capture_transfer(selection(TransferMode::Copy), targets(&source))
        .await
        .unwrap();
    assert_eq!(
        service
            .prepare_transfer(paste(&captured, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::DestinationExists
    );
    let mut named = paste(&captured, &destination);
    named.copy_name = Some("new.txt".into());
    let renamed = service.prepare_transfer(named).await.unwrap();
    let mut skipped = paste(&captured, &destination);
    skipped.skip_existing = true;
    let skipped = service.prepare_transfer(skipped).await.unwrap();
    service.submit(renamed.id.clone()).await.unwrap();
    service.submit(skipped.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert_eq!(
        service
            .report(renamed.id)
            .await
            .unwrap()
            .unwrap()
            .counts
            .changed,
        1
    );
    assert_eq!(
        service
            .report(skipped.id)
            .await
            .unwrap()
            .unwrap()
            .counts
            .skipped,
        1
    );
    assert_eq!(fs::read(&source).unwrap(), b"source bytes");
    assert_eq!(
        fs::read(destination.join("same.txt")).unwrap(),
        b"existing bytes"
    );
    assert_eq!(
        fs::read(destination.join("new.txt")).unwrap(),
        b"source bytes"
    );
}

#[tokio::test]
async fn copy_conflict_options_cannot_change_a_cut_into_a_copy() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("original.txt");
    let destination = files.path().join("target");
    fs::create_dir(&destination).unwrap();
    fs::write(&source, b"keep original").unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let cut = service
        .capture_transfer(selection(TransferMode::Move), targets(&source))
        .await
        .unwrap();
    for skip in [false, true] {
        let mut request = paste(&cut, &destination);
        request.skip_existing = skip;
        request.copy_name = if skip {
            None
        } else {
            Some("renamed.txt".into())
        };
        assert_eq!(
            service.prepare_transfer(request).await.unwrap_err().code,
            FaultCode::InvalidRequest
        );
    }
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"keep original");
    assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
}

#[tokio::test]
async fn captured_copy_survives_listing_release_and_can_be_explicitly_reused() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("한글.txt");
    let a = files.path().join("a");
    let b = files.path().join("b");
    fs::write(&source, b"unchanged captured bytes").unwrap();
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let captured = service
        .capture_transfer(selection(TransferMode::Copy), targets(&source))
        .await
        .unwrap();
    assert_eq!(captured.count, 1);
    let response = serde_json::to_value(&captured).unwrap();
    assert_eq!(response.as_object().unwrap().len(), 3);
    assert!(response.get("sources").is_none());
    let first = service
        .prepare_transfer(paste(&captured, &a))
        .await
        .unwrap();
    let second = service
        .prepare_transfer(paste(&captured, &b))
        .await
        .unwrap();
    assert_eq!(fs::read_dir(&a).unwrap().count(), 0);
    service.submit(first.id.clone()).await.unwrap();
    service.submit(second.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    for id in [first.id, second.id] {
        assert_eq!(
            service.report(id).await.unwrap().unwrap().status(),
            OperationStatus::Succeeded
        );
    }
    for path in [&source, &a.join("한글.txt"), &b.join("한글.txt")] {
        assert_eq!(fs::read(path).unwrap(), b"unchanged captured bytes");
    }
}

#[tokio::test]
async fn cut_has_one_preview_is_consumed_at_submission_and_never_restored() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("cut.txt");
    let destination = files.path().join("dest");
    fs::write(&source, b"cut bytes").unwrap();
    fs::create_dir(&destination).unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let captured = service
        .capture_transfer(selection(TransferMode::Move), targets(&source))
        .await
        .unwrap();
    let first = service
        .prepare_transfer(paste(&captured, &destination))
        .await
        .unwrap();
    assert_eq!(
        service
            .prepare_transfer(paste(&captured, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::Busy
    );
    service.discard(first.id.clone()).await.unwrap();
    assert_eq!(
        service.submit(first.id).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    let retry = service
        .prepare_transfer(paste(&captured, &destination))
        .await
        .unwrap();
    service.submit(retry.id.clone()).await.unwrap();
    assert_eq!(
        service
            .prepare_transfer(paste(&captured, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert_eq!(
        service.submit(retry.id.clone()).await.unwrap().status,
        OperationStatus::Queued
    );
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert_eq!(
        service
            .report(retry.id.clone())
            .await
            .unwrap()
            .unwrap()
            .status(),
        OperationStatus::Succeeded
    );
    drop(service);
    let recovered = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        recovered
            .prepare_transfer(paste(&captured, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert_eq!(
        recovered.submit(retry.id).await.unwrap().status,
        OperationStatus::Succeeded
    );
    recovered.request_close(false).await.unwrap();
    recovered.clone().run(0).await.unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(destination.join("cut.txt")).unwrap(), b"cut bytes");
}

#[tokio::test]
async fn new_capture_revokes_cut_preview_and_substituted_originals_are_refused() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("source.txt");
    let destination = files.path().join("dest");
    fs::write(&source, b"original").unwrap();
    fs::create_dir(&destination).unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let old = service
        .capture_transfer(selection(TransferMode::Move), targets(&source))
        .await
        .unwrap();
    let preview = service
        .prepare_transfer(paste(&old, &destination))
        .await
        .unwrap();
    let current = service
        .capture_transfer(selection(TransferMode::Copy), targets(&source))
        .await
        .unwrap();
    assert_eq!(
        service
            .prepare_transfer(paste(&old, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert_eq!(
        service.submit(preview.id).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    fs::rename(&source, files.path().join("held.txt")).unwrap();
    fs::write(&source, b"replacement").unwrap();
    assert_eq!(
        service
            .prepare_transfer(paste(&current, &destination))
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert!(service.state().await.unwrap().operations.is_empty());
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    assert_eq!(fs::read(source).unwrap(), b"replacement");
    assert_eq!(
        fs::read(files.path().join("held.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn transfer_requests_reject_raw_sources_duplicate_ids_and_oversized_selections() {
    let request = selection(TransferMode::Copy);
    let mut raw = serde_json::to_value(&request).unwrap();
    raw["sources"] = serde_json::json!(["C:\\untrusted"]);
    assert!(serde_json::from_value::<CaptureTransfer>(raw).is_err());
    let mut duplicate = request.clone();
    duplicate.selection.entry_ids.push("entry-1".into());
    assert_eq!(
        duplicate.validate().unwrap_err().code,
        FaultCode::InvalidRequest
    );
    let mut oversized = request;
    oversized.selection.entry_ids = (0..=MAX_OPERATION_SOURCES)
        .map(|n| format!("entry-{n}"))
        .collect();
    assert_eq!(
        oversized.validate().unwrap_err().code,
        FaultCode::ResourceLimit
    );
}
