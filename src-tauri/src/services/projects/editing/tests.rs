use super::*;
use crate::persistence::projects::ProjectStore;

fn fixture() -> (tempfile::TempDir, ProjectService, Loaded) {
    let directory = tempfile::tempdir().unwrap();
    let service = ProjectService::new(ProjectStore::new(directory.path(), None).unwrap());
    let preset = parse_project(include_bytes!(
        "../../../../../tests/fixtures/projects/project-v1.json"
    ))
    .unwrap()
    .preset;
    let created = service
        .create(CreateRequest {
            name: "original".into(),
            root_path: "C:\\source".into(),
            preset,
        })
        .unwrap();
    service
        .save_session(SessionRequest {
            project_id: created.project.id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: created.workspace.clone(),
        })
        .unwrap();
    (directory, service, created)
}
fn paths(root: &str, workspace: &Workspace) -> Vec<Option<String>> {
    workspace
        .panes
        .iter()
        .flat_map(|p| &p.tabs)
        .map(|t| resolve_path_ref(root, &t.path).unwrap())
        .collect()
}
#[test]
fn edit_rebases_both_workspaces_preserves_paths_and_allows_later_saves() {
    let _guard = TEST_LOCK.lock().unwrap();
    for root in [
        "D:\\new",
        "\\\\server\\share\\new",
        "\\\\wsl.localhost\\Ubuntu\\home\\Me",
    ] {
        let (_directory, service, created) = fixture();
        let original_paths = paths(&created.project.root_path, &created.workspace);
        let result = service
            .edit(EditRequest {
                project_id: created.project.id.clone(),
                expected_revision: 0,
                name: "edited".into(),
                root_path: root.into(),
            })
            .unwrap();
        assert_eq!(result.project.root_path, root);
        assert_eq!(result.project.revision, 1);
        assert_eq!(result.session_revision, 2);
        assert_eq!(paths(root, &result.workspace), original_paths);
        let preset = service
            .load(LoadRequest {
                project_id: created.project.id.clone(),
                source: Source::Preset,
            })
            .unwrap();
        assert_eq!(paths(root, &preset.workspace), original_paths);
        service
            .rename(NameRequest {
                project_id: created.project.id.clone(),
                expected_revision: 1,
                name: "again".into(),
            })
            .unwrap();
        service
            .save_session(SessionRequest {
                project_id: created.project.id.clone(),
                expected_project_revision: 2,
                expected_session_revision: 2,
                workspace: result.workspace,
            })
            .unwrap();
        assert_eq!(
            service
                .edit(EditRequest {
                    project_id: created.project.id,
                    expected_revision: 0,
                    name: "stale".into(),
                    root_path: "E:\\wrong".into()
                })
                .unwrap_err()
                .code,
            "revision_conflict"
        );
    }
}
#[test]
fn unfinished_root_edit_recovers_at_every_document_boundary() {
    let _guard = TEST_LOCK.lock().unwrap();
    for stage in 0..=6 {
        let (directory, service, created) = fixture();
        let old_paths = paths(&created.project.root_path, &created.workspace);
        service
            .store
            .transaction(|tx| {
                let intent = Intent {
                    schema_version: 1,
                    id: created.project.id.clone(),
                    revision: 0,
                    old_root: "C:\\source".into(),
                    new_root: "D:\\target".into(),
                    name: "recovered".into(),
                    session_revision: Some(1),
                };
                let (project, session, catalog) = prepare(tx, &intent)?;
                journal(tx, Some(&intent))?;
                for step in 0..stage {
                    match step % 3 {
                        0 => {
                            let bytes = serialize_session(session.as_ref().unwrap()).unwrap();
                            tx.write(&format!("{}.session.json", intent.id), &bytes, |b| {
                                parse_session(b).map(|_| ()).map_err(contract)
                            })?;
                        }
                        1 => write_project(tx, &project)?,
                        _ => write_manifest(tx, &catalog)?,
                    }
                }
                Ok(())
            })
            .unwrap();
        // Fresh service instance exercises restart recovery, not in-memory state.
        let restarted = ProjectService::new(ProjectStore::new(directory.path(), None).unwrap());
        let loaded = restarted
            .load(LoadRequest {
                project_id: created.project.id.clone(),
                source: Source::Session,
            })
            .unwrap();
        assert!(!loaded.read_only);
        assert_eq!(loaded.project.root_path, "D:\\target");
        assert_eq!(loaded.session_revision, 2);
        assert_eq!(
            paths(&loaded.project.root_path, &loaded.workspace),
            old_paths
        );
        restarted
            .store
            .transaction(|tx| {
                assert!(parse(&tx.read(JOURNAL)?.unwrap())?.is_none());
                let backup = parse_session(
                    &tx.read_backup(&format!("{}.session.json", created.project.id))?
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(
                    paths(&loaded.project.root_path, &backup.workspace),
                    old_paths
                );
                Ok(())
            })
            .unwrap();
    }
}
#[test]
fn invalid_edit_leaves_documents_unchanged() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, created) = fixture();
    for (name, root) in [
        ("", "D:\\target"),
        ("valid", "relative"),
        ("valid", "C:\\..\\bad"),
    ] {
        assert!(
            service
                .edit(EditRequest {
                    project_id: created.project.id.clone(),
                    expected_revision: 0,
                    name: name.into(),
                    root_path: root.into()
                })
                .is_err()
        );
        let loaded = service
            .load(LoadRequest {
                project_id: created.project.id.clone(),
                source: Source::Session,
            })
            .unwrap();
        assert_eq!(loaded.project, created.project);
        assert_eq!(loaded.session_revision, 1);
    }
}

#[test]
fn case_only_wsl_root_change_never_redirects_existing_tabs() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, created) = fixture();
    let first = service
        .edit(EditRequest {
            project_id: created.project.id.clone(),
            expected_revision: 0,
            name: "wsl".into(),
            root_path: "\\\\wsl.localhost\\Ubuntu\\home\\User".into(),
        })
        .unwrap();
    let mut workspace = first.workspace;
    workspace.panes[0].tabs[0].path = PathRef::Relative {
        value: "src".into(),
    };
    service
        .save_session(SessionRequest {
            project_id: created.project.id.clone(),
            expected_project_revision: 1,
            expected_session_revision: 2,
            workspace,
        })
        .unwrap();
    let second = service
        .edit(EditRequest {
            project_id: created.project.id,
            expected_revision: 1,
            name: "wsl".into(),
            root_path: "\\\\wsl.localhost\\Ubuntu\\home\\user".into(),
        })
        .unwrap();
    assert_eq!(
        resolve_path_ref(
            &second.project.root_path,
            &second.workspace.panes[0].tabs[0].path
        )
        .unwrap(),
        Some("\\\\wsl.localhost\\Ubuntu\\home\\User\\src".into())
    );
}

#[test]
fn legacy_project_without_session_can_change_root() {
    let _guard = TEST_LOCK.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let service = ProjectService::new(ProjectStore::new(directory.path(), None).unwrap());
    let preset = parse_project(include_bytes!(
        "../../../../../tests/fixtures/projects/project-v1.json"
    ))
    .unwrap()
    .preset;
    let original = service
        .create(CreateRequest {
            name: "legacy".into(),
            root_path: "c:/source/./child".into(),
            preset,
        })
        .unwrap();
    let edited = service
        .edit(EditRequest {
            project_id: original.project.id,
            expected_revision: 0,
            name: "updated".into(),
            root_path: "D:/target/./".into(),
        })
        .unwrap();
    assert_eq!(edited.source, Source::Preset);
    assert_eq!(edited.session_revision, 0);
    assert_eq!(edited.project.root_path, "D:\\target");
    assert_eq!(
        paths(&edited.project.root_path, &edited.workspace),
        paths("C:\\source\\child", &original.workspace)
    );
}
