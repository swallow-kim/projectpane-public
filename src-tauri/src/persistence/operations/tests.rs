use super::*;
use crate::domain::{EntryKind, operations::*};

fn intent(id: &str) -> OperationIntent {
    OperationIntent {
        schema_version: 1,
        id: id.into(),
        plan: OperationPlan {
            project_id: None,
            action: PreparedAction::NewFolder {
                parent: OperationSource {
                    path: r"D:\fixture".into(),
                    kind: EntryKind::Folder,
                    volume: 1,
                    file_index: 1,
                },
                name: "한글 자료".into(),
            },
        },
    }
}

fn cancelled(id: &str) -> OperationCompletion {
    OperationCompletion {
        schema_version: 1,
        id: id.into(),
        report: OperationReport {
            failure: None,
            user_skipped: vec![],
            user_decisions_only: None,
            counts: OutcomeCounts::default(),
            items: vec![],
            evidence: FinishEvidence {
                perform_succeeded: false,
                aborted: Some(true),
                outcomes_complete: true,
                cancellation_confirmed: true,
            },
        },
    }
}

#[test]
fn durable_intent_survives_restart_without_replaying_and_terminal_is_immutable() {
    let root = tempfile::tempdir().unwrap();
    let (mut journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
    assert!(recovered.is_empty());
    journal.record_intent(&intent("run-1")).unwrap();
    journal.record_intent(&intent("run-2")).unwrap();
    journal.record_completion(&cancelled("run-2")).unwrap();
    assert!(journal.record_intent(&intent("run-1")).is_err());
    assert!(journal.record_completion(&cancelled("run-2")).is_err());
    drop(journal);
    for _ in 0..2 {
        let (_journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].status(), OperationStatus::InterruptedUnknown);
        assert_eq!(recovered[1].status(), OperationStatus::Cancelled);
    }
}

#[test]
fn cross_volume_tree_intent_reopens_unknown_without_replay() {
    let root = tempfile::tempdir().unwrap();
    let source = OperationSource {
        path: r"C:\synthetic\tree".into(),
        kind: EntryKind::Folder,
        volume: 1,
        file_index: 10,
    };
    let child = OperationSource {
        path: r"C:\synthetic\tree\file.txt".into(),
        kind: EntryKind::File,
        volume: 1,
        file_index: 11,
    };
    let destination = OperationSource {
        path: r"D:\synthetic".into(),
        kind: EntryKind::Folder,
        volume: 2,
        file_index: 20,
    };
    let mut intent = intent("tree-move");
    intent.plan.action = PreparedAction::MoveTree {
        roots: vec![source],
        descendants: vec![child],
        destination,
    };
    intent.plan.validate().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    journal.record_intent(&intent).unwrap();
    drop(journal);
    for _ in 0..2 {
        let (mut journal, records) = OperationJournal::open(root.path(), None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status(), OperationStatus::InterruptedUnknown);
        assert!(journal.record_intent(&intent).is_err());
    }
}

#[test]
fn torn_result_is_unknown_and_original_bytes_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    journal.record_intent(&intent("run-1")).unwrap();
    let path = journal.directory.join("run-1.result.json");
    fs::write(&path, b"{\"schemaVersion\":").unwrap();
    drop(journal);
    let (_journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
    assert_eq!(recovered[0].status(), OperationStatus::InterruptedUnknown);
    assert_eq!(fs::read(path).unwrap(), b"{\"schemaVersion\":");
}

#[test]
fn torn_intent_and_future_schema_refuse_admission_without_erasing_evidence() {
    for bytes in [b"{".as_slice(), br#"{"schemaVersion":2}"#] {
        let root = tempfile::tempdir().unwrap();
        let (journal, _) = OperationJournal::open(root.path(), None).unwrap();
        let path = journal.directory.join("run-1.intent.json");
        fs::write(&path, bytes).unwrap();
        drop(journal);
        assert!(OperationJournal::open(root.path(), None).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn journal_lock_and_capacity_refuse_concurrent_or_unbounded_admission() {
    let root = tempfile::tempdir().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    assert!(OperationJournal::open(root.path(), None).is_err());
    for i in 0..MAX_OPERATION_HISTORY {
        journal.record_intent(&intent(&format!("run-{i}"))).unwrap();
    }
    assert_eq!(journal.remaining(), 0);
    assert!(journal.record_intent(&intent("overflow")).is_err());
    assert!(!journal.directory.join("overflow.intent.json").exists());
}

#[test]
fn failed_intent_write_does_not_authorize_later_work() {
    let root = tempfile::tempdir().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    fs::create_dir(journal.directory.join("run-1.intent.json")).unwrap();
    assert!(journal.record_intent(&intent("run-1")).is_err());
    assert!(journal.record_intent(&intent("run-2")).is_err());
    assert!(!journal.directory.join("run-2.intent.json").exists());
}

#[test]
fn retirement_releases_capacity_without_touching_referenced_files() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("owned-payload.txt");
    fs::write(&sentinel, b"keep all user bytes").unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    for i in 0..MAX_OPERATION_HISTORY {
        let id = format!("run-{i}");
        journal.record_intent(&intent(&id)).unwrap();
        journal.record_completion(&cancelled(&id)).unwrap();
    }
    assert_eq!(journal.remaining(), 0);
    journal.retire(&cancelled("run-42")).unwrap();
    assert_eq!(journal.remaining(), 1);
    assert!(!journal.directory.join("run-42.intent.json").exists());
    assert!(!journal.directory.join("run-42.result.json").exists());
    assert!(!journal.directory.join("run-42.retired.json").exists());
    journal.record_intent(&intent("new-record")).unwrap();
    drop(journal);
    let (journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
    assert_eq!(journal.remaining(), 0);
    assert_eq!(recovered.len(), MAX_OPERATION_HISTORY);
    assert!(recovered.iter().all(|record| record.intent.id != "run-42"));
    assert_eq!(fs::read(sentinel).unwrap(), b"keep all user bytes");
}

#[test]
fn restart_finishes_each_retirement_step_without_replaying_or_deleting_live_intents() {
    for stage in 0..=3 {
        let root = tempfile::tempdir().unwrap();
        let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
        journal.record_intent(&intent("done")).unwrap();
        journal.record_completion(&cancelled("done")).unwrap();
        journal.record_intent(&intent("unfinished")).unwrap();
        let directory = journal.directory.clone();
        let unfinished = fs::read(directory.join("unfinished.intent.json")).unwrap();
        if stage >= 1 {
            fs::rename(
                directory.join("done.intent.json"),
                directory.join("done.retired.json"),
            )
            .unwrap();
        }
        if stage >= 2 {
            fs::remove_file(directory.join("done.result.json")).unwrap();
        }
        if stage >= 3 {
            fs::remove_file(directory.join("done.retired.json")).unwrap();
        }
        drop(journal);
        for _ in 0..2 {
            let (journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
            assert_eq!(recovered.len(), if stage == 0 { 2 } else { 1 });
            assert_eq!(journal.remaining(), MAX_OPERATION_HISTORY - recovered.len());
            assert_eq!(
                recovered
                    .iter()
                    .find(|r| r.intent.id == "unfinished")
                    .unwrap()
                    .status(),
                OperationStatus::InterruptedUnknown
            );
            assert_eq!(
                fs::read(directory.join("unfinished.intent.json")).unwrap(),
                unfinished
            );
            assert!(!directory.join("done.retired.json").exists());
        }
    }
}

#[test]
fn corrupt_or_ambiguous_inventory_blocks_retirement_recovery_before_any_cleanup() {
    for kind in ["torn", "future", "duplicate", "orphan"] {
        let root = tempfile::tempdir().unwrap();
        let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
        journal.record_intent(&intent("valid")).unwrap();
        journal.record_completion(&cancelled("valid")).unwrap();
        let directory = journal.directory.clone();
        fs::rename(
            directory.join("valid.intent.json"),
            directory.join("valid.retired.json"),
        )
        .unwrap();
        let bad = directory.join("zz-invalid.retired.json");
        match kind {
            "torn" => fs::write(&bad, b"{").unwrap(),
            "future" => {
                let mut future = intent("zz-invalid");
                future.schema_version = 2;
                files::create(&bad, &future).unwrap();
            }
            "duplicate" => {
                files::create(&bad, &intent("zz-invalid")).unwrap();
                files::create(
                    &directory.join("zz-invalid.intent.json"),
                    &intent("zz-invalid"),
                )
                .unwrap();
            }
            _ => files::create(
                &directory.join("zz-invalid.result.json"),
                &cancelled("zz-invalid"),
            )
            .unwrap(),
        }
        drop(journal);
        let before: BTreeMap<_, _> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect();
        assert!(OperationJournal::open(root.path(), None).is_err());
        for (name, bytes) in before {
            assert_eq!(fs::read(directory.join(name)).unwrap(), bytes);
        }
    }
}

#[test]
fn changed_disk_completion_is_preserved_and_does_not_release_capacity() {
    let root = tempfile::tempdir().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    journal.record_intent(&intent("done")).unwrap();
    journal.record_completion(&cancelled("done")).unwrap();
    let file = journal.directory.join("done.result.json");
    let damaged = b"{\"schemaVersion\":999}";
    fs::write(&file, damaged).unwrap();
    assert!(journal.retire(&cancelled("done")).is_err());
    assert_eq!(journal.remaining(), MAX_OPERATION_HISTORY - 1);
    assert!(journal.record_intent(&intent("blocked")).is_err());
    assert_eq!(fs::read(file).unwrap(), damaged);
    assert!(journal.directory.join("done.intent.json").exists());
}

#[cfg(windows)]
#[test]
fn sharing_failure_keeps_retirement_marker_until_a_safe_restart() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let (mut journal, _) = OperationJournal::open(root.path(), None).unwrap();
    journal.record_intent(&intent("done")).unwrap();
    journal.record_completion(&cancelled("done")).unwrap();
    let directory = journal.directory.clone();
    let marker = directory.join("done.retired.json");
    fs::rename(directory.join("done.intent.json"), &marker).unwrap();
    drop(journal);
    let held = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&marker)
        .unwrap();
    assert!(OperationJournal::open(root.path(), None).is_err());
    assert!(marker.exists());
    assert!(!directory.join("done.result.json").exists());
    drop(held);
    let (journal, recovered) = OperationJournal::open(root.path(), None).unwrap();
    assert!(recovered.is_empty());
    assert_eq!(journal.remaining(), MAX_OPERATION_HISTORY);
    assert!(!marker.exists());
}
