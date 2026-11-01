use super::*;
use crate::domain::{ActivationKind, ActivationTarget, Identity, operations::*};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub(super) fn copy_plan(
    paths: &[&Path],
    destination: &Path,
) -> Result<OperationPlan, crate::domain::Fault> {
    let targets = paths
        .iter()
        .map(|path| ActivationTarget {
            path: path.to_str().unwrap().into(),
            kind: if path.is_dir() {
                ActivationKind::Folder
            } else {
                ActivationKind::File
            },
        })
        .collect();
    super::prepare::prepare(
        PrepareOperation {
            project_id: None,
            action: OperationAction::Copy {
                selection: OperationSelection {
                    identity: Identity {
                        pane: 0,
                        tab_id: "copy-test".into(),
                        request_id: "copy-request".into(),
                        generation: 1,
                    },
                    entry_ids: (0..paths.len())
                        .map(|index| format!("entry-{index}"))
                        .collect(),
                },
                destination: destination.to_str().unwrap().into(),
            },
        },
        targets,
    )
}

#[tokio::test]
async fn native_directory_copy_preserves_all_sources_and_covers_every_descendant() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("한글 폴더");
    let file = fixture.path().join("separate.txt");
    let target = fixture.path().join("target");
    fs::create_dir_all(folder.join("nested/empty")).unwrap();
    fs::create_dir(&target).unwrap();
    let payload: Vec<u8> = (0..262144).map(|i| (i % 251) as u8).collect();
    fs::write(folder.join("nested/한글.bin"), &payload).unwrap();
    fs::write(&file, b"separate file").unwrap();
    let plan = copy_plan(&[&folder, &file], &target).expect("bounded tree can be prepared");
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(plan, 0, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(
        report.items.len(),
        5,
        "root, nested folder, empty folder, nested file, separate file: {report:?}"
    );
    assert_eq!(report.counts.changed, 5);
    assert_eq!(report.counts.unknown, 0);
    assert!(report.evidence.outcomes_complete);
    assert_eq!(fs::read(folder.join("nested/한글.bin")).unwrap(), payload);
    assert_eq!(
        fs::read(target.join("한글 폴더/nested/한글.bin")).unwrap(),
        payload
    );
    assert!(folder.join("nested/empty").is_dir());
    assert!(target.join("한글 폴더/nested/empty").is_dir());
    assert_eq!(fs::read(file).unwrap(), b"separate file");
    assert_eq!(
        fs::read(target.join("separate.txt")).unwrap(),
        b"separate file"
    );
    report.validate().unwrap();
}

#[test]
fn copy_callback_empty_name_keeps_the_planned_leaf_but_explicit_rename_is_refused() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("source");
    let target = fixture.path().join("target");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(&target).unwrap();
    let plan = copy_plan(&[&folder], &target).unwrap();
    let guard = super::copy_tree_sink::TreeCopyProgress::default();
    for name in [None, Some(""), Some("source")] {
        guard
            .check_before(
                &plan,
                folder.to_str().unwrap(),
                target.to_str().unwrap(),
                name,
            )
            .unwrap();
    }
    for name in ["wrong", "..", "other\\source"] {
        assert!(
            guard
                .check_before(
                    &plan,
                    folder.to_str().unwrap(),
                    target.to_str().unwrap(),
                    Some(name)
                )
                .is_err()
        );
    }
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}

#[test]
fn copy_callbacks_resolve_aliases_but_reject_unplanned_links_and_replaced_sources() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("source");
    let file = fixture.path().join("file.txt");
    let target = fixture.path().join("target");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(&file, b"original").unwrap();
    let plan = copy_plan(&[&folder, &file], &target).unwrap();
    let guard = super::copy_tree_sink::TreeCopyProgress::default();
    let alias = folder.join(".");
    guard
        .check_before(
            &plan,
            alias.to_str().unwrap(),
            target.to_str().unwrap(),
            None,
        )
        .unwrap();
    fs::create_dir(target.join("source")).unwrap();
    guard
        .check_after(
            &plan,
            alias.to_str().unwrap(),
            target.join("source").to_str().unwrap(),
        )
        .unwrap();

    let unplanned_link = fixture.path().join("unplanned.txt");
    fs::hard_link(&file, &unplanned_link).unwrap();
    fs::write(target.join("file.txt"), b"copied").unwrap();
    for source in [&unplanned_link, &file] {
        if source == &file {
            fs::rename(&file, fixture.path().join("held.txt")).unwrap();
            fs::write(&file, b"replacement").unwrap();
        }
        assert_eq!(
            guard
                .check_before(
                    &plan,
                    source.to_str().unwrap(),
                    target.to_str().unwrap(),
                    None
                )
                .unwrap_err()
                .code,
            crate::domain::FaultCode::StaleRequest
        );
        assert_eq!(
            guard
                .check_after(
                    &plan,
                    source.to_str().unwrap(),
                    target.join("file.txt").to_str().unwrap()
                )
                .unwrap_err()
                .code,
            crate::domain::FaultCode::StaleRequest
        );
    }
    assert_eq!(fs::read(unplanned_link).unwrap(), b"original");
    assert_eq!(fs::read(file).unwrap(), b"replacement");
    assert_eq!(fs::read(target.join("file.txt")).unwrap(), b"copied");
}

#[tokio::test]
#[ignore = "requires explicitly supplied fixture parent on another fixed volume"]
async fn native_directory_copy_across_volumes_preserves_sources_and_empty_folders() {
    let parent = std::env::var_os("PROJECTPANE_CROSS_VOLUME_FIXTURE")
        .expect("dedicated fixture parent required");
    let origin = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir_in(parent).unwrap();
    let folder = origin.path().join("한글 복사");
    fs::create_dir_all(folder.join("empty")).unwrap();
    let payload: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
    fs::write(folder.join("child.bin"), &payload).unwrap();
    let source = capture_source(folder.to_str().unwrap()).unwrap();
    assert_ne!(
        source.volume,
        capture_source(destination.path().to_str().unwrap())
            .unwrap()
            .volume
    );
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(
            copy_plan(&[&folder], destination.path()).unwrap(),
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.counts.changed, 3);
    assert_eq!(report.items.len(), 3);
    assert_eq!(capture_source(folder.to_str().unwrap()).unwrap(), source);
    assert_eq!(fs::read(folder.join("child.bin")).unwrap(), payload);
    assert_eq!(
        fs::read(destination.path().join("한글 복사/child.bin")).unwrap(),
        payload
    );
    assert!(folder.join("empty").is_dir());
    assert!(destination.path().join("한글 복사/empty").is_dir());
    report.validate().unwrap();
}
