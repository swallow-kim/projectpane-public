use super::*;
use crate::domain::{ActivationKind, Identity};
use std::fs;

struct Fixture {
    files: tempfile::TempDir,
    data: tempfile::TempDir,
}

impl Fixture {
    fn new(collisions: usize) -> Self {
        let files = tempfile::tempdir().unwrap();
        fs::create_dir(files.path().join("target")).unwrap();
        for index in 0..2 {
            fs::write(files.path().join(format!("{index}.txt")), b"source").unwrap();
            if index < collisions {
                fs::write(
                    files.path().join(format!("target/{index}.txt")),
                    b"sentinel",
                )
                .unwrap();
            }
        }
        Self {
            files,
            data: tempfile::tempdir().unwrap(),
        }
    }

    async fn prepare(&self, service: &Arc<OperationService>) -> OperationPreview {
        service
            .prepare(
                PrepareOperation {
                    project_id: None,
                    action: OperationAction::CopySkipExisting {
                        selection: OperationSelection {
                            identity: Identity {
                                pane: 0,
                                tab_id: "tab".into(),
                                request_id: "request".into(),
                                generation: 1,
                            },
                            entry_ids: vec!["opaque-0".into(), "opaque-1".into()],
                        },
                        destination: self.files.path().join("target").to_str().unwrap().into(),
                    },
                },
                (0..2)
                    .map(|index| ActivationTarget {
                        path: self
                            .files
                            .path()
                            .join(format!("{index}.txt"))
                            .to_str()
                            .unwrap()
                            .into(),
                        kind: ActivationKind::File,
                    })
                    .collect(),
            )
            .await
            .unwrap()
    }

    fn service(&self) -> Arc<OperationService> {
        OperationService::open(self.data.path(), None).unwrap()
    }

    fn result_path(&self, id: &str) -> std::path::PathBuf {
        self.data
            .path()
            .join("foundation/operations-v1")
            .join(format!("{id}.result.json"))
    }
}

#[tokio::test]
async fn skip_existing_runtime_preview_same_id_results_and_restart_preserve_decisions() {
    for collisions in 0..=2 {
        let fixture = Fixture::new(collisions);
        let service = fixture.service();
        let preview = fixture.prepare(&service).await;
        let wire = serde_json::to_value(&preview).unwrap();
        assert_eq!(wire["kind"], "copy_skip_existing");
        assert_eq!(preview.sources.len(), 2);
        assert_eq!(preview.skipped_sources.as_ref().unwrap().len(), collisions);
        assert_eq!(
            preview.destination.as_deref(),
            fixture.files.path().join("target").to_str()
        );
        for skipped in preview.skipped_sources.as_ref().unwrap() {
            assert!(preview.sources.contains(skipped));
        }
        service.submit(preview.id.clone()).await.unwrap();
        service.submit(preview.id.clone()).await.unwrap();
        service.request_close(false).await.unwrap();
        service.clone().run(0).await.unwrap();
        let report = service.report(preview.id.clone()).await.unwrap().unwrap();
        assert_eq!(report.user_skipped.len(), collisions);
        assert_eq!(report.items.len(), 2 - collisions);
        assert_eq!(
            service.submit(preview.id.clone()).await.unwrap().status,
            report.status()
        );
        assert_eq!(
            fs::read_dir(fixture.files.path().join("target"))
                .unwrap()
                .count(),
            2
        );
        drop(service);
        let restarted = fixture.service();
        let recovered = restarted.report(preview.id.clone()).await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&recovered).unwrap(),
            serde_json::to_value(report).unwrap()
        );
        assert_eq!(
            restarted.submit(preview.id).await.unwrap().status,
            recovered.status()
        );
        restarted.request_close(false).await.unwrap();
        restarted.clone().run(0).await.unwrap();
    }
}

#[tokio::test]
async fn skip_existing_runtime_queued_restart_has_no_replay_or_applied_decisions() {
    let fixture = Fixture::new(1);
    let service = fixture.service();
    let preview = fixture.prepare(&service).await;
    service.submit(preview.id.clone()).await.unwrap();
    drop(service);
    let restarted = fixture.service();
    assert_eq!(
        restarted.submit(preview.id.clone()).await.unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    assert!(restarted.report(preview.id).await.unwrap().is_none());
    restarted.request_close(false).await.unwrap();
    restarted.clone().run(0).await.unwrap();
    assert!(!fixture.files.path().join("target/1.txt").exists());
    assert_eq!(
        fs::read(fixture.files.path().join("target/0.txt")).unwrap(),
        b"sentinel"
    );
}

#[tokio::test]
async fn skip_existing_runtime_stale_preflight_failure_never_records_decisions() {
    let fixture = Fixture::new(1);
    let service = fixture.service();
    let preview = fixture.prepare(&service).await;
    fs::rename(
        fixture.files.path().join("target/0.txt"),
        fixture.files.path().join("held"),
    )
    .unwrap();
    service.submit(preview.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    let report = service.report(preview.id).await.unwrap().unwrap();
    assert_eq!(report.failure.unwrap().code, FaultCode::StaleRequest);
    assert!(report.user_skipped.is_empty());
    assert!(report.items.is_empty());
    assert_eq!(report.user_decisions_only, None);
    assert_eq!(report.counts, OutcomeCounts::default());
}

#[tokio::test]
async fn skip_existing_runtime_restart_rejects_decisions_not_bound_to_original_intent() {
    let fixture = Fixture::new(2);
    let service = fixture.service();
    let preview = fixture.prepare(&service).await;
    service.submit(preview.id.clone()).await.unwrap();
    service.request_close(false).await.unwrap();
    service.clone().run(0).await.unwrap();
    drop(service);
    let result = fixture.result_path(&preview.id);
    let mut wire: serde_json::Value = serde_json::from_slice(&fs::read(&result).unwrap()).unwrap();
    wire["report"]["userSkipped"][0]["source"] = serde_json::json!("C:\\unselected.txt");
    fs::write(&result, serde_json::to_vec(&wire).unwrap()).unwrap();
    let restarted = fixture.service();
    assert!(
        restarted
            .report(preview.id.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        restarted.submit(preview.id).await.unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    restarted.request_close(false).await.unwrap();
    restarted.clone().run(0).await.unwrap();
}
