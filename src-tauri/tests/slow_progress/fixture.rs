use projectpane_core::domain::{BATCH_SIZE, Batch, Entry, Fault, MAX_FRAME, Request};
use std::io::{BufRead, Write};
use std::time::Duration;

pub const TOTAL: usize = 70 * BATCH_SIZE + 17;

pub fn entry(index: usize) -> Entry {
    Entry {
        size_bytes: None,
        id: format!("entry-{index}"),
        name: format!("{index:06}-한글.txt"),
        kind: projectpane_core::domain::EntryKind::File,
        modified_at_ms: None,
    }
}

pub fn run() {
    let mut total = 0;
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        let (offset, entries) = match request {
            Request::Open { .. } => {
                total = BATCH_SIZE;
                (0, (0..total).map(entry).collect())
            }
            Request::Progress => {
                // Deliberately slow each exchange, independent of filesystem speed.
                std::thread::sleep(Duration::from_millis(25));
                let offset = total;
                total = (total + BATCH_SIZE).min(TOTAL);
                (offset, Vec::new())
            }
            Request::Window { offset, limit } => {
                assert!(limit <= BATCH_SIZE);
                (
                    offset,
                    (offset..(offset + limit).min(total)).map(entry).collect(),
                )
            }
            Request::Advance
            | Request::SetView { .. }
            | Request::Locate { .. }
            | Request::Activate { .. }
            | Request::ActivateMany { .. } => panic!("unexpected fixture request"),
        };
        let batch: Result<Batch, Fault> = Ok(Batch {
            revision: 1,
            offset,
            entries,
            total,
            complete: total == TOTAL,
            capped: false,
            snapshot_bytes: total * 64,
            cap_reason: None,
        });
        let bytes = serde_json::to_vec(&batch).unwrap();
        assert!(bytes.len() < MAX_FRAME);
        output.write_all(&bytes).unwrap();
        output.write_all(b"\n").unwrap();
        output.flush().unwrap();
    }
}
