use super::*;
use std::ffi::OsString;

fn row(name: String) -> StoredEntry {
    StoredEntry {
        name: OsString::from(name).into_boxed_os_str(),
        kind: EntryKind::File,
        modified_at_ms: 0,
        size_bytes: 0,
    }
}

#[test]
fn dispersed_long_names_cluster_after_sort_but_every_display_page_is_full() {
    let mut listing = Listing::default();
    #[cfg(windows)]
    let suffix = "한".repeat(240);
    #[cfg(unix)]
    let suffix = "\"".repeat(240);
    for i in 0..1100 {
        let name = if i % 3 == 0 {
            format!("a{i:04}{suffix}")
        } else {
            format!("z{i:04}")
        };
        assert!(listing.retain(row(name)).unwrap());
    }
    listing.rebuild_view();
    let worker = listing.batch(0, BATCH_SIZE).unwrap();
    assert!(worker.entries.len() < BATCH_SIZE);
    let mut received = std::collections::HashSet::new();
    for offset in (0..1100).step_by(DISPLAY_PAGE_SIZE) {
        let page = listing.batch(offset, DISPLAY_PAGE_SIZE).unwrap();
        assert_eq!(page.entries.len(), DISPLAY_PAGE_SIZE.min(1100 - offset));
        assert!(serde_json::to_vec(&page).unwrap().len() < MAX_FRAME);
        for entry in page.entries {
            assert!(received.insert(entry.id));
        }
    }
    assert_eq!(received.len(), 1100);
    assert!(listing.cap_reason.is_none());
}

#[test]
fn an_individual_oversized_payload_is_explicitly_capped_before_retention() {
    let mut listing = Listing::default();
    assert!(!listing.retain(row("\u{1}".repeat(510))).unwrap());
    assert_eq!(listing.cap_reason, Some("payload_budget"));
    assert!(listing.entries.is_empty());
    let batch = listing.batch(0, DISPLAY_PAGE_SIZE).unwrap();
    assert!(batch.complete && batch.capped);
    assert_eq!(batch.cap_reason.as_deref(), Some("payload_budget"));
}
