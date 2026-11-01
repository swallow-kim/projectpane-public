use super::{valid, write_json};
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path, time::Instant};

const ZERO: &[u8] = br#"{"revision":0}"#;
const ONE: &[u8] = br#"{"revision":1}"#;
const TWO: &[u8] = br#"{"revision":2}"#;

fn seeded() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.json");
    write_json(&path, ZERO, valid).unwrap();
    write_json(&path, ONE, valid).unwrap();
    (directory, path)
}

fn deny_delete(path: &Path) -> fs::File {
    OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap()
}

#[test]
fn failed_primary_replace_preserves_preexisting_backup_and_retry() {
    let (_directory, path) = seeded();
    let backup = path.with_extension("json.bak");
    let held = deny_delete(&path);
    let start = Instant::now();
    let error = write_json(&path, TWO, valid).unwrap_err();
    assert!(matches!(error.raw_os_error(), Some(5 | 32 | 33)));
    assert!(start.elapsed().as_secs() < 3);
    assert_eq!(fs::read(&path).unwrap(), ONE);
    assert_eq!(fs::read(&backup).unwrap(), ZERO);
    drop(held);
    write_json(&path, TWO, valid).unwrap();
    assert_eq!(fs::read(&path).unwrap(), TWO);
    assert_eq!(fs::read(&backup).unwrap(), ONE);
}

#[test]
fn denied_backup_replace_preserves_both_generations_and_retry() {
    let (_directory, path) = seeded();
    let backup = path.with_extension("json.bak");
    let held = deny_delete(&backup);
    let start = Instant::now();
    let error = write_json(&path, TWO, valid).unwrap_err();
    assert!(matches!(error.raw_os_error(), Some(5 | 32 | 33)));
    assert!(start.elapsed().as_secs() < 3);
    assert_eq!(fs::read(&path).unwrap(), ONE);
    assert_eq!(fs::read(&backup).unwrap(), ZERO);
    drop(held);
    write_json(&path, TWO, valid).unwrap();
    assert_eq!(fs::read(&path).unwrap(), TWO);
    assert_eq!(fs::read(&backup).unwrap(), ONE);
}
