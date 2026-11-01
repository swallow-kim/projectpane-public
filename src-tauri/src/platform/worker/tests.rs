use super::entry::UNKNOWN_MODIFIED_AT_MS;
use super::*;
use std::path::PathBuf;

fn with_root(root: &str) -> Listing {
    Listing {
        root: Some(PathBuf::from(root).into_boxed_path()),
        ..Listing::default()
    }
}
fn entry(name: String) -> StoredEntry {
    StoredEntry {
        size_bytes: u64::MAX,
        name: std::ffi::OsString::from(name).into_boxed_os_str(),
        kind: EntryKind::File,
        modified_at_ms: UNKNOWN_MODIFIED_AT_MS,
    }
}

#[test]
fn root_cost_is_paid_once_and_100k_names_fit_without_raising_budget() {
    let short_root = "r";
    let long_root = "r".repeat(200);
    let mut short = with_root(short_root);
    let mut long = with_root(&long_root);
    for index in 0..MAX_ENTRIES {
        let name = format!("{index:06}-한글.txt");
        assert!(short.retain(entry(name.clone())).unwrap());
        assert!(long.retain(entry(name)).unwrap());
    }
    assert!(long.snapshot_bytes() < 10 * 1024 * 1024);
    assert_eq!(long.snapshot_bytes(), 9_724_808);
    assert_eq!(long.visible.capacity(), MAX_ENTRIES);
    assert_eq!(long.snapshot_bytes() - short.snapshot_bytes(), 199);
    let last = long.batch(MAX_ENTRIES - 1, 1).unwrap();
    assert_eq!(last.entries[0].name, "099999-한글.txt");
    assert_eq!(
        last.entries[0].id,
        native_id::encode(std::ffi::OsStr::new("099999-한글.txt"))
    );
    assert!(!last.capped);
    assert!(!long.retain(entry("overflow".into())).unwrap());
    assert_eq!(long.cap_reason, Some("entry_limit"));
}

#[test]
fn long_native_names_still_respect_snapshot_budget_and_reset_releases_storage() {
    let mut listing = with_root("root");
    while listing.retain(entry("한".repeat(255))).unwrap() {}
    assert_eq!(listing.cap_reason, Some("snapshot_budget"));
    assert!(listing.entries.len() < MAX_ENTRIES);
    assert!(listing.snapshot_bytes() <= MAX_SNAPSHOT_BYTES);
    listing = Listing::default();
    assert_eq!(listing.snapshot_bytes(), 0);
    assert_eq!(listing.entries.capacity(), 0);
}

#[test]
fn native_names_are_retained_even_when_display_is_lossy() {
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![b'a', 0xff])
    };
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0x61, 0xd800])
    };
    let mut listing = with_root("root");
    assert!(
        listing
            .retain(StoredEntry {
                size_bytes: u64::MAX,
                name: name.clone().into_boxed_os_str(),
                kind: EntryKind::File,
                modified_at_ms: UNKNOWN_MODIFIED_AT_MS,
            })
            .unwrap()
    );
    assert_eq!(&*listing.entries[0].name, name.as_os_str());
    assert_eq!(
        listing
            .root
            .as_ref()
            .unwrap()
            .join(&*listing.entries[0].name),
        Path::new("root").join(&name)
    );
    assert_eq!(listing.batch(0, 1).unwrap().entries[0].name, "a�");
}
