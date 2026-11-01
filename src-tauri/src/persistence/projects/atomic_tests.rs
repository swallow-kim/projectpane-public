use super::atomic::write_json;
use std::{fs, io};

#[cfg(windows)]
pub(super) mod windows;
fn valid(bytes: &[u8]) -> io::Result<()> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .map(|_| ())
        .map_err(io::Error::other)
}

#[test]
fn atomic_create_and_replace_preserve_valid_backup() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("project.json");
    write_json(&path, br#"{"revision":1}"#, valid).unwrap();
    write_json(&path, br#"{"revision":2}"#, valid).unwrap();
    assert_eq!(fs::read(&path).unwrap(), br#"{"revision":2}"#);
    assert_eq!(
        fs::read(path.with_extension("json.bak")).unwrap(),
        br#"{"revision":1}"#
    );
}

#[test]
fn corrupt_original_and_backup_are_not_overwritten() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("project.json");
    fs::write(&path, b"{broken").unwrap();
    assert!(write_json(&path, b"{}", valid).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken");
    fs::write(&path, b"{}").unwrap();
    let backup = path.with_extension("json.bak");
    fs::write(&backup, b"{broken-backup").unwrap();
    assert!(write_json(&path, b"{\"new\":true}", valid).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    assert_eq!(fs::read(&backup).unwrap(), b"{broken-backup");
}

#[test]
fn oversized_read_and_write_preserve_original_and_unowned_temporary() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("project.json");
    let stale = fixture.path().join(".projects-stale.tmp");
    fs::write(&stale, b"recoverable").unwrap();
    fs::write(&path, b"{}").unwrap();
    let large = vec![b' '; super::atomic::MAX_BYTES + 1];
    assert!(write_json(&path, &large, valid).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    fs::write(&path, &large).unwrap();
    assert!(super::atomic::read(&path).is_err());
    assert_eq!(fs::read(&stale).unwrap(), b"recoverable");
}

#[cfg(windows)]
#[test]
fn windows_sharing_failure_is_finite_and_preserves_valid_state() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, time::Instant};
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("project.json");
    write_json(&path, b"{\"revision\":1}", valid).unwrap();
    // Allow reads but deny delete sharing, so backup reads work and native rename fails.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let start = Instant::now();
    let error = write_json(&path, b"{\"revision\":2}", valid).unwrap_err();
    assert!(
        matches!(error.raw_os_error(), Some(5 | 32 | 33)),
        "{error:?}"
    );
    assert!(start.elapsed().as_secs() < 3);
    assert_eq!(fs::read(&path).unwrap(), b"{\"revision\":1}");
    assert_eq!(
        fs::read(path.with_extension("json.bak")).unwrap(),
        b"{\"revision\":1}"
    );
    drop(held);
    write_json(&path, b"{\"revision\":2}", valid).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"{\"revision\":2}");
}
