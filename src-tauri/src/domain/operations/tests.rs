use super::*;
use crate::domain::Identity;

fn selection() -> OperationSelection {
    OperationSelection {
        identity: Identity {
            pane: 0,
            tab_id: "tab-1".into(),
            request_id: "request-1".into(),
            generation: 1,
        },
        entry_ids: vec!["w-0061".into()],
    }
}

fn evidence() -> FinishEvidence {
    FinishEvidence {
        perform_succeeded: true,
        aborted: Some(false),
        outcomes_complete: true,
        cancellation_confirmed: false,
    }
}

#[test]
fn accepts_unicode_single_component_names_and_snapshot_bound_requests() {
    for name in ["한글 자료", "report.txt", ".gitignore", "😀 자료"] {
        assert!(validate_leaf_name(name).is_ok(), "{name}");
    }
    let request = PrepareOperation {
        project_id: None,
        action: OperationAction::Copy {
            selection: selection(),
            destination: r"D:\fixture\destination".into(),
        },
    };
    assert!(request.validate().is_ok());
}

#[test]
fn only_complete_success_evidence_can_report_success() {
    let counts = OutcomeCounts {
        changed: 1,
        ..OutcomeCounts::default()
    };
    assert_eq!(
        classify_outcome(counts, evidence()),
        OperationStatus::Succeeded
    );
    assert_eq!(
        classify_outcome(
            OutcomeCounts {
                skipped: 1,
                ..counts
            },
            evidence()
        ),
        OperationStatus::PartiallySucceeded
    );
    assert_eq!(
        classify_outcome(
            OutcomeCounts {
                skipped: 1,
                ..OutcomeCounts::default()
            },
            evidence()
        ),
        OperationStatus::Failed
    );
}

#[test]
fn restart_never_replays_nonterminal_work_and_retains_terminal_results() {
    for status in [
        OperationStatus::Queued,
        OperationStatus::AwaitingUserDecision,
        OperationStatus::Running,
        OperationStatus::CancelRequested,
    ] {
        assert_eq!(status.after_restart(), OperationStatus::InterruptedUnknown);
    }
    for status in [
        OperationStatus::Succeeded,
        OperationStatus::PartiallySucceeded,
        OperationStatus::Cancelled,
        OperationStatus::Failed,
        OperationStatus::InterruptedUnknown,
    ] {
        assert_eq!(status.after_restart(), status);
    }
}

#[test]
fn rejects_windows_aliases_and_more_than_one_component() {
    for name in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "name.",
        "name ",
        "CON",
        "Con.txt",
        "CON .txt",
        "COM¹.txt",
        "LPT9",
        "NUL",
        "x?",
        "a\0b",
    ] {
        assert!(validate_leaf_name(name).is_err(), "{name:?}");
    }
    assert!(validate_leaf_name(&"😀".repeat(128)).is_err());
    assert!(validate_leaf_name(&"한".repeat(255)).is_ok());
}

#[test]
fn rejects_duplicate_oversized_and_invalid_snapshot_selections() {
    let mut picked = selection();
    picked.entry_ids.push(picked.entry_ids[0].clone());
    assert!(picked_request(picked).validate().is_err());
    let mut picked = selection();
    picked.entry_ids = (0..=MAX_OPERATION_SOURCES)
        .map(|i| format!("w-{i}"))
        .collect();
    assert!(picked_request(picked).validate().is_err());
    let mut picked = selection();
    picked.identity.generation = 0;
    assert!(picked_request(picked).validate().is_err());
    let mut picked = selection();
    picked.entry_ids.clear();
    assert!(picked_request(picked).validate().is_err());
}

fn picked_request(selection: OperationSelection) -> PrepareOperation {
    PrepareOperation {
        project_id: None,
        action: OperationAction::Recycle { selection },
    }
}

#[test]
fn rejects_ambiguous_json_and_raw_frame_overflow() {
    for bytes in [
        br#"{"projectId":null,"projectId":"two","action":{"kind":"new_folder","parent":"D:\\fixture","name":"x"}}"#.as_slice(),
        br#"{"projectId":null,"action":{"kind":"new_folder","parent":"D:\\fixture","name":"x","unknown":true}}"#,
        br#"{"action":{"kind":"new_folder","parent":"D:\\fixture","name":"x"}}"#,
    ] {
        assert!(parse_operation(bytes).is_err());
    }
    let bytes = vec![b' '; crate::domain::MAX_FRAME];
    assert_eq!(
        parse_operation(&bytes).unwrap_err().code,
        crate::domain::FaultCode::PayloadLimit
    );
    let request = picked_request(selection());
    assert_eq!(
        parse_operation(&serde_json::to_vec(&request).unwrap()).unwrap(),
        request
    );
}

#[test]
fn cancellation_and_unknown_outcomes_never_hide_confirmed_changes() {
    let cancel = FinishEvidence {
        aborted: Some(true),
        cancellation_confirmed: true,
        ..evidence()
    };
    assert_eq!(
        classify_outcome(OutcomeCounts::default(), cancel),
        OperationStatus::Cancelled
    );
    assert_eq!(
        classify_outcome(
            OutcomeCounts {
                changed: 1,
                ..OutcomeCounts::default()
            },
            cancel
        ),
        OperationStatus::PartiallySucceeded
    );
    let counts = OutcomeCounts {
        changed: 1,
        ..OutcomeCounts::default()
    };
    for uncertain in [
        FinishEvidence {
            aborted: None,
            ..evidence()
        },
        FinishEvidence {
            outcomes_complete: false,
            ..evidence()
        },
        FinishEvidence {
            cancellation_confirmed: true,
            ..evidence()
        },
    ] {
        assert_eq!(
            classify_outcome(counts, uncertain),
            OperationStatus::InterruptedUnknown
        );
    }
    assert_eq!(
        classify_outcome(
            OutcomeCounts {
                unknown: 1,
                ..counts
            },
            evidence()
        ),
        OperationStatus::InterruptedUnknown
    );
    assert_eq!(
        classify_outcome(OutcomeCounts::default(), evidence()),
        OperationStatus::InterruptedUnknown
    );
}
