use projectpane_core::{domain::Request, platform::worker::Listing};
use std::{
    fs::FileTimes,
    time::{Duration, UNIX_EPOCH},
};

#[test]
fn native_file_timestamp_is_serialized_in_milliseconds() {
    // Given a real file with a known native modification time.
    let root = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(root.path().join("dated.txt")).unwrap();
    file.set_times(
        FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_millis(1_700_000_000_123)),
    )
    .unwrap();
    // When the worker enumerates it.
    let batch = Listing::default()
        .handle(Request::Open {
            view: Default::default(),
            path: root.path().to_string_lossy().into_owned(),
        })
        .unwrap();
    // Then the wire entry includes the native timestamp, including milliseconds.
    let wire = serde_json::to_value(&batch.entries[0]).unwrap();
    assert_eq!(
        wire.get("modifiedAtMs"),
        Some(&serde_json::json!(1_700_000_000_123_i64))
    );
}

#[test]
fn native_folder_timestamp_keeps_folder_kind() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("folder");
    std::fs::create_dir(&path).unwrap();
    #[cfg(windows)]
    let folder = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_WRITE_ATTRIBUTES and FILE_FLAG_BACKUP_SEMANTICS permit directory times.
        std::fs::OpenOptions::new()
            .access_mode(0x100)
            .custom_flags(0x0200_0000)
            .open(&path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let folder = std::fs::File::open(&path).unwrap();
    folder
        .set_times(
            FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_millis(1_600_000_000_456)),
        )
        .unwrap();

    let batch = Listing::default()
        .handle(Request::Open {
            view: Default::default(),
            path: root.path().to_string_lossy().into_owned(),
        })
        .unwrap();

    assert_eq!(
        batch.entries[0].kind,
        projectpane_core::domain::EntryKind::Folder
    );
    assert_eq!(batch.entries[0].modified_at_ms, Some(1_600_000_000_456));
}

#[test]
fn window_keeps_captured_timestamp_after_file_changes() {
    let root = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(root.path().join("snapshot.txt")).unwrap();
    file.set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_600_000_000)))
        .unwrap();
    let mut listing = Listing::default();
    let open = || Request::Open {
        view: Default::default(),
        path: root.path().to_string_lossy().into_owned(),
    };
    listing.handle(open()).unwrap();
    file.set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000)))
        .unwrap();

    let snapshot = listing
        .handle(Request::Window {
            offset: 0,
            limit: 1,
        })
        .unwrap();

    assert_eq!(snapshot.entries[0].modified_at_ms, Some(1_600_000_000_000));
    let fresh = Listing::default().handle(open()).unwrap();
    assert_eq!(fresh.entries[0].modified_at_ms, Some(1_700_000_000_000));
}

#[test]
fn entry_serialization_preserves_zero_negative_and_null() {
    use projectpane_core::domain::Entry;
    for millis in [Some(0), Some(-1234), None] {
        let entry = Entry {
            size_bytes: None,
            id: "entry-0".into(),
            name: "file".into(),
            kind: projectpane_core::domain::EntryKind::File,
            modified_at_ms: millis,
        };
        let wire = serde_json::to_value(&entry).unwrap();
        assert_eq!(wire.get("modifiedAtMs"), Some(&serde_json::json!(millis)));
        let decoded: Entry = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded.modified_at_ms, millis);
    }
}

#[test]
fn worker_wire_reports_native_timestamp() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let root = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(root.path().join("wire.txt")).unwrap();
    file.set_times(FileTimes::new().set_modified(UNIX_EPOCH - Duration::from_millis(1234)))
        .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_projectpane-read-worker"));
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    serde_json::to_writer(
        &mut input,
        &Request::Open {
            view: Default::default(),
            path: root.path().to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    input.write_all(b"\n").unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let wire: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(wire["Ok"]["entries"][0]["modifiedAtMs"], -1234);
}
