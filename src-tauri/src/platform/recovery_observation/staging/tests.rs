use super::*;
use sha2::{Digest, Sha256};
use std::fs;
use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_FLAG_BACKUP_SEMANTICS};

#[test]
fn staged_bytes_match_source_and_survive_handle_release() {
    for bytes in [Vec::new(), b"abc".to_vec(), vec![79; 1024 * 1024 + 1]] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("새 파일");
        fs::write(&source, &bytes).unwrap();

        let staged = create_stage(source.to_str().unwrap(), destination.to_str().unwrap()).unwrap();

        let receipt = serde_json::to_value(staged.receipt()).unwrap();
        assert_eq!(receipt["bytes"], bytes.len());
        assert_eq!(receipt["sha256"], format!("{:x}", Sha256::digest(&bytes)));
        assert_ne!(receipt["sourceIndex"], receipt["stagedIndex"]);
        assert!(receipt["sourceIndex"].is_string());
        assert!(receipt["stagedIndex"].is_string());
        drop(staged);
        assert_eq!(fs::read(source).unwrap(), bytes);
        assert_eq!(fs::read(destination).unwrap(), bytes);
    }
}

#[test]
fn occupied_destination_and_source_alias_remain_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::write(&source, b"source").unwrap();
    fs::write(&destination, b"original").unwrap();

    for target in [&source, &destination] {
        assert!(matches!(
            create_stage(source.to_str().unwrap(), target.to_str().unwrap()),
            Err(StageFailure::NotCreated(_))
        ));
    }

    assert_eq!(fs::read(source).unwrap(), b"source");
    assert_eq!(fs::read(destination).unwrap(), b"original");
}

#[test]
fn hardlinks_size_limit_and_writer_lock_refuse_before_creation() {
    for variant in ["hardlink", "limit", "locked"] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("new");
        fs::write(&source, b"abc").unwrap();
        let held = match variant {
            "hardlink" => {
                fs::hard_link(&source, root.path().join("alias")).unwrap();
                None
            }
            "limit" => {
                OpenOptions::new()
                    .write(true)
                    .open(&source)
                    .unwrap()
                    .set_len(MAX_STAGE_BYTES + 1)
                    .unwrap();
                None
            }
            "locked" => Some(OpenOptions::new().write(true).open(&source).unwrap()),
            _ => unreachable!(),
        };

        assert!(matches!(
            create_stage(source.to_str().unwrap(), destination.to_str().unwrap()),
            Err(StageFailure::NotCreated(_))
        ));

        assert!(!destination.exists());
        drop(held);
    }
}

#[test]
fn retained_handles_block_source_stage_and_parent_changes_until_drop() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("new");
    fs::write(&source, b"abc").unwrap();
    let staged = create_stage(source.to_str().unwrap(), destination.to_str().unwrap()).unwrap();

    for path in [&source, &destination] {
        assert!(OpenOptions::new().write(true).open(path).is_err());
        assert!(
            OpenOptions::new()
                .access_mode(DELETE)
                .share_mode(7)
                .open(path)
                .is_err()
        );
    }
    assert!(
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(root.path())
            .is_err()
    );
    drop(staged);

    assert!(OpenOptions::new().write(true).open(source).is_ok());
    assert!(OpenOptions::new().write(true).open(destination).is_ok());
}

#[test]
fn failed_copy_preserves_partial_file_and_original_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("partial");
    fs::write(&source, b"source").unwrap();

    let result = create_with(
        source.to_str().unwrap(),
        destination.to_str().unwrap(),
        |_, output, _| {
            output.write_all(b"part")?;
            Err(io::Error::other("injected_copy_failure"))
        },
    );

    assert!(matches!(result, Err(StageFailure::Unacknowledged(_))));
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(&destination).unwrap(), b"part");
    assert!(matches!(
        create_stage(source.to_str().unwrap(), destination.to_str().unwrap()),
        Err(StageFailure::NotCreated(_))
    ));
    assert_eq!(fs::read(destination).unwrap(), b"part");
}

#[test]
fn incorrect_copy_count_or_bytes_never_yields_a_receipt() {
    for bad_count in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("new");
        fs::write(&source, b"abc").unwrap();

        let result = create_with(
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            |_, output, _| {
                output.write_all(if bad_count { b"abc" } else { b"bad" })?;
                Ok(if bad_count { 2 } else { 3 })
            },
        );

        assert!(matches!(result, Err(StageFailure::Unacknowledged(_))));
        assert_eq!(fs::read(source).unwrap(), b"abc");
        assert!(destination.exists());
    }
}
