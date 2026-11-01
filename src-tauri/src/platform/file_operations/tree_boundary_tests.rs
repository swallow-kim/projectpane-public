use super::{directory_copy_tests::copy_plan, *};
use crate::domain::{FaultCode, operations::*};
use std::fs;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir_all(folder.join("child/empty")).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(folder.join("child/payload"), b"source sentinel").unwrap();
    (root, folder, target)
}

#[test]
fn tree_copy_preflight_rejects_added_replaced_and_missing_descendants() {
    for change in [0, 1, 2] {
        let (root, folder, target) = fixture();
        let plan = copy_plan(&[&folder], &target).unwrap();
        if change == 0 {
            fs::write(folder.join("extra"), b"unplanned").unwrap();
        } else {
            fs::rename(folder.join("child/payload"), root.path().join("held")).unwrap();
            if change == 1 {
                fs::write(folder.join("child/payload"), b"replacement").unwrap();
            }
        }
        assert!(check_plan(&plan).is_err(), "{change}");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        if change != 0 {
            assert_eq!(
                fs::read(root.path().join("held")).unwrap(),
                b"source sentinel"
            );
        }
    }
}

#[test]
fn tree_copy_rejects_overlap_descendant_destination_and_collision() {
    let (_root, folder, target) = fixture();
    for destination in [&folder, &folder.join("child")] {
        assert_eq!(
            copy_plan(&[&folder], destination).unwrap_err().code,
            FaultCode::InvalidRequest
        );
    }
    assert_eq!(
        copy_plan(&[&folder, &folder.join("child/payload")], &target)
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    fs::create_dir(target.join("source")).unwrap();
    fs::write(target.join("source/sentinel"), b"destination sentinel").unwrap();
    assert_eq!(
        copy_plan(&[&folder], &target).unwrap_err().code,
        FaultCode::DestinationExists
    );
    assert_eq!(
        fs::read(target.join("source/sentinel")).unwrap(),
        b"destination sentinel"
    );
    assert_eq!(
        fs::read(folder.join("child/payload")).unwrap(),
        b"source sentinel"
    );
}

#[test]
fn tree_copy_callback_refuses_unplanned_source_and_unowned_destination_parent() {
    let (_root, folder, target) = fixture();
    let plan = copy_plan(&[&folder], &target).unwrap();
    let guard = super::copy_tree_sink::TreeCopyProgress::default();
    fs::write(folder.join("extra"), b"unplanned").unwrap();
    assert!(
        guard
            .check_before(
                &plan,
                folder.join("extra").to_str().unwrap(),
                target.to_str().unwrap(),
                None
            )
            .is_err()
    );
    let parent = target.join("source/child");
    fs::create_dir_all(&parent).unwrap();
    assert!(
        guard
            .check_before(
                &plan,
                folder.join("child/payload").to_str().unwrap(),
                parent.to_str().unwrap(),
                None
            )
            .is_err()
    );
    assert!(!parent.join("payload").exists());
}

#[test]
fn tree_copy_manifest_and_result_budgets_refuse_oversized_tree_before_mutation() {
    let (_root, folder, target) = fixture();
    for index in 0..MAX_OPERATION_RESULTS {
        fs::write(folder.join(format!("item-{index:04}")), b"").unwrap();
    }
    let code = copy_plan(&[&folder], &target).unwrap_err().code;
    assert!(matches!(
        code,
        FaultCode::PayloadLimit | FaultCode::ResourceLimit
    ));
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
}

#[test]
fn tree_copy_refuses_junction_escape_and_preserves_outside_sentinel() {
    use std::os::windows::process::CommandExt;
    let (root, folder, target) = fixture();
    let outside = root.path().join("outside-owned");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), b"outside unchanged").unwrap();
    let before = capture_source(outside.join("sentinel").to_str().unwrap()).unwrap();
    let created = std::process::Command::new("pwsh").args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:PROJECTPANE_TEST_JUNCTION -Value $env:PROJECTPANE_TEST_TARGET | Out-Null"])
        .env("PROJECTPANE_TEST_JUNCTION", folder.join("junction"))
        .env("PROJECTPANE_TEST_TARGET", &outside).creation_flags(0x08000000).output().unwrap();
    assert!(
        created.status.success(),
        "owned junction fixture creation failed"
    );
    assert_eq!(
        copy_plan(&[&folder], &target).unwrap_err().code,
        FaultCode::InvalidPath
    );
    assert_eq!(
        capture_source(outside.join("sentinel").to_str().unwrap()).unwrap(),
        before
    );
    assert_eq!(
        fs::read(outside.join("sentinel")).unwrap(),
        b"outside unchanged"
    );
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}

#[test]
fn tree_copy_rejects_duplicate_root_names_from_separate_source_parents() {
    let (root, folder, target) = fixture();
    let other = root.path().join("other/source");
    fs::create_dir_all(&other).unwrap();
    assert_eq!(
        copy_plan(&[&folder, &other], &target).unwrap_err().code,
        FaultCode::InvalidRequest
    );
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}
