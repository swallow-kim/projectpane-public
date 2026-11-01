use super::*;
use crate::persistence::recovery::{read_journal, tests::record};
use std::fs;

fn phase(id: &str, name: &str) -> RecoveryRecord {
    RecoveryRecord::parse(&serde_json::to_vec(&record(id, name)).unwrap()).unwrap()
}

fn directory() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    File::create(root.path().join("journal.lock")).unwrap();
    root
}

fn inventory(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut result: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect();
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result
}

#[test]
fn contiguous_chain_acknowledges_only_new_synced_files_and_reopens() {
    let root = directory();
    for name in [
        "ready",
        "preserve_requested",
        "original_preserved",
        "install_requested",
        "install_observed",
    ] {
        let record = phase("proof-1", name);
        let before = inventory(root.path());
        let ack = append_phase(root.path(), &record).unwrap();
        assert_eq!(ack.id, "proof-1");
        assert_eq!(ack.phase.file_label(), name);
        for (path, bytes) in before {
            assert_eq!(fs::read(root.path().join(path)).unwrap(), bytes);
        }
        let loaded = read_journal(root.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].to_bytes().unwrap(), record.to_bytes().unwrap());
    }
    assert_eq!(inventory(root.path()).len(), 6);
}

#[test]
fn rejects_duplicate_gap_case_and_changed_binding_without_creation() {
    for variant in ["duplicate", "gap", "case", "binding"] {
        let root = directory();
        let _ack = append_phase(root.path(), &phase("proof-1", "ready")).unwrap();
        let mut value = record(
            if variant == "case" {
                "PROOF-1"
            } else {
                "proof-1"
            },
            match variant {
                "duplicate" => "ready",
                "gap" => "install_observed",
                _ => "preserve_requested",
            },
        );
        if variant == "binding" {
            value["original"]["content"]["bytes"] = serde_json::json!(9);
        }
        let request = RecoveryRecord::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let before = inventory(root.path());
        assert!(
            matches!(
                append_phase(root.path(), &request),
                Err(AppendFailure::NotCreated(_))
            ),
            "{variant}"
        );
        assert_eq!(inventory(root.path()), before);
    }
}

#[test]
fn existing_shared_and_exclusive_locks_block_before_creation() {
    for shared in [false, true] {
        let root = directory();
        let lock = File::open(root.path().join("journal.lock")).unwrap();
        if shared {
            lock.try_lock_shared().unwrap();
        } else {
            lock.try_lock().unwrap();
        }
        assert!(matches!(
            append_phase(root.path(), &phase("proof-1", "ready")),
            Err(AppendFailure::NotCreated(io::ErrorKind::WouldBlock))
        ));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        drop(lock);
        let _ack = append_phase(root.path(), &phase("proof-1", "ready")).unwrap();
    }
}

#[test]
fn post_creation_failures_never_report_not_created_or_delete_evidence() {
    for step in ["create", "partial", "write", "flush", "sync"] {
        let root = directory();
        let result = append_with(root.path(), &phase("proof-1", "ready"), |file, bytes| {
            match step {
                "create" => {}
                "partial" => {
                    file.write_all(&bytes[..7])?;
                }
                "write" => {
                    file.write_all(bytes)?;
                }
                "flush" => {
                    file.write_all(bytes)?;
                    file.flush()?;
                }
                "sync" => {
                    persist(file, bytes)?;
                }
                _ => unreachable!(),
            }
            Err(io::Error::other("injected_failure"))
        });
        assert!(matches!(
            result,
            Err(AppendFailure::Unacknowledged(io::ErrorKind::Other))
        ));
        let path = root.path().join("proof-1.ready.json");
        assert!(path.exists());
        if matches!(step, "create" | "partial") {
            let before = inventory(root.path());
            assert!(read_journal(root.path()).is_err());
            assert!(matches!(
                append_phase(root.path(), &phase("proof-1", "preserve_requested")),
                Err(AppendFailure::NotCreated(_))
            ));
            assert_eq!(inventory(root.path()), before);
        } else {
            // A complete surviving claim is readable, but no acknowledgement was returned.
            assert_eq!(read_journal(root.path()).unwrap().len(), 1);
        }
    }
}

#[test]
fn existing_records_and_ancestors_remain_guarded_through_write() {
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_FLAG_BACKUP_SEMANTICS};
    let root = directory();
    let _ack = append_phase(root.path(), &phase("proof-1", "ready")).unwrap();
    let prior = root.path().join("proof-1.ready.json");
    let _ack = append_with(
        root.path(),
        &phase("proof-1", "preserve_requested"),
        |file, bytes| {
            assert!(OpenOptions::new().write(true).open(&prior).is_err());
            assert!(
                OpenOptions::new()
                    .access_mode(DELETE)
                    .share_mode(7)
                    .open(&prior)
                    .is_err()
            );
            assert!(
                OpenOptions::new()
                    .access_mode(DELETE)
                    .share_mode(7)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                    .open(root.path())
                    .is_err()
            );
            assert!(read_journal(root.path()).is_err());
            persist(file, bytes)
        },
    )
    .unwrap();
    assert!(OpenOptions::new().write(true).open(&prior).is_ok());
}

#[test]
fn missing_directory_or_lock_never_get_initialized() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent");
    assert!(matches!(
        append_phase(&absent, &phase("proof-1", "ready")),
        Err(AppendFailure::NotCreated(_))
    ));
    assert!(!absent.exists());
    assert!(matches!(
        append_phase(root.path(), &phase("proof-1", "ready")),
        Err(AppendFailure::NotCreated(_))
    ));
    assert_eq!(inventory(root.path()).len(), 0);
}
