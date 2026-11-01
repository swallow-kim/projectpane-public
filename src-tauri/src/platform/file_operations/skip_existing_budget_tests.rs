use super::sink_state::SinkContext;
use crate::domain::{MAX_FRAME, operations::*};
use std::sync::{Arc, atomic::AtomicBool};
use windows::core::HRESULT;

#[test]
fn skip_existing_reserved_rows_leave_every_decision_identity_in_the_completion() {
    let decisions: Vec<_> = (0..1024)
        .map(|index| UserSkipped {
            source: format!("C:\\source\\{index}"),
            destination: format!("C:\\target\\{index}"),
        })
        .collect();
    let context = SinkContext::new(1025, Arc::new(AtomicBool::new(false)), Arc::default());
    {
        let mut state = context.state.borrow_mut();
        state.reserved_rows = decisions.len();
        state.bytes += serde_json::to_vec(&decisions).unwrap().len() + 32;
        for index in 0..1025 {
            let result = state.push(ItemOutcome {
                source: format!("C:\\native\\{index}"),
                destination: Some(format!("C:\\target\\native-{index}")),
                status: ItemStatus::Changed,
                hresult: 0,
            });
            assert_eq!(result.is_ok(), index < 1024);
        }
        state.finished = Some(HRESULT(0));
    }
    let mut report = context.finish(Ok(()), Some(false));
    report.user_skipped = decisions.clone();
    report.counts.skipped += 1024;
    report.validate().unwrap();
    assert_eq!(
        report.items.len() + report.user_skipped.len(),
        MAX_OPERATION_RESULTS
    );
    assert_eq!(report.user_skipped, decisions);
    assert!(!report.evidence.outcomes_complete);
    assert_eq!(report.status(), OperationStatus::InterruptedUnknown);
    assert!(
        serde_json::to_vec(&OperationCompletion {
            schema_version: 1,
            id: "bounded".into(),
            report
        })
        .unwrap()
        .len()
            < MAX_FRAME
    );
}

#[test]
fn skip_existing_reserved_bytes_cannot_be_consumed_by_native_callbacks() {
    let decisions: Vec<_> = (0..100)
        .map(|index| UserSkipped {
            source: format!("C:\\{}\\{index}", "s\\".repeat(190)),
            destination: format!("C:\\{}\\{index}", "t\\".repeat(190)),
        })
        .collect();
    let context = SinkContext::new(200, Arc::new(AtomicBool::new(false)), Arc::default());
    {
        let mut state = context.state.borrow_mut();
        state.reserved_rows = decisions.len();
        state.bytes += serde_json::to_vec(&decisions).unwrap().len() + 32;
        let mut refused = false;
        for index in 0..200 {
            if state
                .push(ItemOutcome {
                    source: format!("C:\\{}\\native-{index}", "s\\".repeat(200)),
                    destination: Some(format!("C:\\{}\\native-{index}", "t\\".repeat(200))),
                    status: ItemStatus::Changed,
                    hresult: 0,
                })
                .is_err()
            {
                refused = true;
                break;
            }
        }
        assert!(refused);
        state.finished = Some(HRESULT(0));
    }
    let mut report = context.finish(Ok(()), Some(false));
    report.user_skipped = decisions.clone();
    report.counts.skipped += 100;
    report.validate().unwrap();
    assert_eq!(report.user_skipped, decisions);
    assert!(!report.evidence.outcomes_complete);
    assert!(
        serde_json::to_vec(&OperationCompletion {
            schema_version: 1,
            id: "bounded".into(),
            report
        })
        .unwrap()
        .len()
            < MAX_FRAME
    );
}
