use super::*;
use crate::persistence::recovery::{read_journal, tests::record};
use std::fs;

struct FailingFile<'a> {
    file: &'a mut File,
    fail: &'static str,
    calls: &'a mut Vec<&'static str>,
}

impl Write for FailingFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls.push("write");
        if self.fail == "write" {
            return Err(io::Error::other("injected_write"));
        }
        self.file.write_all(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.calls.push("flush");
        if self.fail == "flush" {
            return Err(io::Error::other("injected_flush"));
        }
        self.file.flush()
    }
}

impl SyncWrite for FailingFile<'_> {
    fn sync(&mut self) -> io::Result<()> {
        self.calls.push("sync");
        if self.fail == "sync" {
            return Err(io::Error::other("injected_sync"));
        }
        self.file.sync_all()
    }
}

#[test]
fn production_persistence_sequence_propagates_each_operation_failure() {
    let record =
        RecoveryRecord::parse(&serde_json::to_vec(&record("proof-1", "ready")).unwrap()).unwrap();
    for (fail, expected) in [
        ("write", vec!["write"]),
        ("flush", vec!["write", "flush"]),
        ("sync", vec!["write", "flush", "sync"]),
        ("none", vec!["write", "flush", "sync"]),
    ] {
        let root = tempfile::tempdir().unwrap();
        File::create(root.path().join("journal.lock")).unwrap();
        let mut calls = Vec::new();
        let result = append_with(root.path(), &record, |file, bytes| {
            persist(
                &mut FailingFile {
                    file,
                    fail,
                    calls: &mut calls,
                },
                bytes,
            )
        });
        assert_eq!(calls, expected, "{fail}");
        if fail == "none" {
            assert!(result.is_ok());
        } else {
            assert!(
                matches!(
                    result,
                    Err(AppendFailure::Unacknowledged(io::ErrorKind::Other))
                ),
                "{fail}"
            );
        }
        let bytes = fs::read(root.path().join("proof-1.ready.json")).unwrap();
        if fail == "write" {
            assert!(bytes.is_empty());
            assert!(read_journal(root.path()).is_err());
        } else {
            assert_eq!(bytes, record.to_bytes().unwrap());
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
