use super::*;

fn decision_json() -> serde_json::Value {
    serde_json::json!({
        "counts": {"changed":0,"skipped":1,"failed":0,"unknown":0},
        "evidence": {"performSucceeded":false,"aborted":null,
            "outcomesComplete":true,"cancellationConfirmed":false},
        "items": [], "userSkipped": [{"source":"C:\\source\\a",
            "destination":"C:\\target\\a"}], "userDecisionsOnly":true
    })
}

#[test]
fn skip_existing_decision_only_is_success_without_windows_success() {
    let wire = decision_json();
    let report: OperationReport = serde_json::from_value(wire.clone()).unwrap();
    report.validate().unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded);
    assert!(!report.evidence.perform_succeeded);
    assert_eq!(serde_json::to_value(report).unwrap(), wire);
}

#[test]
fn skip_existing_request_has_explicit_policy_and_strict_fields() {
    let wire = serde_json::json!({"projectId":null,"action":{
        "kind":"copy_skip_existing", "selection":{"identity":{
            "pane":0,"tabId":"tab","requestId":"request","generation":1},
            "entryIds":["opaque-1","opaque-2"]}, "destination":"C:\\target"}});
    let bytes = serde_json::to_vec(&wire).unwrap();
    assert!(parse_operation(&bytes).is_ok());
}

#[test]
fn skip_existing_rejects_malformed_decision_counts_evidence_and_provenance() {
    let base = decision_json();
    let mut cases = Vec::new();
    for (field, value) in [
        ("performSucceeded", serde_json::json!(true)),
        ("aborted", serde_json::json!(false)),
        ("outcomesComplete", serde_json::json!(false)),
        ("cancellationConfirmed", serde_json::json!(true)),
    ] {
        let mut wire = base.clone();
        wire["evidence"][field] = value;
        cases.push(wire);
    }
    for (field, value) in [
        ("changed", 1),
        ("skipped", 0),
        ("skipped", 2),
        ("failed", 1),
        ("unknown", 1),
    ] {
        let mut wire = base.clone();
        wire["counts"][field] = value.into();
        cases.push(wire);
    }
    for value in [serde_json::json!(false), serde_json::Value::Null] {
        let mut wire = base.clone();
        wire["userDecisionsOnly"] = value;
        cases.push(wire);
    }
    for value in [serde_json::json!([]), serde_json::Value::Null] {
        let mut wire = base.clone();
        wire["userSkipped"] = value;
        cases.push(wire);
    }
    let mut wire = base.clone();
    wire["userSkipped"][0]["hresult"] = 0.into();
    cases.push(wire);
    let mut wire = base.clone();
    wire["userSkipped"][0]["destination"] = wire["userSkipped"][0]["source"].clone();
    cases.push(wire);
    let mut wire = base.clone();
    wire.as_object_mut().unwrap().remove("userDecisionsOnly");
    cases.push(wire);
    for wire in cases {
        if let Ok(report) = serde_json::from_value::<OperationReport>(wire.clone()) {
            assert!(report.validate().is_err(), "{wire}");
        }
    }
}

#[test]
fn skip_existing_mixed_keeps_native_hresult_and_rejects_overlapping_decisions() {
    let mut wire = decision_json();
    wire.as_object_mut().unwrap().remove("userDecisionsOnly");
    wire["items"] = serde_json::json!([{
        "source":"C:\\source\\b","destination":"C:\\target\\b","status":"changed","hresult":0
    }]);
    wire["counts"]["changed"] = 1.into();
    wire["evidence"]["performSucceeded"] = true.into();
    wire["evidence"]["aborted"] = false.into();
    let report: OperationReport = serde_json::from_value(wire.clone()).unwrap();
    report.validate().unwrap();
    assert_eq!(report.status(), OperationStatus::PartiallySucceeded);
    for field in ["source", "destination"] {
        let mut bad = wire.clone();
        let mut duplicate = bad["userSkipped"][0].clone();
        let other = if field == "source" {
            "destination"
        } else {
            "source"
        };
        duplicate[other] = serde_json::json!("C:\\other\\c");
        bad["userSkipped"].as_array_mut().unwrap().push(duplicate);
        bad["counts"]["skipped"] = 2.into();
        let report: OperationReport = serde_json::from_value(bad).unwrap();
        assert!(report.validate().is_err());
    }
    wire["userSkipped"][0]["source"] = wire["items"][0]["source"].clone();
    let report: OperationReport = serde_json::from_value(wire).unwrap();
    assert!(report.validate().is_err());
}

#[test]
fn skip_existing_legacy_report_serializes_exact_old_shape() {
    let mut wire = decision_json();
    wire.as_object_mut().unwrap().remove("userSkipped");
    wire.as_object_mut().unwrap().remove("userDecisionsOnly");
    wire["counts"]["skipped"] = 0.into();
    wire["evidence"]["aborted"] = true.into();
    wire["evidence"]["cancellationConfirmed"] = true.into();
    let report: OperationReport = serde_json::from_value(wire.clone()).unwrap();
    report.validate().unwrap();
    assert_eq!(serde_json::to_value(report).unwrap(), wire);
}

#[test]
fn skip_existing_report_combined_row_and_frame_bounds_are_unchanged() {
    let mut wire = decision_json();
    wire["userSkipped"] = serde_json::Value::Array((0..MAX_OPERATION_RESULTS).map(|index|
        serde_json::json!({"source":format!("C:\\s\\{index}"),"destination":format!("C:\\t\\{index}")})
    ).collect());
    wire["counts"]["skipped"] = MAX_OPERATION_RESULTS.into();
    let report: OperationReport = serde_json::from_value(wire.clone()).unwrap();
    report.validate().unwrap();
    wire.as_object_mut().unwrap().remove("userDecisionsOnly");
    wire["items"] = serde_json::json!([{"source":"C:\\s\\extra","destination":"C:\\t\\extra",
        "status":"changed","hresult":0}]);
    wire["counts"]["changed"] = 1.into();
    let report: OperationReport = serde_json::from_value(wire).unwrap();
    assert_eq!(
        report.validate().unwrap_err().code,
        crate::domain::FaultCode::ResourceLimit
    );
    let mut report: OperationReport = serde_json::from_value(decision_json()).unwrap();
    report.user_skipped = (0..100)
        .map(|index| UserSkipped {
            source: format!("C:\\{}\\{index}", "s\\".repeat(900)),
            destination: format!("C:\\{}\\{index}", "t\\".repeat(900)),
        })
        .collect();
    report.counts.skipped = 100;
    assert_eq!(
        report.validate().unwrap_err().code,
        crate::domain::FaultCode::PayloadLimit
    );
}

#[test]
fn skip_existing_decision_paths_are_byte_bounded_and_overlap_checks_ignore_ascii_case() {
    for path in [
        "".to_owned(),
        "C:\\bad\0name".to_owned(),
        format!("C:\\{}", "한".repeat(11000)),
    ] {
        let mut wire = decision_json();
        wire["userSkipped"][0]["destination"] = path.into();
        let report: OperationReport = serde_json::from_value(wire).unwrap();
        assert!(report.validate().is_err());
    }
    let mut wire = decision_json();
    wire.as_object_mut().unwrap().remove("userDecisionsOnly");
    wire["items"] = serde_json::json!([{
        "source":"c:\\SOURCE\\A","destination":null,"status":"failed","hresult":-1
    }]);
    wire["counts"]["failed"] = 1.into();
    let report: OperationReport = serde_json::from_value(wire).unwrap();
    assert!(report.validate().is_err());
}
