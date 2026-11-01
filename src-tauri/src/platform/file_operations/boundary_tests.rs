use super::*;
use crate::domain::operations::*;
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn source(path: &Path) -> OperationSource {
    capture_source(path.to_str().unwrap()).unwrap()
}
async fn execute(lane: &NativeFileOperationLane, action: PreparedAction) -> OperationReport {
    lane.execute(
        OperationPlan {
            project_id: None,
            action,
        },
        0,
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn actual_case_only_and_nonempty_folder_rename_preserve_identity_and_contents() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("원래 폴더");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("name.txt"), b"untouched child").unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let file = source(&folder.join("name.txt"));
    let renamed = execute(
        &lane,
        PreparedAction::Rename {
            source: file.clone(),
            name: "NAME.txt".into(),
        },
    )
    .await;
    assert_eq!(renamed.status(), OperationStatus::Succeeded, "{renamed:?}");
    assert_eq!(source(&folder.join("NAME.txt")).file_index, file.file_index);
    assert_eq!(
        fs::read_dir(&folder)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name(),
        "NAME.txt"
    );
    let original = source(&folder);
    let result = execute(
        &lane,
        PreparedAction::Rename {
            source: original.clone(),
            name: "변경 폴더".into(),
        },
    )
    .await;
    assert_eq!(result.status(), OperationStatus::Succeeded, "{result:?}");
    assert!(!folder.exists());
    let destination = fixture.path().join("변경 폴더");
    assert_eq!(source(&destination).file_index, original.file_index);
    assert_eq!(
        fs::read(destination.join("NAME.txt")).unwrap(),
        b"untouched child"
    );
    lane.shutdown().await.unwrap();
}

#[tokio::test]
async fn actual_locked_source_is_not_renamed_or_reported_successful() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("locked.txt");
    fs::write(&path, b"sentinel").unwrap();
    let source = source(&path);
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(
            OperationPlan {
                project_id: None,
                action: PreparedAction::Rename {
                    source,
                    name: "changed.txt".into(),
                },
            },
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    if let Ok(report) = result {
        assert_ne!(report.status(), OperationStatus::Succeeded, "{report:?}");
        assert_eq!(report.counts.changed, 0);
    }
    assert_eq!(fs::read(&path).unwrap(), b"sentinel");
    assert!(!fixture.path().join("changed.txt").exists());
    drop(locked);
    lane.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PROJECTPANE_CROSS_VOLUME_FIXTURE on a different fixed volume"]
async fn actual_cross_volume_file_copy_and_move_verify_all_bytes() {
    let destination_parent = std::env::var_os("PROJECTPANE_CROSS_VOLUME_FIXTURE")
        .expect("dedicated fixture parent required");
    let origin = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir_in(destination_parent).unwrap();
    assert_ne!(source(origin.path()).volume, source(target.path()).volume);
    let payload: Vec<u8> = (0..262144).map(|i| (i % 251) as u8).collect();
    let copy_source = origin.path().join("한글 복사.txt");
    let move_source = origin.path().join("한글 이동.txt");
    fs::write(&copy_source, &payload).unwrap();
    fs::write(&move_source, &payload).unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let copied = execute(
        &lane,
        PreparedAction::Copy {
            sources: vec![source(&copy_source)],
            destination: source(target.path()),
        },
    )
    .await;
    assert_eq!(fs::read(&copy_source).unwrap(), payload);
    assert_eq!(
        fs::read(target.path().join("한글 복사.txt")).unwrap(),
        payload
    );
    assert_eq!(copied.status(), OperationStatus::Succeeded, "{copied:?}");
    let moved = execute(
        &lane,
        PreparedAction::Move {
            sources: vec![source(&move_source)],
            destination: source(target.path()),
        },
    )
    .await;
    assert!(!move_source.exists());
    assert_eq!(
        fs::read(target.path().join("한글 이동.txt")).unwrap(),
        payload
    );
    assert_eq!(moved.status(), OperationStatus::Succeeded, "{moved:?}");
    lane.shutdown().await.unwrap();
}
