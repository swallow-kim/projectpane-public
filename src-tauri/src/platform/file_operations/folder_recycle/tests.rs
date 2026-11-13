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

#[test]
fn batch_preparation_captures_files_and_multiple_folders_and_rejects_stale_descendants() {
    let fixture = tempfile::tempdir().unwrap();
    let first = fixture.path().join("first.txt");
    let one = fixture.path().join("one");
    let two = fixture.path().join("two");
    fs::write(&first, b"file").unwrap();
    fs::create_dir_all(one.join("nested")).unwrap();
    fs::create_dir(&two).unwrap();
    fs::write(one.join("nested/child"), b"nested").unwrap();
    let sources: Vec<_> = [&first, &one, &two]
        .into_iter()
        .map(|path| capture_source(path.to_str().unwrap()).unwrap())
        .collect();
    let action = prepare(sources.clone()).unwrap();
    let plan = OperationPlan {
        project_id: None,
        action,
    };
    assert!(
        matches!(&plan.action, PreparedAction::RecycleBatch { roots, descendants } if roots.len() == 3 && descendants.len() == 2)
    );
    super::super::check_plan(&plan).unwrap();
    fs::write(two.join("new-child"), b"late").unwrap();
    assert!(super::super::check_plan(&plan).is_err());
    assert_eq!(fs::read(first).unwrap(), b"file");
    assert_eq!(fs::read(one.join("nested/child")).unwrap(), b"nested");
    assert!(
        prepare(vec![
            sources[1].clone(),
            capture_source(one.join("nested").to_str().unwrap()).unwrap()
        ])
        .is_err()
    );
}

#[test]
fn batch_preparation_preserves_file_only_compatibility_and_rejects_invalid_roots() {
    let fixture = tempfile::tempdir().unwrap();
    let file = fixture.path().join("file");
    fs::write(&file, b"keep").unwrap();
    let source = capture_source(file.to_str().unwrap()).unwrap();
    assert!(matches!(
        prepare(vec![source.clone()]).unwrap(),
        PreparedAction::Recycle { .. }
    ));
    let mut nonlocal = capture_source(fixture.path().to_str().unwrap()).unwrap();
    nonlocal.path = r"\\server\share\folder".into();
    assert!(check_root(&nonlocal).is_err());
    let mut bin = nonlocal;
    bin.path = r"C:\$Recycle.Bin\folder".into();
    assert!(check_root(&bin).is_err());
    let duplicate = OperationPlan {
        project_id: None,
        action: prepare(vec![source.clone(), source]).unwrap(),
    };
    assert!(super::super::check_plan(&duplicate).is_err());
}

#[test]
fn recycle_request_prepares_single_and_mixed_folder_selections_without_deleting() {
    use crate::domain::{ActivationKind, ActivationTarget, Identity};
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("folder");
    let file = fixture.path().join("file");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("child"), b"nested").unwrap();
    fs::write(&file, b"file").unwrap();
    for mixed in [false, true] {
        let mut targets = vec![ActivationTarget {
            path: folder.to_str().unwrap().into(),
            kind: ActivationKind::Folder,
        }];
        if mixed {
            targets.push(ActivationTarget {
                path: file.to_str().unwrap().into(),
                kind: ActivationKind::File,
            });
        }
        let request = PrepareOperation {
            project_id: None,
            action: OperationAction::Recycle {
                selection: OperationSelection {
                    identity: Identity {
                        pane: 0,
                        tab_id: "tab-1".into(),
                        request_id: "request-1".into(),
                        generation: 1,
                    },
                    entry_ids: (0..targets.len())
                        .map(|index| format!("entry-{index}"))
                        .collect(),
                },
            },
        };
        let plan = super::super::prepare::prepare(request, targets).unwrap();
        assert!(
            matches!(&plan.action, PreparedAction::RecycleBatch { roots, .. } if roots.len() == if mixed { 2 } else { 1 })
        );
        super::super::check_plan(&plan).unwrap();
        assert_eq!(fs::read(folder.join("child")).unwrap(), b"nested");
        assert_eq!(fs::read(&file).unwrap(), b"file");
    }
}
