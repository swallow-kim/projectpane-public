use super::*;
use crate::domain::EntryKind;

fn source(index: u64, kind: EntryKind) -> OperationSource {
    OperationSource {
        path: format!(r"D:\fixture\entry-{index}"),
        kind,
        volume: 1,
        file_index: index,
    }
}

#[test]
fn duplicate_file_identity_and_non_directory_destinations_are_rejected() {
    let first = source(1, EntryKind::File);
    let alias = OperationSource {
        path: r"D:\fixture\hardlink".into(),
        ..first.clone()
    };
    let plan = OperationPlan {
        project_id: None,
        action: PreparedAction::Copy {
            sources: vec![first, alias],
            destination: source(2, EntryKind::Folder),
        },
    };
    assert!(plan.validate().is_err());
    for kind in [EntryKind::File, EntryKind::Link, EntryKind::Unknown] {
        let plan = OperationPlan {
            project_id: None,
            action: PreparedAction::NewFolder {
                parent: source(1, kind),
                name: "new".into(),
            },
        };
        assert!(plan.validate().is_err());
    }
}

#[test]
fn unobserved_success_counts_and_contradictory_hresult_are_rejected() {
    let mut report = OperationReport {
        failure: None,
        user_skipped: vec![],
        user_decisions_only: None,
        counts: OutcomeCounts {
            changed: 1,
            ..OutcomeCounts::default()
        },
        evidence: FinishEvidence {
            perform_succeeded: true,
            aborted: Some(false),
            outcomes_complete: true,
            cancellation_confirmed: false,
        },
        items: vec![],
    };
    assert!(report.validate().is_err());
    report.items.push(ItemOutcome {
        source: r"D:\fixture\a".into(),
        destination: None,
        status: ItemStatus::Changed,
        hresult: -1,
    });
    assert!(report.validate().is_err());
    report.items[0].hresult = 0;
    assert!(report.validate().is_ok());
}

#[test]
fn old_reports_remain_readable_and_dispatch_faults_cannot_claim_native_success() {
    let old = r#"{"counts":{"changed":0,"skipped":0,"failed":0,"unknown":0},"items":[],"evidence":{"performSucceeded":false,"aborted":false,"outcomesComplete":true,"cancellationConfirmed":false}}"#;
    let mut report: OperationReport = serde_json::from_str(old).unwrap();
    report.validate().unwrap();
    assert!(report.failure.is_none());
    report.failure = Some(crate::domain::Fault::new(
        crate::domain::FaultCode::StaleRequest,
    ));
    report.validate().unwrap();
    assert_eq!(report.status(), OperationStatus::Failed);
    let recovered: OperationReport =
        serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(recovered.failure, report.failure);
    report.evidence.perform_succeeded = true;
    assert!(report.validate().is_err());
    report.evidence.perform_succeeded = false;
    report.counts.changed = 1;
    assert!(report.validate().is_err());
    report.counts = OutcomeCounts::default();
    report.evidence.cancellation_confirmed = true;
    assert!(report.validate().is_err());
    report.evidence.cancellation_confirmed = false;
    report.evidence.aborted = Some(true);
    assert!(report.validate().is_err());
    report.evidence.aborted = None;
    report.evidence.outcomes_complete = false;
    report.failure = Some(crate::domain::Fault::new(
        crate::domain::FaultCode::WorkerUnavailable,
    ));
    report.validate().unwrap();
    assert_eq!(report.status(), OperationStatus::InterruptedUnknown);
}
