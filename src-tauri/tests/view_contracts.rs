use projectpane_core::{domain::*, platform::worker::Listing};
use serde_json::json;

#[test]
fn view_defaults_and_strict_validation_match_frontend_bounds() {
    let request: Request = serde_json::from_value(json!({"kind":"open","path":"unused"})).unwrap();
    assert!(matches!(request, Request::Open { view, .. } if view == ListingView::default()));
    let valid = serde_json::to_value(ListingView::default()).unwrap();
    for (field, value) in [
        ("sort", json!("other")),
        ("direction", json!("other")),
        ("foldersFirst", json!(0)),
        ("filter", json!(null)),
        ("extra", json!(true)),
    ] {
        let mut malformed = valid.clone();
        malformed[field] = value;
        assert!(serde_json::from_value::<ListingView>(malformed).is_err());
    }
    for filter in ["x".repeat(257), "🧪".repeat(129), "a\0b".into()] {
        assert_eq!(
            ListingView {
                filter,
                ..ListingView::default()
            }
            .validate()
            .unwrap_err()
            .code,
            FaultCode::InvalidRequest
        );
    }
    for filter in ["x".repeat(256), "🧪".repeat(128), "한".repeat(256)] {
        assert!(
            ListingView {
                filter,
                ..ListingView::default()
            }
            .validate()
            .is_ok()
        );
    }
}

#[test]
fn changed_file_kind_and_links_are_not_dispatched_as_snapshot_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, b"untouched").unwrap();
    let mut listing = Listing::default();
    let open = listing
        .handle(Request::Open {
            path: root.path().to_str().unwrap().into(),
            view: ListingView::default(),
        })
        .unwrap();
    let id = &open.entries[0].id;
    assert_eq!(
        listing.activation_target(id).unwrap().kind,
        ActivationKind::File
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        listing.activation_target(id).unwrap_err().code,
        FaultCode::InvalidPath
    );
    #[cfg(unix)]
    {
        std::fs::remove_dir(&path).unwrap();
        std::os::unix::fs::symlink(root.path(), &path).unwrap();
        assert_eq!(
            listing.activation_target(id).unwrap_err().code,
            FaultCode::InvalidPath
        );
    }
}
