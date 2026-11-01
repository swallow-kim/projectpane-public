use super::*;
#[test]
fn os_catalog_lock_fails_fast_without_mutation() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let lock_path = directory.path().join("foundation/projects-v1/catalog.lock");
    let held = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .unwrap();
    held.try_lock().unwrap();
    let failure = service
        .create(CreateRequest {
            name: "busy".into(),
            root_path: "C:\\source".into(),
            preset,
        })
        .unwrap_err();
    assert_eq!(failure.code, "busy");
    assert!(
        !directory
            .path()
            .join("foundation/projects-v1/manifest.json")
            .exists()
    );
}

#[test]
fn manifest_sharing_failure_keeps_selection_and_recoverable_orphan() {
    use std::os::windows::fs::OpenOptionsExt;
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let first = create(&service, preset.clone());
    let catalog_path = directory
        .path()
        .join("foundation/projects-v1/manifest.json");
    let original = std::fs::read(&catalog_path).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&catalog_path)
        .unwrap();
    assert!(
        service
            .create(CreateRequest {
                name: "orphan".into(),
                root_path: "C:\\source".into(),
                preset
            })
            .is_err()
    );
    assert_eq!(std::fs::read(&catalog_path).unwrap(), original);
    drop(held);
    let catalog = service.list().unwrap();
    assert_eq!(catalog.projects.len(), 1);
    assert_eq!(catalog.last_project_id, Some(first.project.id));
    let preserved = std::fs::read_dir(catalog_path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".project.json")
        })
        .count();
    assert_eq!(preserved, 2);
}

#[test]
fn invalid_names_roots_and_project_identifiers_have_no_disk_side_effect() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    for root in [
        "C:relative",
        "C:\\source\\..\\..\\escape",
        "\\\\?\\C:\\device",
    ] {
        assert!(
            service
                .create(CreateRequest {
                    name: "project".into(),
                    root_path: root.into(),
                    preset: preset.clone()
                })
                .is_err()
        );
    }
    assert!(
        service
            .create(CreateRequest {
                name: " ".into(),
                root_path: "C:\\source".into(),
                preset
            })
            .is_err()
    );
    assert!(
        service
            .load(LoadRequest {
                project_id: "../../outside".into(),
                source: Source::Preset
            })
            .is_err()
    );
    assert!(
        !directory
            .path()
            .join("foundation/projects-v1/manifest.json")
            .exists()
    );
}

#[test]
fn changed_root_and_future_session_revision_are_not_applied() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let id = create(&service, preset.clone()).project.id;
    let session_path = document(&directory, &id, "session");
    let mut workspace = preset.clone();
    workspace.columns = 66.0;
    let future = Session {
        schema_version: 1,
        project_id: id.clone(),
        project_revision: 1,
        revision: 1,
        workspace: workspace.clone(),
    };
    let session_bytes = serialize_session(&future).unwrap();
    std::fs::write(&session_path, &session_bytes).unwrap();
    let loaded = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Session,
        })
        .unwrap();
    assert!(loaded.read_only);
    assert_eq!(loaded.workspace, preset);
    assert!(
        service
            .save_session(SessionRequest {
                project_id: id.clone(),
                expected_project_revision: 0,
                expected_session_revision: 1,
                workspace
            })
            .is_err()
    );
    assert_eq!(std::fs::read(session_path).unwrap(), session_bytes);
    let path = document(&directory, &id, "project");
    let mut project = parse_project(&std::fs::read(&path).unwrap()).unwrap();
    project.root_path = "C:\\changed-root".into();
    let changed_bytes = serialize_project(&project).unwrap();
    std::fs::write(&path, &changed_bytes).unwrap();
    assert!(
        service
            .load(LoadRequest {
                project_id: id.clone(),
                source: Source::Session
            })
            .is_err()
    );
    assert!(
        service
            .rename(NameRequest {
                project_id: id,
                expected_revision: 0,
                name: "blocked".into()
            })
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), changed_bytes);
}
