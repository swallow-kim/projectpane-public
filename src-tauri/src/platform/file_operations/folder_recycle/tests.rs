use super::*;

#[test]
fn prepared_tree_rejects_forged_or_missing_descendants() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("child"), b"keep").unwrap();
    fs::write(fixture.path().join("outside"), b"outside").unwrap();
    let snapshot = FolderRecycleSnapshot::capture(root.to_str().unwrap()).unwrap();
    let original = snapshot.descendants().to_vec();
    let outside = capture_source(fixture.path().join("outside").to_str().unwrap()).unwrap();
    for descendants in [vec![], vec![outside], vec![original[0].clone(); 2]] {
        assert!(FolderRecycleSnapshot::from_prepared(snapshot.root(), &descendants).is_err());
    }
    let action = snapshot.into_action();
    let plan = OperationPlan {
        project_id: None,
        action,
    };
    super::super::check_plan(&plan).unwrap();
    fs::write(root.join("extra"), b"extra").unwrap();
    assert!(super::super::check_plan(&plan).is_err());
    assert_eq!(fs::read(root.join("child")).unwrap(), b"keep");
}

#[test]
fn nested_snapshot_preserves_files_and_rejects_membership_changes() {
    for change in ["add", "remove", "replace"] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("한글 폴더");
        fs::create_dir_all(root.join("nested/empty")).unwrap();
        let file = root.join("nested/payload");
        fs::write(&file, b"original").unwrap();
        let snapshot = FolderRecycleSnapshot::capture(root.to_str().unwrap()).unwrap();
        assert_eq!(snapshot.descendants().len(), 3);
        assert_eq!(
            snapshot.root(),
            &capture_source(root.to_str().unwrap()).unwrap()
        );
        assert!(bounded_json(&snapshot).unwrap().len() < MAX_FRAME);
        snapshot.revalidate().unwrap();
        if change == "add" {
            fs::write(root.join("new"), b"new").unwrap();
        } else {
            fs::rename(&file, fixture.path().join("held")).unwrap();
            if change == "replace" {
                fs::write(&file, b"replacement").unwrap();
            }
        }
        assert!(snapshot.revalidate().is_err(), "{change}");
        let preserved = if change == "add" {
            file
        } else {
            fixture.path().join("held")
        };
        assert_eq!(fs::read(preserved).unwrap(), b"original");
        assert!(root.join("nested/empty").is_dir());
    }
}

#[test]
fn root_replacement_and_non_folder_inputs_are_rejected() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("source");
    fs::create_dir(&root).unwrap();
    let snapshot = FolderRecycleSnapshot::capture(root.to_str().unwrap()).unwrap();
    fs::rename(&root, fixture.path().join("held")).unwrap();
    fs::create_dir(&root).unwrap();
    assert_eq!(
        snapshot.revalidate().unwrap_err().code,
        FaultCode::StaleRequest
    );
    let file = root.join("file");
    fs::write(&file, b"keep").unwrap();
    assert_eq!(
        FolderRecycleSnapshot::capture(file.to_str().unwrap())
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    let drive = root.ancestors().last().unwrap();
    assert_eq!(
        FolderRecycleSnapshot::capture(drive.to_str().unwrap())
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    assert_eq!(fs::read(file).unwrap(), b"keep");
}

#[test]
fn long_names_hit_document_budget_before_entry_limit_without_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let suffix = "한글".repeat(40);
    for index in 0..1024 {
        fs::write(fixture.path().join(format!("{index:04}-{suffix}")), b"keep").unwrap();
    }
    let path = fixture.path().to_str().unwrap();
    assert_eq!(
        FolderRecycleSnapshot::capture(path).unwrap_err().code,
        FaultCode::PayloadLimit
    );
    let entries: Vec<_> = fs::read_dir(fixture.path()).unwrap().collect();
    assert_eq!(entries.len(), 1024);
    for entry in entries {
        assert_eq!(fs::read(entry.unwrap().path()).unwrap(), b"keep");
    }
}

#[test]
fn relocation_evidence_requires_root_children_and_original_absence() {
    for damage in ["none", "add", "replace", "original-reappears", "wrong-root"] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("한글-source");
        let target = fixture.path().join("relocated");
        fs::create_dir_all(root.join("nested/empty")).unwrap();
        fs::write(root.join("nested/payload"), b"preserved payload").unwrap();
        let snapshot = FolderRecycleSnapshot::capture(root.to_str().unwrap()).unwrap();
        fs::rename(&root, &target).unwrap();
        match damage {
            "add" => fs::write(target.join("extra"), b"unplanned").unwrap(),
            "replace" => {
                fs::rename(target.join("nested/payload"), fixture.path().join("held")).unwrap();
                fs::write(target.join("nested/payload"), b"replacement").unwrap();
            }
            "original-reappears" => fs::create_dir(&root).unwrap(),
            "wrong-root" => {
                fs::rename(&target, fixture.path().join("held-tree")).unwrap();
                fs::create_dir_all(target.join("nested/empty")).unwrap();
                fs::write(target.join("nested/payload"), b"preserved payload").unwrap();
            }
            "none" => {}
            _ => unreachable!(),
        }
        let actual = capture_source(target.to_str().unwrap()).unwrap();
        let result = snapshot.confirm_relocated(actual);
        assert_eq!(result.is_ok(), damage == "none", "{damage}");
        assert_eq!(
            snapshot
                .confirm_recycled(target.to_str().unwrap())
                .unwrap_err()
                .code,
            FaultCode::InvalidPath
        );
        let preserved = match damage {
            "replace" => fixture.path().join("held"),
            "wrong-root" => fixture.path().join("held-tree/nested/payload"),
            _ => target.join("nested/payload"),
        };
        assert_eq!(fs::read(preserved).unwrap(), b"preserved payload");
    }
}
