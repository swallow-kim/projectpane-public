use super::super::capture_source;
use super::*;
fn source(path: &Path) -> OperationSource {
    capture_source(path.to_str().unwrap()).unwrap()
}
fn run(
    roots: Vec<OperationSource>,
    destination: OperationSource,
    name: &str,
    extract: bool,
) -> OperationReport {
    let PreparedAction::Archive {
        roots,
        descendants,
        destination,
        name,
        extract,
    } = prepare(roots, destination, name.into(), extract).unwrap()
    else {
        panic!()
    };
    execute(
        &roots,
        &descendants,
        &destination,
        &name,
        extract,
        &AtomicBool::new(false),
        &Default::default(),
    )
    .unwrap()
}
#[test]
fn windows_zip_roundtrip_multiple_unicode_files_hidden_file_empty_folder_and_zone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let folder = root.join("한글 공백");
    fs::create_dir_all(folder.join("empty")).unwrap();
    fs::write(folder.join("a.txt"), b"original bytes").unwrap();
    fs::write(root.join(".env"), b"hidden-like bytes").unwrap();
    let report = run(
        vec![source(&folder), source(&root.join(".env"))],
        source(root),
        "result.zip",
        false,
    );
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    fs::write(
        format!("{}:Zone.Identifier", root.join("result.zip").display()),
        b"[ZoneTransfer]\r\nZoneId=3\r\n",
    )
    .unwrap();
    let report = run(
        vec![source(&root.join("result.zip"))],
        source(root),
        "extracted",
        true,
    );
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(
        fs::read(root.join("extracted/한글 공백/a.txt")).unwrap(),
        b"original bytes"
    );
    assert_eq!(
        fs::read(root.join("extracted/.env")).unwrap(),
        b"hidden-like bytes"
    );
    assert!(root.join("extracted/한글 공백/empty").is_dir());
    assert_eq!(
        fs::read(format!(
            "{}:Zone.Identifier",
            root.join("extracted/한글 공백/a.txt").display()
        ))
        .unwrap(),
        b"[ZoneTransfer]\r\nZoneId=3\r\n"
    );
    assert_eq!(fs::read(folder.join("a.txt")).unwrap(), b"original bytes");
    assert!(
        prepare(
            vec![source(&folder)],
            source(root),
            "result.zip".into(),
            false
        )
        .is_err()
    );
}
#[test]
fn corrupt_archive_is_not_reported_as_success_and_does_not_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.txt"), b"verify CRC bytes").unwrap();
    assert_eq!(
        run(
            vec![source(&root.join("a.txt"))],
            source(root),
            "data.zip",
            false
        )
        .status(),
        OperationStatus::Succeeded
    );
    let mut bytes = fs::read(root.join("data.zip")).unwrap();
    let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
    bytes[central + 16] ^= 0xff;
    fs::write(root.join("data.zip"), bytes).unwrap();
    let report = run(
        vec![source(&root.join("data.zip"))],
        source(root),
        "partial",
        true,
    );
    assert!(!report.evidence.perform_succeeded);
    assert_eq!(report.counts.unknown, 1);
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"verify CRC bytes");
    assert!(
        prepare(
            vec![source(&root.join("data.zip"))],
            source(root),
            "partial".into(),
            true
        )
        .is_err()
    );
}
#[test]
fn cancellation_before_archive_creation_and_output_inside_source_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let folder = root.join("folder");
    fs::create_dir(&folder).unwrap();
    assert!(
        prepare(
            vec![source(&folder)],
            source(&folder),
            "inside.zip".into(),
            false
        )
        .is_err()
    );
    fs::write(root.join("a.txt"), b"unchanged").unwrap();
    assert_eq!(
        execute(
            &[source(&root.join("a.txt"))],
            &[],
            &source(root),
            "cancel.zip",
            false,
            &AtomicBool::new(true),
            &Default::default()
        )
        .unwrap_err()
        .code,
        FaultCode::Cancelled
    );
    assert!(!root.join("cancel.zip").exists());
}
