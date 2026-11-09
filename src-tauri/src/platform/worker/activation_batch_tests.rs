use super::*;
use std::{path::PathBuf, time::Instant};

#[test]
fn activation_batch_preserves_order_duplicates_first_error_and_snapshot() {
    let root = tempfile::tempdir().unwrap();
    for name in ["File2.txt", "file10.txt", "한글.txt"] {
        fs::write(root.path().join(name), b"synthetic").unwrap();
    }
    let mut listing = Listing::default();
    let open = listing
        .handle(Request::Open {
            path: root.path().to_str().unwrap().into(),
            view: ListingView::default(),
        })
        .unwrap();
    let ids: Vec<_> = open
        .entries
        .iter()
        .rev()
        .map(|e| e.id.clone())
        .chain(open.entries.iter().take(1).map(|e| e.id.clone()))
        .collect();
    let bytes = listing.snapshot_bytes();
    let targets = listing.activation_targets(&ids).unwrap();
    assert_eq!(targets.len(), ids.len());
    for (id, target) in ids.iter().zip(targets) {
        let serial = listing.activation_target(id).unwrap();
        assert_eq!(target.path, serial.path);
        assert_eq!(target.kind, serial.kind);
    }
    assert_eq!(bytes, listing.snapshot_bytes());
    let missing = native_id::encode(OsStr::new("missing.txt"));
    assert_eq!(
        listing
            .activation_targets(&[missing.clone(), "invalid".into()])
            .unwrap_err()
            .code,
        FaultCode::NotFound
    );
    assert_eq!(
        listing
            .activation_targets(&["invalid".into(), missing])
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    assert_eq!(
        listing.activation_targets(&[]).unwrap_err().code,
        FaultCode::InvalidRequest
    );
    assert_eq!(
        listing
            .activation_targets(&vec![ids[0].clone(); MAX_ACTIVATION_BATCH + 1])
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    let target = listing.activation_target(&ids[0]).unwrap();
    fs::remove_file(&target.path).unwrap();
    fs::create_dir(&target.path).unwrap();
    assert_eq!(
        listing.activation_targets(&ids).unwrap_err().code,
        FaultCode::InvalidPath
    );
}

#[test]
fn activation_batch_response_contract_rejects_wrong_count_and_shape() {
    let request = Request::ActivateMany {
        entry_ids: vec!["entry".into()],
    };
    assert!(WorkerResponse::Targets(vec![]).validate(&request).is_err());
    let target = ActivationTarget {
        kind: ActivationKind::File,
        path: std::env::temp_dir()
            .join("synthetic")
            .to_string_lossy()
            .into_owned(),
    };
    let wire = serde_json::to_vec(&WorkerResponse::Targets(vec![target])).unwrap();
    let parsed: WorkerResponse = serde_json::from_slice(&wire).unwrap();
    assert!(parsed.validate(&request).is_ok());
    assert!(
        parsed
            .validate(&Request::Activate {
                entry_id: "entry".into()
            })
            .is_err()
    );
}

// Synthetic snapshot; only the selected 128 files are created in a temporary folder.
#[test]
#[ignore]
fn benchmark_activation_batch() {
    let root = tempfile::tempdir().unwrap();
    let mut listing = Listing {
        root: Some(PathBuf::from(root.path()).into_boxed_path()),
        ..Listing::default()
    };
    let mut ids = Vec::new();
    for i in 0..100_000 {
        let name = format!("file{i:06}.txt");
        if i >= 100_000 - MAX_ACTIVATION_BATCH {
            fs::write(root.path().join(&name), b"synthetic").unwrap();
            ids.push(native_id::encode(OsStr::new(&name)));
        }
        listing.entries.push(StoredEntry {
            name: OsStr::new(&name).into(),
            kind: EntryKind::File,
            modified_at_ms: 0,
            size_bytes: 9,
        });
    }
    for _ in 0..3 {
        let start = Instant::now();
        let serial: Vec<_> = ids
            .iter()
            .map(|id| listing.activation_target(id).unwrap())
            .collect();
        let before = start.elapsed();
        let start = Instant::now();
        let batch = listing.activation_targets(&ids).unwrap();
        let after = start.elapsed();
        for (a, b) in serial.iter().zip(batch) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.kind, b.kind);
        }
        eprintln!("100k snapshot, last128 activation: serial={before:?} batch={after:?}");
    }
}
