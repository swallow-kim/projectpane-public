use super::*;
use crate::domain::{FaultCode, operations::*};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn source(path: &Path) -> OperationSource {
    capture_source(path.to_str().unwrap()).unwrap()
}
fn move_plan(path: &Path, destination: &Path) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action: PreparedAction::Move {
            sources: vec![source(path)],
            destination: source(destination),
        },
    }
}

#[tokio::test]
async fn same_volume_directory_move_preserves_nested_contents_and_native_identity() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let folder = root.join("한글 폴더");
    let target = root.join("destination");
    fs::create_dir_all(folder.join("nested/empty")).unwrap();
    fs::create_dir(&target).unwrap();
    let payload: Vec<u8> = (0..262144).map(|i| (i % 251) as u8).collect();
    fs::write(folder.join("nested/payload.bin"), &payload).unwrap();
    fs::write(folder.join("한글.txt"), b"owned UTF-8 filename fixture").unwrap();
    let before = source(&folder);
    let nested_before = source(&folder.join("nested/payload.bin"));
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(
            move_plan(&folder, &target),
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert!(!folder.exists());
    let moved = target.join("한글 폴더");
    let after = source(&moved);
    let nested_after = source(&moved.join("nested/payload.bin"));
    assert_eq!(
        (before.volume, before.file_index),
        (after.volume, after.file_index)
    );
    assert_eq!(
        (nested_before.volume, nested_before.file_index),
        (nested_after.volume, nested_after.file_index)
    );
    assert_eq!(fs::read(moved.join("nested/payload.bin")).unwrap(), payload);
    assert_eq!(
        fs::read(moved.join("한글.txt")).unwrap(),
        b"owned UTF-8 filename fixture"
    );
    assert!(moved.join("nested/empty").is_dir());
    assert_eq!(report.items.len(), 1);
}

#[test]
fn directory_self_descendant_collision_and_recursive_copy_are_refused_without_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("CaseFolder");
    let child = folder.join("child");
    let target = fixture.path().join("destination");
    fs::create_dir_all(&child).unwrap();
    fs::create_dir_all(target.join("CaseFolder")).unwrap();
    fs::write(folder.join("sentinel"), b"source").unwrap();
    fs::write(target.join("CaseFolder/sentinel"), b"target").unwrap();
    for destination in [&folder, &child] {
        assert_eq!(
            check_plan(&move_plan(&folder, destination))
                .unwrap_err()
                .code,
            FaultCode::InvalidRequest
        );
    }
    assert_eq!(
        check_plan(&move_plan(&folder, &target)).unwrap_err().code,
        FaultCode::DestinationExists
    );
    let copy = OperationPlan {
        project_id: None,
        action: PreparedAction::Copy {
            sources: vec![source(&folder)],
            destination: source(fixture.path()),
        },
    };
    assert_eq!(
        check_plan(&copy).unwrap_err().code,
        FaultCode::InvalidRequest
    );
    let overlapping = OperationPlan {
        project_id: None,
        action: PreparedAction::Move {
            sources: vec![source(&folder), source(&folder.join("sentinel"))],
            destination: source(&target),
        },
    };
    assert_eq!(
        check_plan(&overlapping).unwrap_err().code,
        FaultCode::InvalidRequest
    );
    assert_eq!(fs::read(folder.join("sentinel")).unwrap(), b"source");
    assert_eq!(
        fs::read(target.join("CaseFolder/sentinel")).unwrap(),
        b"target"
    );
}

#[tokio::test]
async fn same_volume_folder_move_preserves_junction_without_touching_its_target() {
    use std::os::windows::{fs::MetadataExt, process::CommandExt};
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    let outside = fixture.path().join("outside-owned-fixture");
    for path in [&folder, &destination, &outside] {
        fs::create_dir(path).unwrap();
    }
    fs::write(outside.join("sentinel"), b"outside unchanged").unwrap();
    let before = source(&outside.join("sentinel"));
    let created = std::process::Command::new("pwsh").args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:PROJECTPANE_TEST_JUNCTION -Value $env:PROJECTPANE_TEST_TARGET | Out-Null"])
        .env("PROJECTPANE_TEST_JUNCTION", folder.join("junction"))
        .env("PROJECTPANE_TEST_TARGET", &outside).creation_flags(0x08000000).output().unwrap();
    assert!(
        created.status.success(),
        "owned junction fixture creation failed"
    );
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(
            move_plan(&folder, &destination),
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.items.len(), 1);
    assert!(!folder.exists());
    let moved_link = destination.join("source/junction");
    assert_ne!(
        fs::symlink_metadata(&moved_link).unwrap().file_attributes() & 0x400,
        0
    );
    assert_eq!(source(&outside.join("sentinel")), before);
    assert_eq!(
        fs::read(outside.join("sentinel")).unwrap(),
        b"outside unchanged"
    );
    assert_eq!(
        fs::read(moved_link.join("sentinel")).unwrap(),
        b"outside unchanged"
    );
}

#[tokio::test]
#[ignore = "requires an explicitly supplied fixture parent on another fixed volume"]
async fn cross_volume_directory_move_stays_gated_and_preserves_both_roots() {
    let parent =
        std::env::var_os("PROJECTPANE_CROSS_VOLUME_FIXTURE").expect("explicit fixture parent");
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::Builder::new()
        .prefix("projectpane-directory-move-")
        .tempdir_in(parent)
        .unwrap();
    let folder = first.path().join("source");
    fs::create_dir_all(folder.join("child")).unwrap();
    fs::write(folder.join("child/sentinel"), b"unchanged").unwrap();
    assert_ne!(source(&folder).volume, source(second.path()).volume);
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(
            move_plan(&folder, second.path()),
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    lane.shutdown().await.unwrap();
    assert_eq!(result.unwrap_err().code, FaultCode::InvalidRequest);
    assert_eq!(
        fs::read(folder.join("child/sentinel")).unwrap(),
        b"unchanged"
    );
    assert_eq!(fs::read_dir(second.path()).unwrap().count(), 0);
}
