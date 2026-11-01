use super::*;
use crate::domain::EntryKind;
use std::sync::atomic::Ordering;

fn queue(root: &std::path::Path, run: &str) -> OperationQueue {
    let (journal, recovered) = OperationJournal::open(root, None).unwrap();
    OperationQueue::new(journal, recovered, run.into()).unwrap()
}

fn plan(name: &str) -> OperationPlan {
    OperationPlan {
        project_id: Some("project-1".into()),
        action: PreparedAction::NewFolder {
            parent: OperationSource {
                path: r"D:\fixture".into(),
                kind: EntryKind::Folder,
                volume: 1,
                file_index: 1,
            },
            name: name.into(),
        },
    }
}

fn changed() -> OperationReport {
    OperationReport {
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
        items: vec![ItemOutcome {
            source: r"D:\fixture".into(),
            destination: Some(r"D:\fixture\new".into()),
            status: ItemStatus::Changed,
            hresult: 0,
        }],
    }
}

#[test]
fn reviewed_terminal_history_can_be_removed_but_never_reexecuted() {
    let root = tempfile::tempdir().unwrap();
    let mut service = queue(root.path(), "first-run");
    let done = service.prepare(plan("done")).unwrap();
    let active = service.prepare(plan("active")).unwrap();
    service.submit(&done).unwrap();
    service.submit(&active).unwrap();
    assert_eq!(service.forget(&done).unwrap_err().code, FaultCode::Busy);
    assert_eq!(service.start_next().unwrap().id, done);
    assert_eq!(service.forget(&done).unwrap_err().code, FaultCode::Busy);
    service.finish(&done, changed()).unwrap();
    assert_eq!(service.start_next().unwrap().id, active);
    service.forget(&done).unwrap();
    service.forget(&done).unwrap();
    assert!(service.snapshot(&done).is_none());
    assert_eq!(
        service.submit(&done).unwrap_err().code,
        FaultCode::StaleRequest
    );
    assert_eq!(
        service.snapshot(&active).unwrap().status,
        OperationStatus::Running
    );
    drop(service);
    let mut restarted = queue(root.path(), "second-run");
    assert_eq!(
        restarted.submit(&done).unwrap_err().code,
        FaultCode::StaleRequest
    );
    assert_eq!(restarted.forget(&active).unwrap_err().code, FaultCode::Busy);
    assert_eq!(
        restarted.snapshot(&active).unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    assert!(restarted.start_next().is_none());
}

#[test]
fn submit_is_idempotent_and_only_one_immutable_operation_runs() {
    let root = tempfile::tempdir().unwrap();
    let mut queue = queue(root.path(), "run");
    let original = plan("한글");
    let first = queue.prepare(original.clone()).unwrap();
    let second = queue.prepare(plan("second")).unwrap();
    queue.submit(&first).unwrap();
    queue.submit(&first).unwrap();
    queue.submit(&second).unwrap();
    let work = queue.start_next().unwrap();
    assert_eq!(work.id, first);
    assert_eq!(work.plan, original);
    assert!(queue.start_next().is_none());
    assert!(queue.finish(&second, changed()).is_err());
    assert_eq!(
        queue.finish(&first, changed()).unwrap().status,
        OperationStatus::Succeeded
    );
    assert_eq!(
        queue.submit(&first).unwrap().status,
        OperationStatus::Succeeded
    );
    assert_eq!(queue.start_next().unwrap().id, second);
    queue.finish(&second, changed()).unwrap();
    assert!(queue.start_next().is_none());
}

#[test]
fn running_cancellation_keeps_slot_until_actual_completion_even_after_close() {
    let root = tempfile::tempdir().unwrap();
    let mut queue = queue(root.path(), "run");
    let first = queue.prepare(plan("first")).unwrap();
    let second = queue.prepare(plan("second")).unwrap();
    queue.submit(&first).unwrap();
    queue.submit(&second).unwrap();
    let work = queue.start_next().unwrap();
    queue.begin_close();
    assert!(queue.prepare(plan("blocked")).is_err());
    assert_eq!(
        queue.request_cancel(&first).unwrap().status,
        OperationStatus::CancelRequested
    );
    assert!(work.cancel.load(Ordering::Acquire));
    assert!(!queue.is_idle());
    assert!(queue.start_next().is_none());
    assert_eq!(
        queue.request_cancel(&second).unwrap().status,
        OperationStatus::Cancelled
    );
    assert_eq!(
        queue.finish(&first, changed()).unwrap().status,
        OperationStatus::Succeeded
    );
    assert_eq!(
        queue.request_cancel(&first).unwrap().status,
        OperationStatus::Succeeded
    );
    assert!(queue.is_idle());
    queue.keep_open();
    assert!(queue.prepare(plan("allowed")).is_ok());
}

#[test]
fn restart_marks_unfinished_unknown_and_never_dispatches_or_reuses_its_id() {
    let root = tempfile::tempdir().unwrap();
    let mut first = queue(root.path(), "first-run");
    let id = first.prepare(plan("new")).unwrap();
    first.submit(&id).unwrap();
    first.start_next().unwrap();
    drop(first);
    for run in ["second-run", "third-run"] {
        let mut restarted = queue(root.path(), run);
        assert_eq!(
            restarted.submit(&id).unwrap().status,
            OperationStatus::InterruptedUnknown
        );
        assert!(restarted.start_next().is_none());
        assert!(restarted.submit("first-run-999").is_err());
        assert_ne!(restarted.prepare(plan("another")).unwrap(), id);
    }
}

#[test]
fn prepared_and_queued_bounds_are_enforced_without_consuming_failed_submission() {
    let root = tempfile::tempdir().unwrap();
    let mut queue = queue(root.path(), "run");
    let ids: Vec<_> = (0..MAX_PREPARED_OPERATIONS)
        .map(|_| queue.prepare(plan("new")).unwrap())
        .collect();
    assert!(queue.prepare(plan("overflow")).is_err());
    for id in &ids {
        queue.submit(id).unwrap();
    }
    let extra = queue.prepare(plan("extra")).unwrap();
    assert!(queue.submit(&extra).is_err());
    queue.request_cancel(&ids[0]).unwrap();
    assert_eq!(
        queue.submit(&extra).unwrap().status,
        OperationStatus::Queued
    );
}

#[test]
fn storage_failure_prevents_dispatch_and_known_completion_is_not_lost_in_memory() {
    let root = tempfile::tempdir().unwrap();
    let mut queue = queue(root.path(), "run");
    let first = queue.prepare(plan("first")).unwrap();
    let second = queue.prepare(plan("second")).unwrap();
    queue.submit(&first).unwrap();
    queue.submit(&second).unwrap();
    queue.start_next().unwrap();
    let result_path = root
        .path()
        .join("foundation/operations-v1")
        .join(format!("{first}.result.json"));
    std::fs::create_dir(&result_path).unwrap();
    let result = queue.finish(&first, changed()).unwrap();
    assert_eq!(result.status, OperationStatus::Succeeded);
    assert!(!result.journal_saved);
    assert_eq!(queue.report(&first).unwrap().items.len(), 1);
    assert!(queue.start_next().is_none());
    assert!(queue.prepare(plan("refused")).is_err());
    assert_eq!(
        queue.request_cancel(&second).unwrap().status,
        OperationStatus::Cancelled
    );
    assert!(queue.is_idle());
}

#[test]
fn failed_intent_write_never_enters_dispatch_queue() {
    let root = tempfile::tempdir().unwrap();
    let mut queue = queue(root.path(), "run");
    let id = queue.prepare(plan("new")).unwrap();
    std::fs::create_dir(
        root.path()
            .join("foundation/operations-v1")
            .join(format!("{id}.intent.json")),
    )
    .unwrap();
    assert!(queue.submit(&id).is_err());
    assert!(queue.start_next().is_none());
    assert!(queue.is_idle());
}
