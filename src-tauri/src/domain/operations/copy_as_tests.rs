use super::*;
use crate::domain::{EntryKind, FaultCode};

fn request() -> serde_json::Value {
    serde_json::json!({
        "projectId": null,
        "action": {"kind": "copy_as", "destination": "D:\\target", "name": "사본 😀.txt",
            "selection": {"identity": {"pane": 0, "tabId": "tab-1",
                "requestId": "request-1", "generation": 1}, "entryIds": ["opaque-1"]}}
    })
}

#[test]
fn copy_as_accepts_exact_single_selection_and_round_trips_wire_contract() {
    // Given a single opaque entry and an explicit Unicode leaf.
    let value = request();
    // When parsing the IPC request.
    let parsed = parse_operation(&serde_json::to_vec(&value).unwrap()).unwrap();
    // Then the typed action retains the exact contract.
    assert_eq!(parsed.selection().unwrap().entry_ids, ["opaque-1"]);
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn copy_as_rejects_invalid_selection_leaf_and_unrecognized_fields() {
    // Given malformed names, selection counts, IDs, and extra policy fields.
    let mut invalid = Vec::new();
    for name in [
        "", "..", "a/b", "a\\b", "C:\\x", "NUL.txt", "x.", "x ", "a:b", "a\0b",
    ] {
        let mut value = request();
        value["action"]["name"] = name.into();
        invalid.push(value);
    }
    for ids in [vec![], vec!["one", "two"], vec!["one", "one"], vec![""]] {
        let mut value = request();
        value["action"]["selection"]["entryIds"] = serde_json::json!(ids);
        invalid.push(value);
    }
    let mut extra = request();
    extra["action"]["overwrite"] = true.into();
    invalid.push(extra);
    // When parsing each request, then none is admitted.
    for value in invalid {
        assert!(
            parse_operation(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{value}"
        );
    }
}

fn intent(kind: EntryKind) -> serde_json::Value {
    serde_json::json!({"schemaVersion": 1, "id": "copy-as-1", "plan": {
        "projectId": null, "action": {"kind": "copy_as", "name": "사본.txt",
            "source": {"path": "D:\\source.txt", "kind": kind, "volume": 1, "fileIndex": 1},
            "destination": {"path": "D:\\target", "kind": "folder", "volume": 1, "fileIndex": 2}}}})
}

#[test]
fn copy_as_durable_intent_retains_identities_and_requires_regular_file() {
    // Given a persisted intent containing captured identities and target leaf.
    let value = intent(EntryKind::File);
    // When decoding and validating it.
    let parsed: OperationIntent = serde_json::from_value(value.clone()).unwrap();
    parsed.validate().unwrap();
    // Then round trip preserves all intent and non-file sources are refused.
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    for kind in [EntryKind::Folder, EntryKind::Link, EntryKind::Unknown] {
        let parsed: OperationIntent = serde_json::from_value(intent(kind)).unwrap();
        assert!(parsed.validate().is_err());
    }
    let mut value = intent(EntryKind::File);
    value["plan"]["action"]["name"] = "..".into();
    let parsed: OperationIntent = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.validate().unwrap_err().code, FaultCode::InvalidPath);
}

#[test]
fn copy_as_parser_keeps_utf16_leaf_and_raw_frame_limits() {
    let mut value = request();
    value["action"]["name"] = "한".repeat(255).into();
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.resize(crate::domain::MAX_FRAME - 1, b' ');
    assert!(parse_operation(&bytes).is_ok());
    bytes.push(b' ');
    assert_eq!(
        parse_operation(&bytes).unwrap_err().code,
        FaultCode::PayloadLimit
    );
    value["action"]["name"] = "😀".repeat(128).into();
    assert_eq!(
        parse_operation(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code,
        FaultCode::InvalidPath
    );
}

#[test]
fn copy_as_and_skip_existing_bind_confirmed_rows_but_keep_unknown_evidence() {
    let intent: OperationIntent = serde_json::from_value(intent(EntryKind::File)).unwrap();
    let PreparedAction::CopyAs {
        source,
        destination,
        ..
    } = &intent.plan.action
    else {
        panic!("expected named copy");
    };
    let plans = [
        (intent.plan.clone(), "D:\\target\\사본.txt"),
        (
            OperationPlan {
                project_id: None,
                action: PreparedAction::CopySkipExisting {
                    sources: vec![source.clone()],
                    destination: destination.clone(),
                    skipped: vec![],
                },
            },
            "D:\\target\\source.txt",
        ),
    ];
    for (plan, target) in plans {
        plan.validate().unwrap();
        let report = OperationReport {
            failure: None,
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
            items: vec![ItemOutcome {
                source: "D:\\source.txt".into(),
                destination: Some(target.into()),
                status: ItemStatus::Changed,
                hresult: 0,
            }],
            user_skipped: vec![],
            user_decisions_only: None,
        };
        report.validate_for_plan(&plan).unwrap();
        for case in 0..4 {
            let mut bad = report.clone();
            match case {
                0 => bad.items[0].source = "D:\\unselected.txt".into(),
                1 => bad.items[0].destination = Some("D:\\unapproved.txt".into()),
                2 => bad.items[0].destination = None,
                _ => {
                    bad.items.push(bad.items[0].clone());
                    bad.counts.changed = 2;
                }
            }
            bad.validate().unwrap();
            assert!(bad.validate_for_plan(&plan).is_err(), "case {case}");
        }
        let mut unknown = report;
        unknown.items[0].destination = Some("D:\\unexpected.txt".into());
        unknown.items[0].status = ItemStatus::Unknown;
        unknown.counts = OutcomeCounts {
            unknown: 1,
            ..OutcomeCounts::default()
        };
        unknown.evidence.outcomes_complete = false;
        unknown.validate_for_plan(&plan).unwrap();
        assert_eq!(unknown.status(), OperationStatus::InterruptedUnknown);
    }
}
