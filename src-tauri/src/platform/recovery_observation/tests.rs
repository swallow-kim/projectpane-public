use super::*;
use crate::platform::file_operations::capture_source;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn object(path: &Path) -> Value {
    serde_json::to_value(capture_source(path.to_str().unwrap()).unwrap()).unwrap()
}

pub(super) fn captured(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    let sha: [u8; 32] = Sha256::digest(&bytes).into();
    json!({"object":object(path),"content":{"bytes":bytes.len(),"sha256":sha}})
}

fn fixture() -> (tempfile::TempDir, RecoveryRecord) {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("target");
    let artifacts = parent.join(".projectpane-recovery-proof-1");
    fs::create_dir_all(&artifacts).unwrap();
    let source = root.path().join("source");
    let target = parent.join("한글.txt");
    let staged = artifacts.join("staged");
    fs::write(&source, b"abc").unwrap();
    fs::write(&staged, b"abc").unwrap();
    fs::write(&target, b"original").unwrap();
    let value = json!({"schemaVersion":1,"id":"proof-1","phase":"ready",
        "source":captured(&source),"original":captured(&target),"staged":captured(&staged),
        "parent":object(&parent),"artifacts":object(&artifacts),
        "backupPath":artifacts.join("original").to_str().unwrap()});
    let record = RecoveryRecord::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    (root, record)
}

#[test]
fn actual_identity_and_content_match_without_changing_files() {
    let (root, record) = fixture();
    assert_eq!(
        observe_replacement(&record),
        RecoveryAssessment::OriginalAtTarget
    );
    assert_eq!(fs::read(root.path().join("source")).unwrap(), b"abc");
    assert_eq!(
        fs::read(root.path().join("target/한글.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(
            root.path()
                .join("target/.projectpane-recovery-proof-1/staged")
        )
        .unwrap(),
        b"abc"
    );
    assert!(
        !root
            .path()
            .join("target/.projectpane-recovery-proof-1/original")
            .exists()
    );
    assert_eq!(
        fs::read_dir(root.path().join("target/.projectpane-recovery-proof-1"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn changed_same_size_content_missing_stage_and_writer_lock_stay_unknown() {
    for variant in ["same-size", "missing", "locked"] {
        let (root, record) = fixture();
        let target = root.path().join("target/한글.txt");
        let mut held = None;
        match variant {
            "same-size" => fs::write(&target, b"modified").unwrap(),
            "missing" => fs::remove_file(
                root.path()
                    .join("target/.projectpane-recovery-proof-1/staged"),
            )
            .unwrap(),
            "locked" => held = Some(OpenOptions::new().write(true).open(&target).unwrap()),
            _ => unreachable!(),
        }
        assert_eq!(
            observe_replacement(&record),
            RecoveryAssessment::Unknown,
            "{variant}"
        );
        drop(held);
        if variant == "locked" {
            assert_eq!(
                observe_replacement(&record),
                RecoveryAssessment::OriginalAtTarget
            );
        }
    }
}

#[test]
fn hardlink_and_over_budget_file_are_inaccessible() {
    for variant in ["hardlink", "budget"] {
        let (root, record) = fixture();
        let target = root.path().join("target/한글.txt");
        match variant {
            "hardlink" => fs::hard_link(&target, root.path().join("alias")).unwrap(),
            "budget" => OpenOptions::new()
                .write(true)
                .open(&target)
                .unwrap()
                .set_len(MAX_OBSERVATION_BYTES + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(
            observe_replacement(&record),
            RecoveryAssessment::Unknown,
            "{variant}"
        );
    }
}

#[test]
fn held_handles_block_writes_and_budget_is_shared() {
    let (root, _) = fixture();
    let target = root.path().join("target").join("한글.txt");
    let stage = root
        .path()
        .join("target")
        .join(".projectpane-recovery-proof-1")
        .join("staged");
    let mut collector = Collector {
        remaining: 8,
        held: Vec::new(),
    };
    assert!(matches!(
        collector.observe(target.to_str().unwrap()),
        FileObservation::Present { .. }
    ));
    assert_eq!(collector.remaining, 0);
    assert_eq!(
        collector.observe(stage.to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert!(OpenOptions::new().write(true).open(&target).is_err());
    drop(collector);
    assert!(OpenOptions::new().write(true).open(&target).is_ok());
}

#[test]
fn missing_leaf_and_missing_parent_have_different_evidence() {
    let root = tempfile::tempdir().unwrap();
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    assert_eq!(
        collector.observe(root.path().join("absent").to_str().unwrap()),
        FileObservation::Absent
    );
    assert_eq!(
        collector.observe(root.path().join("absent/leaf").to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn fingerprint_matches_independent_known_sha256_vectors() {
    let root = tempfile::tempdir().unwrap();
    for (bytes, expected) in [
        (
            b"".as_slice(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc".as_slice(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        let path = root.path().join("vector");
        fs::write(&path, bytes).unwrap();
        let content = fingerprint(
            &mut File::open(&path).unwrap(),
            u64::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            content
                .sha256
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            expected
        );
    }
}
