use projectpane_core::domain::FaultCode;
use projectpane_core::{
    domain::{BATCH_SIZE, Identity, Request},
    platform::worker::Listing,
    services::Broker,
};
use std::{collections::HashSet, fs};
use tempfile::tempdir;

#[test]
fn progress_omits_discarded_entries_but_preserves_all_window_data() {
    let directory = tempdir().unwrap();
    let expected: HashSet<_> = (0..1025)
        .map(|index| format!("{index:06}-한글.txt"))
        .collect();
    for name in &expected {
        fs::File::create(directory.path().join(name)).unwrap();
    }
    let mut listing = Listing::default();
    let first = listing
        .handle(Request::Open {
            view: Default::default(),
            path: directory.path().to_string_lossy().into_owned(),
        })
        .unwrap();
    assert_eq!(first.entries.len(), BATCH_SIZE);
    let mut total = first.total;
    loop {
        let progress: Request = serde_json::from_str(r#"{"kind":"progress"}"#).unwrap();
        let batch = listing.handle(progress).unwrap();
        assert!(batch.entries.is_empty());
        assert_eq!(batch.offset, if batch.complete { 0 } else { total });
        assert_eq!(batch.revision, first.revision + u64::from(batch.complete));
        assert!(batch.total >= total && batch.total - total <= BATCH_SIZE);
        assert!(!batch.capped);
        total = batch.total;
        if batch.complete {
            break;
        }
    }
    assert_eq!(total, expected.len());
    let mut actual = HashSet::new();
    for offset in (0..total).step_by(BATCH_SIZE) {
        let batch = listing
            .handle(Request::Window {
                offset,
                limit: BATCH_SIZE,
            })
            .unwrap();
        assert!(batch.complete);
        assert_eq!(batch.revision, first.revision + 1);
        for entry in batch.entries {
            assert_eq!(entry.kind, projectpane_core::domain::EntryKind::File);
            assert!(actual.insert(entry.name));
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), total);
}

#[tokio::test]
async fn progress_uses_real_worker_and_obeys_cancellation() {
    real_worker_progress(5000).await;
}

#[tokio::test]
async fn progress_at_batch_boundary_rebuilds_view_without_losing_entries() {
    real_worker_progress(BATCH_SIZE).await;
}

async fn real_worker_progress(entry_count: usize) {
    let directory = tempdir().unwrap();
    for index in 0..entry_count {
        fs::File::create(directory.path().join(format!("{index:06}-한글.txt"))).unwrap();
    }
    let broker = Broker::new(
        env!("CARGO_BIN_EXE_projectpane-read-worker").into(),
        std::time::Duration::from_secs(8),
    );
    let identity = Identity {
        pane: 0,
        tab_id: "progress".into(),
        request_id: "progress-1".into(),
        generation: 1,
    };
    let first = broker
        .start(
            identity.clone(),
            directory.path().to_string_lossy().into_owned(),
        )
        .await
        .unwrap();
    assert_eq!(first.batch.entries.len(), BATCH_SIZE);
    assert_eq!(first.batch.revision, 1);
    assert!(!first.batch.complete);
    let mut total = first.batch.total;
    let mut revision = first.batch.revision;
    loop {
        let reply = broker.progress(identity.clone()).await.unwrap();
        assert_eq!(reply.identity, identity);
        assert!(reply.batch.entries.is_empty());
        if reply.batch.complete {
            assert_eq!(reply.batch.revision, revision + 1);
            // EOF rebuilds the view at offset zero. Coalescing preserves the
            // first exchange's offset when that exchange precedes EOF.
            let expected_offset = if entry_count - total < BATCH_SIZE {
                0
            } else {
                total
            };
            assert_eq!(reply.batch.offset, expected_offset);
        } else {
            assert_eq!(reply.batch.revision, revision);
            assert_eq!(reply.batch.offset, total);
            assert!(reply.batch.total > total);
        }
        assert!(reply.batch.total >= total);
        assert!(reply.batch.total <= entry_count);
        assert!(!reply.batch.capped);
        total = reply.batch.total;
        revision = reply.batch.revision;
        if reply.batch.complete {
            break;
        }
    }
    assert_eq!(total, entry_count);
    let mut names = HashSet::new();
    for offset in (0..total).step_by(BATCH_SIZE) {
        let reply = broker.read(identity.clone(), Some(offset)).await.unwrap();
        assert_eq!(reply.identity, identity);
        assert_eq!(reply.batch.offset, offset);
        assert_eq!(reply.batch.total, total);
        assert_eq!(reply.batch.revision, revision);
        assert!(reply.batch.complete);
        assert_eq!(reply.batch.entries.len(), (total - offset).min(BATCH_SIZE));
        for (index, entry) in reply.batch.entries.iter().enumerate() {
            assert_eq!(entry.name, format!("{:06}-한글.txt", offset + index));
            assert_eq!(
                broker
                    .locate(identity.clone(), entry.id.clone())
                    .await
                    .unwrap()
                    .index,
                Some(offset + index)
            );
            assert_eq!(entry.kind, projectpane_core::domain::EntryKind::File);
            assert!(names.insert(entry.name.clone()));
        }
    }
    assert_eq!(
        names,
        (0..entry_count)
            .map(|index| format!("{index:06}-한글.txt"))
            .collect()
    );
    let last = broker
        .read(identity.clone(), Some(entry_count - 1))
        .await
        .unwrap();
    assert_eq!(last.batch.entries.len(), 1);
    assert_eq!(
        last.batch.entries[0].name,
        format!("{:06}-한글.txt", entry_count - 1)
    );
    broker.cancel(&identity).await.unwrap();
    assert_eq!(
        broker.progress(identity).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
}
