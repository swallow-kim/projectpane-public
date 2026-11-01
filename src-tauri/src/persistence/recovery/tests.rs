use super::{MAX_RECOVERY_JOURNAL_BYTES, files, read_journal};
use crate::domain::{
    MAX_FRAME,
    operations::{MAX_OPERATION_HISTORY, recovery::RecoveryRecord},
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io,
    path::Path,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/recovery-ready.json");
const PHASES: [&str; 5] = [
    "ready",
    "preserve_requested",
    "original_preserved",
    "install_requested",
    "install_observed",
];

pub(super) fn record(id: &str, phase: &str) -> Value {
    let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
    value["id"] = json!(id);
    value["phase"] = json!(phase);
    let directory = format!("D:\\target\\.projectpane-recovery-{id}");
    value["artifacts"]["path"] = json!(directory);
    value["staged"]["object"]["path"] = json!(format!("{directory}\\staged"));
    value["backupPath"] = json!(format!("{directory}\\original"));
    value
}

fn write_record(root: &Path, value: &Value) {
    let name = format!(
        "{}.{}.json",
        value["id"].as_str().unwrap(),
        value["phase"].as_str().unwrap()
    );
    fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

fn directory() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    File::create(root.path().join("journal.lock")).unwrap();
    root
}

#[test]
fn loads_all_contiguous_prefixes_and_preserves_input_bytes() {
    for length in 1..=5 {
        let root = directory();
        for phase in PHASES.iter().take(length) {
            write_record(root.path(), &record("proof-1", phase));
        }
        let before: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let records = read_journal(root.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].phase().file_label(), PHASES[length - 1]);
        let roundtrip = RecoveryRecord::parse(&records[0].to_bytes().unwrap()).unwrap();
        assert_eq!(roundtrip.id(), "proof-1");
        for (path, bytes) in before {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}

#[test]
fn refuses_phase_gaps_partial_json_and_mismatched_binding_without_repair() {
    for variant in ["gap", "first", "partial", "binding", "filename"] {
        let root = directory();
        if variant != "first" {
            write_record(root.path(), &record("proof-1", "ready"));
        }
        let phase = if variant == "gap" {
            "install_requested"
        } else {
            "preserve_requested"
        };
        let mut value = record("proof-1", phase);
        if variant == "binding" {
            value["original"]["content"]["bytes"] = json!(999);
        }
        write_record(root.path(), &value);
        let path = root.path().join(format!("proof-1.{phase}.json"));
        if variant == "partial" {
            fs::write(&path, b"{broken").unwrap();
        }
        if variant == "filename" {
            fs::write(&path, serde_json::to_vec(&record("other", phase)).unwrap()).unwrap();
        }
        let before = fs::read(&path).unwrap();
        assert_eq!(
            read_journal(root.path()).unwrap_err().kind(),
            io::ErrorKind::InvalidData,
            "{variant}"
        );
        assert_eq!(fs::read(path).unwrap(), before);
    }
}

#[test]
fn requires_existing_directory_and_lock_without_creating_them() {
    let parent = tempfile::tempdir().unwrap();
    let absent = parent.path().join("absent");
    assert!(read_journal(&absent).is_err());
    assert!(!absent.exists());
    assert!(read_journal(parent.path()).is_err());
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn exclusive_writer_lock_blocks_reader_then_releases() {
    let root = directory();
    let writer = File::open(root.path().join("journal.lock")).unwrap();
    writer.try_lock().unwrap();
    assert_eq!(
        read_journal(root.path()).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(writer);
    assert!(read_journal(root.path()).unwrap().is_empty());
}

#[test]
fn unknown_entries_case_aliases_and_non_files_fail_closed() {
    for variant in ["extra", "directory", "case"] {
        let root = directory();
        write_record(root.path(), &record("proof-1", "ready"));
        match variant {
            "extra" => fs::write(root.path().join("unexpected.tmp"), b"retain").unwrap(),
            "directory" => {
                fs::create_dir(root.path().join("proof-1.install_requested.json")).unwrap()
            }
            "case" => write_record(root.path(), &record("PROOF-1", "preserve_requested")),
            _ => unreachable!(),
        }
        assert!(read_journal(root.path()).is_err(), "{variant}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
    }
}

#[test]
fn bounds_history_per_frame_and_total_raw_input() {
    let root = directory();
    for index in 0..MAX_OPERATION_HISTORY {
        write_record(root.path(), &record(&format!("id-{index}"), "ready"));
    }
    assert_eq!(
        read_journal(root.path()).unwrap().len(),
        MAX_OPERATION_HISTORY
    );
    write_record(root.path(), &record("overflow", "ready"));
    assert!(read_journal(root.path()).is_err());
    let huge = directory();
    fs::write(
        huge.path().join("proof-1.ready.json"),
        vec![b' '; MAX_FRAME],
    )
    .unwrap();
    assert!(read_journal(huge.path()).is_err());
    let bounded = directory();
    write_record(bounded.path(), &record("proof-1", "ready"));
    let path = bounded.path().join("proof-1.ready.json");
    let length = fs::metadata(&path).unwrap().len();
    let mut total = MAX_RECOVERY_JOURNAL_BYTES - length;
    assert!(files::read(&path, &mut total).is_ok());
    assert_eq!(total, MAX_RECOVERY_JOURNAL_BYTES);
    assert!(files::read(&path, &mut total).is_err());
}

#[test]
fn aggregate_disk_input_limit_is_enforced_across_records() {
    let root = directory();
    for index in 0..16 {
        let mut bytes = serde_json::to_vec(&record(&format!("id-{index}"), "ready")).unwrap();
        bytes.resize(MAX_FRAME - 1, b' ');
        fs::write(root.path().join(format!("id-{index}.ready.json")), bytes).unwrap();
    }
    assert_eq!(read_journal(root.path()).unwrap().len(), 16);
    write_record(root.path(), &record("overflow", "ready"));
    assert_eq!(
        read_journal(root.path()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 18);
}
