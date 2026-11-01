use projectpane_core::{domain::*, platform::worker::Listing, services::Broker};
use std::{collections::HashMap, fs, time::Duration};

fn view(sort: SortColumn, direction: SortDirection) -> ListingView {
    ListingView {
        sort,
        direction,
        ..ListingView::default()
    }
}
fn open(listing: &mut Listing, root: &std::path::Path) -> Batch {
    listing
        .handle(Request::Open {
            path: root.to_str().unwrap().into(),
            view: ListingView::default(),
        })
        .unwrap()
}
fn finish(listing: &mut Listing, mut batch: Batch) -> Batch {
    while !batch.complete {
        batch = listing.handle(Request::Progress).unwrap();
    }
    batch
}
fn rows(listing: &mut Listing, total: usize) -> Vec<Entry> {
    let mut entries = Vec::new();
    while entries.len() < total {
        entries.extend(
            listing
                .handle(Request::Window {
                    offset: entries.len(),
                    limit: BATCH_SIZE,
                })
                .unwrap()
                .entries,
        );
    }
    entries
}

#[test]
fn full_snapshot_sort_filter_and_stable_identity_across_reload() {
    let root = tempfile::tempdir().unwrap();
    for i in (0..601).rev() {
        fs::write(root.path().join(format!("File{i}.txt")), vec![0; i]).unwrap();
    }
    fs::create_dir(root.path().join("folder")).unwrap();
    let mut listing = Listing::default();
    let first = open(&mut listing, root.path());
    assert_eq!(first.revision, 1);
    assert!(!first.complete);
    let final_batch = finish(&mut listing, first);
    assert_eq!(final_batch.revision, 2);
    assert_eq!(listing.handle(Request::Progress).unwrap().revision, 2);
    let original = rows(&mut listing, 602);
    assert_eq!(original[0].kind, EntryKind::Folder);
    for (index, entry) in original[1..].iter().enumerate() {
        assert_eq!(entry.name, format!("File{index}.txt"));
        assert_eq!(entry.size_bytes, Some(u64::try_from(index).unwrap()));
    }
    assert_eq!(original[0].size_bytes, None);
    let ids: HashMap<_, _> = original
        .iter()
        .map(|e| (e.name.clone(), e.id.clone()))
        .collect();
    let reversed = listing
        .handle(Request::SetView {
            view: view(SortColumn::Size, SortDirection::Desc),
        })
        .unwrap();
    assert_eq!(reversed.revision, 3);
    assert_eq!(reversed.entries[0].kind, EntryKind::Folder);
    assert_eq!(reversed.entries[1].name, "File600.txt");
    let anchor = listing.locate(&ids["File0.txt"]).unwrap();
    assert_eq!(anchor.index, Some(601));
    assert_eq!(anchor.revision, 3);
    let filtered = listing
        .handle(Request::SetView {
            view: ListingView {
                filter: "fIlE60".into(),
                ..view(SortColumn::Name, SortDirection::Asc)
            },
        })
        .unwrap();
    assert_eq!(filtered.total, 2);
    assert_eq!(
        filtered
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["File60.txt", "File600.txt"]
    );
    assert_eq!(listing.locate(&ids["File0.txt"]).unwrap().index, None);
    for entry in filtered.entries {
        assert_eq!(entry.id, ids[&entry.name]);
    }
    let first = open(&mut listing, root.path());
    finish(&mut listing, first);
    for entry in rows(&mut listing, 602) {
        assert_eq!(entry.id, ids[&entry.name]);
    }
}

#[test]
fn loading_appends_without_revision_churn_and_final_rebuilds_once() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..800 {
        fs::write(root.path().join(format!("entry{i}.txt")), []).unwrap();
    }
    let mut listing = Listing::default();
    let first = open(&mut listing, root.path());
    let selected = first.entries[0].id.clone();
    let changed = listing
        .handle(Request::SetView {
            view: ListingView {
                filter: "ENTRY".into(),
                ..view(SortColumn::Name, SortDirection::Desc)
            },
        })
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert!(!changed.complete);
    let mut batch = listing.handle(Request::Progress).unwrap();
    assert_eq!(batch.revision, 2);
    assert_eq!(batch.offset, 256);
    while !batch.complete {
        batch = listing.handle(Request::Progress).unwrap();
    }
    assert_eq!(batch.revision, 3);
    assert_eq!(batch.offset, 0);
    assert_eq!(batch.total, 800);
    assert_eq!(listing.locate(&selected).unwrap().revision, 3);
    assert!(listing.locate(&selected).unwrap().index.is_some());
    assert_eq!(listing.handle(Request::Progress).unwrap().revision, 3);
    assert_eq!(rows(&mut listing, 800)[0].name, "entry799.txt");
}

#[tokio::test]
async fn native_worker_commands_reject_stale_unknown_and_removed_activation() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file.txt"), b"untouched").unwrap();
    fs::create_dir(root.path().join("folder")).unwrap();
    let broker = Broker::new(
        env!("CARGO_BIN_EXE_projectpane-read-worker").into(),
        Duration::from_secs(8),
    );
    let id = Identity {
        pane: 0,
        tab_id: "tab".into(),
        request_id: "request".into(),
        generation: 1,
    };
    let first = broker
        .start(id.clone(), root.path().to_str().unwrap().into())
        .await
        .unwrap();
    let file_id = first
        .batch
        .entries
        .iter()
        .find(|e| e.kind == EntryKind::File)
        .unwrap()
        .id
        .clone();
    let folder_id = first.batch.entries[0].id.clone();
    let file = broker
        .resolve_activation(&id, file_id.clone())
        .await
        .unwrap();
    assert_eq!(file.kind, ActivationKind::File);
    assert_eq!(file.path, root.path().join("file.txt").to_str().unwrap());
    let folder = broker.activate(id.clone(), folder_id).await.unwrap();
    assert_eq!(folder.kind, ActivationKind::Folder);
    assert_eq!(folder.path.as_deref(), root.path().join("folder").to_str());
    let reverse = broker
        .set_view(id.clone(), view(SortColumn::Name, SortDirection::Desc))
        .await
        .unwrap();
    assert!(reverse.batch.entries.len() <= 64);
    assert_eq!(
        broker
            .locate(id.clone(), file_id.clone())
            .await
            .unwrap()
            .index,
        Some(1)
    );
    assert_eq!(
        broker
            .resolve_activation(&id, "https://example.com".into())
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    let unknown = if cfg!(windows) { "w-0078" } else { "u-78" };
    assert_eq!(
        broker
            .locate(id.clone(), unknown.into())
            .await
            .unwrap()
            .index,
        None
    );
    assert_eq!(
        broker
            .resolve_activation(&id, unknown.into())
            .await
            .unwrap_err()
            .code,
        FaultCode::NotFound
    );
    fs::remove_file(root.path().join("file.txt")).unwrap();
    assert_eq!(
        broker
            .resolve_activation(&id, file_id.clone())
            .await
            .unwrap_err()
            .code,
        FaultCode::NotFound
    );
    let replacement = Identity {
        generation: 2,
        ..id.clone()
    };
    broker
        .start(replacement.clone(), root.path().to_str().unwrap().into())
        .await
        .unwrap();
    assert_eq!(
        broker
            .resolve_activation(&id, file_id.clone())
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert_eq!(
        broker.activate(id, file_id).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    broker.cancel(&replacement).await.unwrap();
}
