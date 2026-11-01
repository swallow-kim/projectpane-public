use super::{entry::UNKNOWN_MODIFIED_AT_MS, *};
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

fn row(name: &str, kind: EntryKind, size: u64, date: i64) -> StoredEntry {
    StoredEntry {
        name: OsStr::new(name).into(),
        kind,
        size_bytes: size,
        modified_at_ms: date,
    }
}
fn sorted(listing: &mut Listing, sort: SortColumn, direction: SortDirection) -> Vec<String> {
    listing
        .handle(Request::SetView {
            view: ListingView {
                sort,
                direction,
                folders_first: false,
                filter: String::new(),
            },
        })
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[test]
fn nulls_are_last_both_directions_and_type_uses_extension_not_dotfile() {
    let mut listing = Listing::default();
    for item in [
        row("z.txt", EntryKind::File, 9, 10),
        row("a.rs", EntryKind::File, 2, 20),
        row(
            ".dotfile",
            EntryKind::File,
            u64::MAX,
            UNKNOWN_MODIFIED_AT_MS,
        ),
    ] {
        listing.retain(item).unwrap();
    }
    assert_eq!(
        sorted(&mut listing, SortColumn::Size, SortDirection::Asc),
        ["a.rs", "z.txt", ".dotfile"]
    );
    assert_eq!(
        sorted(&mut listing, SortColumn::Size, SortDirection::Desc),
        ["z.txt", "a.rs", ".dotfile"]
    );
    assert_eq!(
        sorted(&mut listing, SortColumn::Modified, SortDirection::Asc),
        ["z.txt", "a.rs", ".dotfile"]
    );
    assert_eq!(
        sorted(&mut listing, SortColumn::Modified, SortDirection::Desc),
        ["a.rs", "z.txt", ".dotfile"]
    );
    assert_eq!(
        sorted(&mut listing, SortColumn::Type, SortDirection::Asc),
        [".dotfile", "a.rs", "z.txt"]
    );
    assert_eq!(
        sorted(&mut listing, SortColumn::Type, SortDirection::Desc),
        ["z.txt", "a.rs", ".dotfile"]
    );
}

#[test]
fn natural_numeric_magnitude_leading_zeros_case_and_unicode_are_deterministic() {
    let mut listing = Listing::default();
    for name in [
        "file10",
        "file2",
        "file02",
        "File2",
        "file9999999999999999999999999999999999999",
        "한2",
        "한10",
    ] {
        listing.retain(row(name, EntryKind::File, 0, 0)).unwrap();
    }
    let asc = sorted(&mut listing, SortColumn::Name, SortDirection::Asc);
    assert_eq!(
        asc,
        [
            "File2",
            "file02",
            "file2",
            "file10",
            "file9999999999999999999999999999999999999",
            "한2",
            "한10"
        ]
    );
    let desc = sorted(&mut listing, SortColumn::Name, SortDirection::Desc);
    assert_eq!(desc, asc.into_iter().rev().collect::<Vec<_>>());
    listing
        .retain(row("Ångström", EntryKind::File, 0, 0))
        .unwrap();
    let filtered = listing
        .handle(Request::SetView {
            view: ListingView {
                filter: "ång".into(),
                ..ListingView::default()
            },
        })
        .unwrap();
    assert_eq!(filtered.entries[0].name, "Ångström");
    assert_eq!(filtered.total, 1);
}

#[test]
fn lossy_native_names_have_distinct_ids_and_cannot_activate() {
    #[cfg(windows)]
    let names = {
        use std::os::windows::ffi::OsStringExt;
        [
            OsString::from_wide(&[0x61, 0xd800]),
            OsString::from_wide(&[0x61, 0xd801]),
        ]
    };
    #[cfg(unix)]
    let names = {
        use std::os::unix::ffi::OsStringExt;
        [
            OsString::from_vec(vec![b'a', 0xfe]),
            OsString::from_vec(vec![b'a', 0xff]),
        ]
    };
    let root = tempfile::tempdir().unwrap();
    let mut listing = Listing {
        root: Some(PathBuf::from(root.path()).into_boxed_path()),
        ..Listing::default()
    };
    for name in &names {
        listing
            .retain(StoredEntry {
                name: name.clone().into_boxed_os_str(),
                kind: EntryKind::File,
                size_bytes: 0,
                modified_at_ms: 0,
            })
            .unwrap();
    }
    let before = listing.batch(0, 2).unwrap();
    assert_eq!(before.entries[0].name, before.entries[1].name);
    assert_ne!(before.entries[0].id, before.entries[1].id);
    for (index, entry) in before.entries.iter().enumerate() {
        assert_eq!(native_id::decode(&entry.id).unwrap(), names[index]);
        assert_eq!(listing.locate(&entry.id).unwrap().index, Some(index));
        assert_eq!(
            listing.activation_target(&entry.id).unwrap_err().code,
            FaultCode::InvalidPath
        );
    }
    sorted(&mut listing, SortColumn::Name, SortDirection::Desc);
    for entry in before.entries {
        let index = listing.locate(&entry.id).unwrap().index.unwrap();
        assert_eq!(listing.batch(index, 1).unwrap().entries[0].id, entry.id);
    }
}

#[test]
fn native_ids_reject_noncanonical_and_oversized_inputs() {
    for id in [
        "",
        "entry-1",
        "w-0",
        "w-D800",
        "w-zzzz",
        "u-FF",
        "u-0",
        &"x".repeat(MAX_ENTRY_ID + 1),
    ] {
        assert_eq!(
            native_id::decode(id).unwrap_err().code,
            FaultCode::InvalidRequest
        );
    }
    #[cfg(windows)]
    let name = OsString::from("한".repeat(255));
    #[cfg(unix)]
    let name = OsString::from("a".repeat(255));
    let id = native_id::encode(&name);
    assert!(id.len() <= MAX_ENTRY_ID);
    assert_eq!(native_id::decode(&id).unwrap(), name);
}

#[test]
fn reordered_long_names_are_split_into_bounded_frames_without_losing_rows() {
    let mut listing = Listing::default();
    for i in 0..600 {
        listing
            .retain(row(
                &format!("{i:03}{}", "한".repeat(250)),
                EntryKind::File,
                0,
                0,
            ))
            .unwrap();
    }
    listing.rebuild_view();
    let mut offset = 0;
    while offset < 600 {
        let batch = listing.batch(offset, BATCH_SIZE).unwrap();
        assert!(!batch.entries.is_empty());
        assert!(serde_json::to_vec(&batch).unwrap().len() < MAX_FRAME);
        assert!(batch.entries.len() <= BATCH_SIZE);
        assert_eq!(batch.total, 600);
        offset += batch.entries.len();
    }
    assert_eq!(offset, 600);
}
