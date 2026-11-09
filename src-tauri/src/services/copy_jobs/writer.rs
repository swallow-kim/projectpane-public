//! One durable intent at a time. Results share the next intent's barrier.
use super::{Event, Snapshot, storage::record_bytes};
use crate::domain::{Fault, FaultCode};
use std::{
    fs::File,
    io::{self, Seek, Write},
    time::{Duration, Instant},
};

pub(super) trait JournalIo: Write + Seek {
    fn sync(&mut self) -> io::Result<()>;
}
impl JournalIo for &mut File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}
pub(super) struct JobLog<W: JournalIo> {
    io: W,
    durable_end: u64,
    records: usize,
    last_sync: Instant,
    poisoned: bool,
    legacy: bool,
    syncs: u64,
}
impl<W: JournalIo> JobLog<W> {
    pub fn new(mut io: W, legacy: bool) -> Result<Self, Fault> {
        let durable_end = io.stream_position().map_err(Fault::from)?;
        Ok(Self {
            io,
            durable_end,
            records: 0,
            last_sync: Instant::now(),
            poisoned: false,
            legacy,
            syncs: 0,
        })
    }
    pub fn durable_end(&self) -> u64 {
        self.durable_end
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned
    }
    #[cfg(test)]
    pub fn syncs(&self) -> u64 {
        self.syncs
    }
    fn append(&mut self, event: &Event) -> Result<(), Fault> {
        if self.poisoned {
            return Err(Fault::new(FaultCode::IoError));
        }
        let result = (|| {
            let bytes = record_bytes(event)?;
            self.io.write_all(&bytes).map_err(Fault::from)?;
            self.records += 1;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn checkpoint(&mut self) -> Result<(), Fault> {
        if self.poisoned {
            return Err(Fault::new(FaultCode::IoError));
        }
        let result = (|| {
            self.io.sync().map_err(Fault::from)?;
            self.syncs += 1;
            let end = self.io.stream_position().map_err(Fault::from)?;
            self.durable_end = end;
            self.records = 0;
            self.last_sync = Instant::now();
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn intent(&mut self, source: &str, destination: &str) -> Result<(), Fault> {
        self.append(&Event::Intent {
            source: source.into(),
            destination: destination.into(),
        })?;
        self.checkpoint()
    }
    /// Returns true only when this result is covered by a successful barrier.
    pub fn result(&mut self, event: &Event) -> Result<bool, Fault> {
        self.append(event)?;
        let position = match self.io.stream_position() {
            Ok(position) => position,
            Err(error) => {
                self.poisoned = true;
                return Err(error.into());
            }
        };
        if self.legacy
            || self.records >= 64
            || position.saturating_sub(self.durable_end) >= 65536
            || self.last_sync.elapsed() >= Duration::from_millis(250)
        {
            self.checkpoint()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn finish(&mut self, snapshot: &Snapshot) -> Result<(), Fault> {
        self.append(&Event::Finished {
            snapshot: snapshot.clone(),
        })?;
        self.checkpoint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::operations::{ItemOutcome, ItemStatus};
    use std::io::{Cursor, Read, SeekFrom};
    #[derive(Default)]
    struct Fake {
        bytes: Cursor<Vec<u8>>,
        synced: Vec<u8>,
        writes: usize,
        syncs: usize,
        fail_write: Option<usize>,
        fail_sync: Option<usize>,
    }
    impl Write for Fake {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            if self.fail_write == Some(self.writes) {
                self.bytes.write_all(&bytes[..bytes.len() / 2])?;
                return Err(io::Error::other("torn"));
            }
            self.bytes.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Seek for Fake {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.bytes.seek(pos)
        }
    }
    impl JournalIo for Fake {
        fn sync(&mut self) -> io::Result<()> {
            self.syncs += 1;
            if self.fail_sync == Some(self.syncs) {
                return Err(io::Error::other("sync"));
            }
            self.synced = self.bytes.get_ref().clone();
            Ok(())
        }
    }
    fn outcome() -> Event {
        Event::Outcome {
            item: ItemOutcome {
                source: "source".into(),
                destination: Some("target".into()),
                status: ItemStatus::Changed,
                hresult: 0,
            },
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            id: "test".into(),
            project_id: None,
            status: super::super::CopyStatus::Succeeded,
            counts: super::super::Counts {
                changed: 1,
                ..Default::default()
            },
            roots_total: 1,
            roots_completed: 1,
            outcome_count: 1,
            journal_saved: true,
            failure: None,
        }
    }
    #[test]
    fn results_join_next_intent_barrier_and_final_completion() {
        let mut log = JobLog::new(Fake::default(), false).unwrap();
        log.intent("source", "target").unwrap();
        let first = log.durable_end();
        assert!(!log.result(&outcome()).unwrap());
        assert!(!log.result(&Event::RootDone).unwrap());
        assert_eq!(log.durable_end(), first);
        assert_eq!(log.io.synced.len() as u64, first);
        log.intent("next", "next-target").unwrap();
        assert!(log.durable_end() > first);
        assert_eq!(log.syncs(), 2);
        assert!(String::from_utf8_lossy(&log.io.synced).contains("root_done"));
        log.finish(&snapshot()).unwrap();
        assert_eq!(log.syncs(), 3);
        assert_eq!(log.io.synced, log.io.bytes.into_inner());
    }
    #[test]
    fn torn_results_rootdone_and_finished_poison_without_continuation() {
        for failing in 2..=4 {
            let fake = Fake {
                fail_write: Some(failing),
                ..Default::default()
            };
            let mut log = JobLog::new(fake, false).unwrap();
            log.intent("source", "target").unwrap();
            let end = log.durable_end();
            let _ = log.result(&outcome());
            let _ = log.result(&Event::RootDone);
            let _ = log.finish(&snapshot());
            assert!(log.poisoned());
            assert_eq!(log.durable_end(), end);
            let writes = log.io.writes;
            assert!(log.intent("forbidden", "forbidden").is_err());
            assert!(log.finish(&snapshot()).is_err());
            assert_eq!(log.io.writes, writes);
        }
    }
    #[test]
    fn failed_barrier_never_advances_watermark_or_allows_mutation() {
        for fail in [1, 2] {
            let fake = Fake {
                fail_sync: Some(fail),
                ..Default::default()
            };
            let mut log = JobLog::new(fake, false).unwrap();
            let first = log.intent("source", "target");
            if fail == 1 {
                assert!(first.is_err());
                assert_eq!(log.durable_end(), 0);
            } else {
                first.unwrap();
                let end = log.durable_end();
                log.result(&outcome()).unwrap();
                assert!(log.intent("next", "next-target").is_err());
                assert_eq!(log.durable_end(), end);
            }
            assert!(log.poisoned());
            let writes = log.io.writes;
            assert!(log.finish(&snapshot()).is_err());
            assert_eq!(writes, log.io.writes);
        }
    }
    #[test]
    fn outcome_only_runs_checkpoint_by_count_and_elapsed_time() {
        let mut log = JobLog::new(Fake::default(), false).unwrap();
        for _ in 0..63 {
            assert!(!log.result(&outcome()).unwrap());
        }
        assert!(log.result(&outcome()).unwrap());
        assert_eq!(log.syncs(), 1);
        log.last_sync = Instant::now() - Duration::from_millis(300);
        assert!(log.result(&outcome()).unwrap());
        assert_eq!(log.syncs(), 2);
    }
    #[test]
    fn cancellation_finishes_staged_results_with_one_final_barrier() {
        let mut log = JobLog::new(Fake::default(), false).unwrap();
        log.intent("source", "target").unwrap();
        log.result(&outcome()).unwrap();
        let mut end = snapshot();
        end.status = super::super::CopyStatus::Cancelled;
        end.roots_completed = 0;
        log.finish(&end).unwrap();
        assert_eq!(log.syncs(), 2);
        assert_eq!(log.durable_end(), log.io.synced.len() as u64);
        let mut cursor = Cursor::new(log.io.synced);
        let mut text = String::new();
        cursor.read_to_string(&mut text).unwrap();
        assert!(text.contains("cancelled"));
    }
}
