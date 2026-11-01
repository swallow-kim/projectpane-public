use super::*;
use crate::domain::{ActivationKind, ActivationTarget, Fault, Identity, operations::*};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub(super) struct Fixture {
    pub root: tempfile::TempDir,
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
}

impl Fixture {
    pub fn new(collisions: usize) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        let sources: Vec<_> = ["가나다 😀.bin", "second.bin"]
            .iter()
            .map(|name| source.join(name))
            .collect();
        for (index, path) in sources.iter().enumerate() {
            fs::write(path, format!("source-{index}")).unwrap();
            if index < collisions {
                fs::write(
                    destination.join(path.file_name().unwrap()),
                    format!("sentinel-{index}"),
                )
                .unwrap();
            }
        }
        Self {
            root,
            sources,
            destination,
        }
    }

    pub fn plan(&self) -> Result<OperationPlan, Fault> {
        prepare::prepare(
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
                        entry_ids: (0..self.sources.len())
                            .map(|i| format!("opaque-{i}"))
                            .collect(),
                    },
                    destination: self.destination.to_str().unwrap().into(),
                },
            },
            self.sources
                .iter()
                .map(|path| ActivationTarget {
                    path: path.to_str().unwrap().into(),
                    kind: if path.is_dir() {
                        ActivationKind::Folder
                    } else {
                        ActivationKind::File
                    },
                })
                .collect(),
        )
    }
}

#[tokio::test]
async fn skip_existing_native_mixed_none_and_all_preserve_sources_and_existing_targets() {
    let lane = NativeFileOperationLane::start().unwrap();
    for collisions in 0..=2 {
        // Given an explicitly selected policy with broker-resolved regular files.
        let fixture = Fixture::new(collisions);
        let plan = fixture.plan().unwrap();
        let PreparedAction::CopySkipExisting {
            sources, skipped, ..
        } = &plan.action
        else {
            panic!()
        };
        assert_eq!(sources.len(), 2);
        assert_eq!(skipped.len(), collisions);
        // When the immutable plan executes.
        let report = lane
            .execute(plan, 0, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        // Then only absent targets change and decisions have no HRESULT.
        report.validate().unwrap();
        assert_eq!(
            report.counts.changed,
            u32::try_from(2 - collisions).unwrap()
        );
        assert_eq!(report.counts.skipped, u32::try_from(collisions).unwrap());
        assert_eq!(report.items.len(), 2 - collisions);
        assert_eq!(report.user_skipped.len(), collisions);
        assert_eq!(
            report.user_decisions_only,
            if collisions == 2 { Some(true) } else { None }
        );
        assert_eq!(
            report.status(),
            if collisions == 1 {
                OperationStatus::PartiallySucceeded
            } else {
                OperationStatus::Succeeded
            }
        );
        for (index, source) in fixture.sources.iter().enumerate() {
            assert_eq!(
                fs::read(source).unwrap(),
                format!("source-{index}").as_bytes()
            );
            let target = fixture.destination.join(source.file_name().unwrap());
            let expected = if index < collisions {
                format!("sentinel-{index}")
            } else {
                format!("source-{index}")
            };
            assert_eq!(fs::read(target).unwrap(), expected.as_bytes());
        }
        if collisions == 2 {
            assert!(report.items.is_empty());
            assert!(!report.evidence.perform_succeeded);
            assert_eq!(report.evidence.aborted, None);
        } else {
            assert!(report.evidence.perform_succeeded);
            assert_eq!(report.evidence.aborted, Some(false));
        }
        for decision in report.user_skipped {
            assert!(
                fixture
                    .sources
                    .iter()
                    .any(|p| p == Path::new(&decision.source))
            );
            assert!(Path::new(&decision.destination).starts_with(&fixture.destination));
            assert!(
                serde_json::to_value(decision)
                    .unwrap()
                    .get("hresult")
                    .is_none()
            );
        }
    }
    lane.shutdown().await.unwrap();
}

#[tokio::test]
async fn skip_existing_cancel_before_dispatch_does_not_apply_decisions() {
    let fixture = Fixture::new(2);
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(fixture.plan().unwrap(), 0, Arc::new(AtomicBool::new(true)))
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Cancelled);
    assert!(report.user_skipped.is_empty());
    assert_eq!(report.user_decisions_only, None);
    assert_eq!(report.counts, OutcomeCounts::default());
}

#[tokio::test]
async fn skip_existing_native_failure_keeps_real_failure_evidence_and_user_provenance() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new(1);
    let plan = fixture.plan().unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&fixture.sources[1])
        .unwrap();
    check_plan(&plan).unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(plan.clone(), 0, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    drop(locked);
    lane.shutdown().await.unwrap();
    report.validate_for_plan(&plan).unwrap();
    assert_eq!(report.status(), OperationStatus::Failed);
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 0,
            skipped: 1,
            failed: 1,
            unknown: 0
        }
    );
    assert_eq!(report.user_decisions_only, None);
    assert_eq!(report.user_skipped.len(), 1);
    assert_eq!(report.items.len(), 1);
    assert_eq!(
        report.items[0].hresult,
        windows::Win32::UI::Shell::COPYENGINE_E_SHARING_VIOLATION_SRC.0
    );
    assert!(!report.evidence.perform_succeeded);
    assert_eq!(report.evidence.aborted, Some(false));
    assert!(report.evidence.outcomes_complete);
    assert_eq!(
        fs::read(
            fixture
                .destination
                .join(fixture.sources[0].file_name().unwrap())
        )
        .unwrap(),
        b"sentinel-0"
    );
    assert!(
        !fixture
            .destination
            .join(fixture.sources[1].file_name().unwrap())
            .exists()
    );
}

#[test]
fn skip_existing_native_skips_and_user_decisions_share_only_the_aggregate_count() {
    let report = OperationReport {
        failure: None,
        user_decisions_only: None,
        user_skipped: vec![UserSkipped {
            source: r"C:\source\a".into(),
            destination: r"C:\target\a".into(),
        }],
        items: vec![ItemOutcome {
            source: r"C:\source\b".into(),
            destination: None,
            status: ItemStatus::Skipped,
            hresult: windows::Win32::UI::Shell::COPYENGINE_S_USER_IGNORED.0,
        }],
        counts: OutcomeCounts {
            skipped: 2,
            ..OutcomeCounts::default()
        },
        evidence: FinishEvidence {
            perform_succeeded: true,
            aborted: Some(false),
            outcomes_complete: true,
            cancellation_confirmed: false,
        },
    };
    report.validate().unwrap();
    assert_eq!(report.status(), OperationStatus::Failed);
}
