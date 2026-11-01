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
fn plan(action: PreparedAction) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action,
    }
}
fn token() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[tokio::test]
async fn real_sta_new_folder_rename_copy_move_preserve_exact_bytes() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let payload: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
    let original = root.join("한글 원본.txt");
    fs::write(&original, &payload).unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let created = lane
        .execute(
            plan(PreparedAction::NewFolder {
                parent: source(root),
                name: "새 폴더".into(),
            }),
            0,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), OperationStatus::Succeeded, "{created:?}");
    let destination = root.join("새 폴더");
    assert!(destination.is_dir());
    let copied = lane
        .execute(
            plan(PreparedAction::Copy {
                sources: vec![source(&original)],
                destination: source(&destination),
            }),
            0,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(copied.status(), OperationStatus::Succeeded, "{copied:?}");
    assert_eq!(
        fs::read(destination.join("한글 원본.txt")).unwrap(),
        payload
    );
    assert_eq!(fs::read(&original).unwrap(), payload);
    let renamed = lane
        .execute(
            plan(PreparedAction::Rename {
                source: source(&original),
                name: "변경.txt".into(),
            }),
            0,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.status(), OperationStatus::Succeeded, "{renamed:?}");
    assert!(!original.exists());
    let renamed_path = root.join("변경.txt");
    let moved = lane
        .execute(
            plan(PreparedAction::Move {
                sources: vec![source(&renamed_path)],
                destination: source(&destination),
            }),
            0,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(moved.status(), OperationStatus::Succeeded, "{moved:?}");
    assert!(!renamed_path.exists());
    assert_eq!(fs::read(destination.join("변경.txt")).unwrap(), payload);
    lane.shutdown().await.unwrap();
}

#[tokio::test]
async fn real_sta_collision_changed_source_folder_recycle_and_queued_cancel_preserve_fixture() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let original = root.join("original.txt");
    let destination = root.join("destination");
    fs::create_dir(&destination).unwrap();
    fs::write(&original, b"original").unwrap();
    fs::write(destination.join("original.txt"), b"sentinel").unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let collision = lane
        .execute(
            plan(PreparedAction::Copy {
                sources: vec![source(&original)],
                destination: source(&destination),
            }),
            0,
            token(),
        )
        .await;
    assert_eq!(collision.unwrap_err().code, FaultCode::DestinationExists);
    let cancelled = lane
        .execute(
            plan(PreparedAction::NewFolder {
                parent: source(root),
                name: "cancelled".into(),
            }),
            0,
            Arc::new(AtomicBool::new(true)),
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status(), OperationStatus::Cancelled);
    assert!(!root.join("cancelled").exists());
    let stale = source(&original);
    fs::rename(&original, root.join("held-original.txt")).unwrap();
    fs::write(&original, b"replacement").unwrap();
    let changed = lane
        .execute(
            plan(PreparedAction::Rename {
                source: stale,
                name: "wrong.txt".into(),
            }),
            0,
            token(),
        )
        .await;
    assert_eq!(changed.unwrap_err().code, FaultCode::StaleRequest);
    assert!(
        lane.execute(
            plan(PreparedAction::Recycle {
                sources: vec![source(&destination)]
            }),
            0,
            token()
        )
        .await
        .is_err()
    );
    assert_eq!(fs::read(&original).unwrap(), b"replacement");
    assert_eq!(
        fs::read(destination.join("original.txt")).unwrap(),
        b"sentinel"
    );
    assert_eq!(
        fs::read(root.join("held-original.txt")).unwrap(),
        b"original"
    );
    assert!(!root.join("wrong.txt").exists());
    lane.shutdown().await.unwrap();
}

#[test]
fn real_identity_rejects_hardlink_duplicates_and_root_mutation() {
    let fixture = tempfile::tempdir().unwrap();
    let first = fixture.path().join("first.txt");
    let alias = fixture.path().join("alias.txt");
    fs::write(&first, b"fixture").unwrap();
    fs::hard_link(&first, &alias).unwrap();
    let action = PreparedAction::Copy {
        sources: vec![source(&first), source(&alias)],
        destination: source(fixture.path()),
    };
    assert!(check_plan(&plan(action)).is_err());
    let root = fixture.path().ancestors().last().unwrap();
    assert!(
        check_plan(&plan(PreparedAction::Rename {
            source: source(root),
            name: "wrong".into()
        }))
        .is_err()
    );
    assert_eq!(fs::read(first).unwrap(), b"fixture");
}
