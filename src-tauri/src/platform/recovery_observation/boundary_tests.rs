use super::*;
use std::process::Command;

#[test]
fn junction_leaf_and_ancestor_are_refused_without_touching_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("outside");
    let link = root.path().join("junction");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("sentinel"), b"retain").unwrap();
    let status = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .unwrap();
    assert!(status.status.success());
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    assert_eq!(
        collector.observe(link.to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert_eq!(
        collector.observe(link.join("sentinel").to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert_eq!(fs::read(target.join("sentinel")).unwrap(), b"retain");
    assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
}

#[test]
fn missing_parent_is_not_a_confirmed_absent_leaf() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("absent").join("leaf");
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    assert_eq!(
        collector.observe(path.to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert!(!root.path().join("absent").exists());
}

#[test]
fn retained_ancestors_close_the_check_open_gap_and_block_delete_access() {
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let leaf = parent.join("absent");
    let guards = native::ancestors(leaf.to_str().unwrap(), 128).unwrap();
    assert!(
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&parent)
            .is_err()
    );
    assert!(
        OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&parent)
            .is_ok()
    );
    assert!(fs::rename(&parent, root.path().join("moved")).is_err());
    assert!(!leaf.exists());
    drop(guards);
    assert!(
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&parent)
            .is_ok()
    );
}

#[test]
fn multi_buffer_digest_eof_and_actual_total_budget_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("million-a");
    fs::write(&path, vec![b'a'; 1_000_000]).unwrap();
    let content = fingerprint(&mut File::open(&path).unwrap(), 1_000_000).unwrap();
    assert_eq!(
        content
            .sha256
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>(),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    assert!(fingerprint(&mut File::open(&path).unwrap(), 1_000_001).is_err());
    let left = root.path().join("left");
    let right = root.path().join("right");
    let overflow = root.path().join("overflow");
    for path in [&left, &right] {
        let file = File::create(path).unwrap();
        file.set_len(MAX_OBSERVATION_BYTES / 2).unwrap();
    }
    fs::write(&overflow, b"x").unwrap();
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    for path in [&left, &right] {
        assert!(matches!(
            collector.observe(path.to_str().unwrap()),
            FileObservation::Present { .. }
        ));
    }
    assert_eq!(collector.remaining, 0);
    assert_eq!(
        collector.observe(overflow.to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert!(native::ancestors(overflow.to_str().unwrap(), 0).is_err());
}

#[test]
fn aggregate_handle_limit_accepts_exact_capacity_then_refuses_more() {
    use windows_sys::Win32::Storage::FileSystem::DELETE;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");
    fs::write(&path, b"x").unwrap();
    let needed = path.ancestors().count();
    let mut collector = Collector {
        remaining: MAX_OBSERVATION_BYTES,
        held: Vec::new(),
    };
    for _ in 0..MAX_OBSERVATION_HANDLES - needed {
        collector.held.push(File::open(&path).unwrap());
    }
    assert!(matches!(
        collector.observe(path.to_str().unwrap()),
        FileObservation::Present { .. }
    ));
    assert_eq!(collector.held.len(), MAX_OBSERVATION_HANDLES);
    assert_eq!(
        collector.observe(path.to_str().unwrap()),
        FileObservation::Inaccessible
    );
    assert_eq!(collector.held.len(), MAX_OBSERVATION_HANDLES);
    assert!(
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .open(&path)
            .is_err()
    );
    drop(collector);
    assert!(
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .open(&path)
            .is_ok()
    );
}
