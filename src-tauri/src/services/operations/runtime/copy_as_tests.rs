use super::*;
use crate::domain::{ActivationKind, Identity};
use std::fs;

fn request(root: &Path) -> PrepareOperation {
    PrepareOperation {
        project_id: Some("project-1".into()),
        action: OperationAction::CopyAs {
            selection: OperationSelection {
                identity: Identity {
                    pane: 0,
                    tab_id: "tab-1".into(),
                    request_id: "request-1".into(),
                    generation: 1,
                },
                entry_ids: vec!["opaque-1".into()],
            },
            destination: root.join("target").to_str().unwrap().into(),
            name: "사본 😀.txt".into(),
        },
    }
}

fn targets(root: &Path) -> Vec<ActivationTarget> {
    vec![ActivationTarget {
        path: root.join("source.txt").to_str().unwrap().into(),
        kind: ActivationKind::File,
    }]
}

fn fixture() -> tempfile::TempDir {
    let files = tempfile::tempdir().unwrap();
    fs::create_dir(files.path().join("target")).unwrap();
    fs::write(files.path().join("source.txt"), b"immutable source").unwrap();
    files
}

#[tokio::test]
async fn copy_as_restart_rejects_changed_destination_not_in_approved_intent() {
    let data = tempfile::tempdir().unwrap();
    let files = fixture();
    let service = OperationService::open(data.path(), None).unwrap();
    let preview = service
        .prepare(request(files.path()), targets(files.path()))
        .await
        .unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    assert_eq!(
        service
            .report(preview.id.clone())
            .await
            .unwrap()
            .unwrap()
            .status(),
        OperationStatus::Succeeded
    );
    drop(service);
    let result = data
        .path()
        .join("foundation/operations-v1")
        .join(format!("{}.result.json", preview.id));
    let mut wire: serde_json::Value = serde_json::from_slice(&fs::read(&result).unwrap()).unwrap();
    wire["report"]["items"][0]["destination"] =
        serde_json::json!(files.path().join("unapproved.txt").to_str().unwrap());
    let damaged = serde_json::to_vec(&wire).unwrap();
    fs::write(&result, &damaged).unwrap();
    let restarted = OperationService::open(data.path(), None).unwrap();
    assert!(
        restarted
            .report(preview.id.clone())
            .await
            .unwrap()
            .is_none(),
        "mismatched changed destination was trusted"
    );
    assert_eq!(
        restarted.submit(preview.id).await.unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    restarted.request_close(false).await.unwrap();
    restarted.clone().run(0).await.unwrap();
    assert_eq!(fs::read(&result).unwrap(), damaged);
    assert_eq!(
        fs::read(files.path().join("source.txt")).unwrap(),
        b"immutable source"
    );
    assert_eq!(
        fs::read(files.path().join("target/사본 😀.txt")).unwrap(),
        b"immutable source"
    );
    assert!(!files.path().join("unapproved.txt").exists());
}

#[tokio::test]
async fn copy_as_runtime_preview_exact_target_duplicate_submit_and_persisted_native_result() {
    let data = tempfile::tempdir().unwrap();
    let files = fixture();
    let service = OperationService::open(data.path(), None).unwrap();
    let preview = service
        .prepare(request(files.path()), targets(files.path()))
        .await
        .unwrap();
    let target = files.path().join("target/사본 😀.txt");
    assert_eq!(
        serde_json::to_value(&preview).unwrap(),
        serde_json::json!({
            "id": preview.id, "projectId": "project-1", "kind": "copy_as",
            "sources": [files.path().join("source.txt").to_str().unwrap()],
            "destination": target.to_str().unwrap().replace('/', "\\"),
        })
    );
    assert!(!target.exists());
    service.submit(preview.id.clone()).await.unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    let report = service.report(preview.id.clone()).await.unwrap().unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.items.len(), 1);
    assert_eq!(fs::read(&target).unwrap(), b"immutable source");
    assert_eq!(
        fs::read(files.path().join("source.txt")).unwrap(),
        b"immutable source"
    );
    assert_eq!(
        fs::read_dir(files.path().join("target")).unwrap().count(),
        1
    );
    drop(service);
    let restarted = OperationService::open(data.path(), None).unwrap();
    let recovered = restarted.report(preview.id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(recovered).unwrap(),
        serde_json::to_value(report).unwrap()
    );
    restarted.request_close(false).await.unwrap();
    restarted.clone().run(0).await.unwrap();
}

#[tokio::test]
async fn copy_as_runtime_restart_never_replays_queued_immutable_intent() {
    let data = tempfile::tempdir().unwrap();
    let files = fixture();
    let service = OperationService::open(data.path(), None).unwrap();
    let preview = service
        .prepare(request(files.path()), targets(files.path()))
        .await
        .unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    let intent_path = data
        .path()
        .join("foundation/operations-v1")
        .join(format!("{}.intent.json", preview.id));
    let intent: OperationIntent = serde_json::from_slice(&fs::read(intent_path).unwrap()).unwrap();
    intent.validate().unwrap();
    let PreparedAction::CopyAs {
        source,
        destination,
        name,
    } = intent.plan.action
    else {
        panic!("expected named-copy intent");
    };
    assert_eq!(
        source,
        crate::platform::file_operations::capture_source(
            files.path().join("source.txt").to_str().unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        destination,
        crate::platform::file_operations::capture_source(
            files.path().join("target").to_str().unwrap()
        )
        .unwrap()
    );
    assert_eq!(name, "사본 😀.txt");
    drop(service);
    let restarted = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        restarted.submit(preview.id).await.unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    restarted.request_close(false).await.unwrap();
    restarted.clone().run(0).await.unwrap();
    assert_eq!(
        fs::read_dir(files.path().join("target")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read(files.path().join("source.txt")).unwrap(),
        b"immutable source"
    );
}
