use projectpane_core::domain::FaultCode;
use projectpane_core::{
    domain::{BATCH_SIZE, Identity, Request},
    platform::worker::Listing,
    services::Broker,
};
use std::{fs, time::Duration};
use tempfile::tempdir;

fn identity(pane: usize, generation: u64) -> Identity {
    Identity {
        pane,
        tab_id: format!("tab-{pane}"),
        request_id: format!("request-{pane}-{generation}"),
        generation,
    }
}
fn broker() -> Broker {
    Broker::new(
        env!("CARGO_BIN_EXE_projectpane-read-worker").into(),
        Duration::from_secs(8),
    )
}

#[tokio::test]
async fn batches_windows_and_unicode_are_lossless_in_read_only_fixture() {
    let dir = tempdir().unwrap();
    for i in 0..600 {
        fs::write(
            dir.path().join(format!("한글 space 🧪 {i}.txt")),
            b"unchanged",
        )
        .unwrap();
    }
    fs::create_dir(dir.path().join("folder")).unwrap();
    let broker = broker();
    let id = identity(0, 1);
    let first = broker
        .start(id.clone(), dir.path().to_string_lossy().into())
        .await
        .unwrap();
    assert_eq!(first.batch.entries.len(), BATCH_SIZE);
    assert!(!first.batch.complete);
    loop {
        let result = broker.read(id.clone(), None).await.unwrap();
        assert!(result.batch.entries.len() <= BATCH_SIZE);
        if result.batch.complete {
            assert_eq!(result.batch.total, 601);
            break;
        }
    }
    let mut names = Vec::new();
    for offset in (0..601).step_by(BATCH_SIZE) {
        names.extend(
            broker
                .read(id.clone(), Some(offset))
                .await
                .unwrap()
                .batch
                .entries
                .into_iter()
                .map(|e| e.name),
        );
    }
    let window = broker.read(id.clone(), Some(0)).await.unwrap();
    assert_eq!(window.batch.entries[0].name, "folder");
    let anchor = broker
        .locate(id.clone(), first.batch.entries[0].id.clone())
        .await
        .unwrap();
    let anchored = broker.read(id.clone(), anchor.index).await.unwrap();
    assert_eq!(anchored.batch.entries[0].id, first.batch.entries[0].id);
    assert_eq!(names.len(), 601);
    assert!(names.contains(&"한글 space 🧪 599.txt".to_string()));
    broker.cancel(&id).await.unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 601);
    assert_eq!(
        fs::read(dir.path().join("한글 space 🧪 599.txt")).unwrap(),
        b"unchanged"
    );
}

#[tokio::test]
async fn replacement_rejects_old_results_and_old_cancellation_is_harmless() {
    let dir = tempdir().unwrap();
    let broker = broker();
    let a = identity(0, 1);
    let c = identity(0, 3);
    broker
        .start(a.clone(), dir.path().to_string_lossy().into())
        .await
        .unwrap();
    broker
        .start(c.clone(), dir.path().to_string_lossy().into())
        .await
        .unwrap();
    assert_eq!(
        broker.read(a.clone(), None).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    broker.cancel(&a).await.unwrap();
    assert!(
        broker
            .read(c.clone(), Some(0))
            .await
            .unwrap()
            .batch
            .complete
    );
    assert_eq!(
        broker
            .start(identity(0, 2), dir.path().to_string_lossy().into())
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    broker.cancel(&c).await.unwrap();
}

#[tokio::test]
async fn errors_are_not_empty_folders_and_worker_capacity_is_four() {
    let dir = tempdir().unwrap();
    let broker = broker();
    assert_eq!(
        broker
            .start(
                identity(0, 1),
                dir.path().join("missing").to_string_lossy().into()
            )
            .await
            .unwrap_err()
            .code,
        FaultCode::NotFound
    );
    assert_eq!(
        broker
            .start(identity(4, 1), dir.path().to_string_lossy().into())
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    assert_eq!(
        broker
            .start(identity(0, 2), "relative".into())
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidPath
    );
    for pane in 0..4 {
        let result = broker
            .start(identity(pane, 3), dir.path().to_string_lossy().into())
            .await
            .unwrap();
        assert!(result.batch.complete);
        assert_eq!(result.batch.total, 0);
    }
    for pane in 0..4 {
        broker.cancel(&identity(pane, 3)).await.unwrap();
    }
}

#[test]
fn invalid_window_limits_are_rejected() {
    let mut listing = Listing::default();
    assert_eq!(
        listing
            .handle(Request::Window {
                offset: 0,
                limit: BATCH_SIZE + 1
            })
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
}

#[test]
fn long_root_lists_100k_and_overflow_is_explicit() {
    use projectpane_core::domain::MAX_ENTRIES;
    let dir = tempdir().unwrap();
    // Above the 93-byte root that previously stopped at 81,628 entries, while
    // keeping individual Windows paths below MAX_PATH for this regression.
    let root = dir.path().join("long-root-".repeat(10));
    fs::create_dir(&root).unwrap();
    for i in 0..MAX_ENTRIES {
        let path = root.join(format!("{i:06}-한글.txt"));
        if i % 10 == 0 {
            fs::create_dir(path).unwrap();
        } else {
            fs::File::create(path).unwrap();
        }
    }
    let mut listing = Listing::default();
    for overflow in [false, true] {
        if overflow {
            fs::File::create(root.join("overflow.txt")).unwrap();
        }
        let mut batch = listing
            .handle(Request::Open {
                view: Default::default(),
                path: root.to_string_lossy().into(),
            })
            .unwrap();
        let mut names = std::collections::HashSet::new();
        let mut folders = 0;
        while !batch.complete {
            assert!(batch.entries.len() <= BATCH_SIZE);
            batch = listing.handle(Request::Advance).unwrap();
        }
        for offset in (0..MAX_ENTRIES).step_by(BATCH_SIZE) {
            let page = listing
                .handle(Request::Window {
                    offset,
                    limit: BATCH_SIZE,
                })
                .unwrap();
            for e in &page.entries {
                assert!(names.insert(e.name.clone()));
                folders += usize::from(e.kind == projectpane_core::domain::EntryKind::Folder);
            }
        }
        assert_eq!(batch.total, MAX_ENTRIES);
        assert_eq!(names.len(), MAX_ENTRIES);
        assert_eq!(batch.capped, overflow);
        assert_eq!(
            batch.cap_reason.as_deref(),
            overflow.then_some("entry_limit")
        );
        assert!(batch.snapshot_bytes < 10 * 1024 * 1024);
        if !overflow {
            assert_eq!(folders, 10_000);
            assert!(names.contains("099999-한글.txt"));
        }
        let end = listing
            .handle(Request::Window {
                offset: 99_840,
                limit: BATCH_SIZE,
            })
            .unwrap();
        assert_eq!(end.entries.len(), 160);
        assert_eq!(
            listing.locate(&end.entries[159].id).unwrap().index,
            Some(99_999)
        );
    }
}

#[cfg(unix)]
#[path = "listing/unix.rs"]
mod unix;
