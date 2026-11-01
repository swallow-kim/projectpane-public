use projectpane_core::domain::FaultCode;
use projectpane_core::{domain::Identity, services::Broker};
use std::{collections::HashSet, fs, time::Duration};

#[tokio::test]
async fn display_pages_keep_omitted_rows_accessible_and_legacy_batches_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let expected: HashSet<_> = (0..1001).map(|i| format!("{i:06}-한글.txt")).collect();
    for name in &expected {
        fs::File::create(directory.path().join(name)).unwrap();
    }
    let broker = Broker::new(
        env!("CARGO_BIN_EXE_projectpane-read-worker").into(),
        Duration::from_secs(8),
    );
    let identity = Identity {
        pane: 0,
        tab_id: "display".into(),
        request_id: "display-1".into(),
        generation: 1,
    };
    let first = broker
        .start_visible(identity.clone(), directory.path().to_string_lossy().into())
        .await
        .unwrap();
    assert_eq!(first.batch.entries.len(), 64);
    assert_eq!(first.batch.total, 256);
    assert!(!first.batch.complete);
    let mut complete = broker.progress(identity.clone()).await.unwrap();
    while !complete.batch.complete {
        let previous = complete.batch.total;
        complete = broker.progress(identity.clone()).await.unwrap();
        assert!(complete.batch.total > previous);
        assert!(complete.batch.total <= 1001);
    }
    assert_eq!(complete.batch.total, 1001);
    assert!(!complete.batch.capped);
    let mut received = HashSet::new();
    for offset in (0..1001).step_by(64) {
        let reply = broker.read_visible(identity.clone(), offset).await.unwrap();
        assert_eq!(reply.identity, identity);
        assert_eq!(reply.batch.offset, offset);
        assert_eq!(reply.batch.entries.len(), 64.min(1001 - offset));
        for entry in reply.batch.entries {
            assert_eq!(entry.kind, projectpane_core::domain::EntryKind::File);
            assert!(received.insert(entry.name));
        }
    }
    assert_eq!(received, expected);
    assert_eq!(
        broker
            .read(identity.clone(), Some(0))
            .await
            .unwrap()
            .batch
            .entries
            .len(),
        256
    );
    broker.cancel(&identity).await.unwrap();
    assert_eq!(
        broker.read_visible(identity, 0).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
}
