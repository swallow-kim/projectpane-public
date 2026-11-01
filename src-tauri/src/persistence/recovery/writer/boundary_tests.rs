use super::*;
use crate::{domain::MAX_FRAME, persistence::recovery::tests::record};
use std::fs;

fn phase(id: &str, name: &str) -> RecoveryRecord {
    RecoveryRecord::parse(&serde_json::to_vec(&record(id, name)).unwrap()).unwrap()
}

fn directory() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    File::create(root.path().join("journal.lock")).unwrap();
    root
}

#[test]
fn full_history_allows_next_phase_but_refuses_new_identity() {
    let root = directory();
    for index in 0..MAX_OPERATION_HISTORY {
        let id = format!("id-{index}");
        fs::write(
            root.path().join(format!("{id}.ready.json")),
            phase(&id, "ready").to_bytes().unwrap(),
        )
        .unwrap();
    }
    assert!(matches!(
        append_phase(root.path(), &phase("overflow", "ready")),
        Err(AppendFailure::NotCreated(_))
    ));
    assert!(!root.path().join("overflow.ready.json").exists());
    let _ack = append_phase(root.path(), &phase("id-0", "preserve_requested")).unwrap();
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        MAX_OPERATION_HISTORY + 2
    );
}

#[test]
fn total_byte_limit_refuses_before_creating_an_otherwise_valid_next_phase() {
    let root = directory();
    for index in 0..16 {
        let id = format!("id-{index}");
        let mut bytes = phase(&id, "ready").to_bytes().unwrap();
        bytes.resize(MAX_FRAME - 1, b' ');
        fs::write(root.path().join(format!("{id}.ready.json")), bytes).unwrap();
    }
    assert!(matches!(
        append_phase(root.path(), &phase("id-0", "preserve_requested")),
        Err(AppendFailure::NotCreated(_))
    ));
    assert!(!root.path().join("id-0.preserve_requested.json").exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 17);
}

#[test]
fn corrupt_or_occupied_phase_name_is_never_overwritten() {
    for bytes in [b"{broken".as_slice(), b"do not replace".as_slice()] {
        let root = directory();
        let path = root.path().join("proof-1.ready.json");
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            append_phase(root.path(), &phase("proof-1", "ready")),
            Err(AppendFailure::NotCreated(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
