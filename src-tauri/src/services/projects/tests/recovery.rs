use super::*;
#[test]
fn future_unknown_corrupt_originals_and_settings_remain_byte_identical() {
    let _guard = TEST_LOCK.lock().unwrap();
    for damage in ["future", "unknown", "corrupt"] {
        let (directory, service, preset) = fixture();
        let created = create(&service, preset.clone());
        let id = created.project.id;
        let path = document(&directory, &id, "project");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let damaged = match damage {
            "future" => {
                value["schemaVersion"] = 2.into();
                serde_json::to_vec(&value).unwrap()
            }
            "unknown" => {
                value["unrecognized"] = true.into();
                serde_json::to_vec(&value).unwrap()
            }
            _ => b"{broken".to_vec(),
        };
        std::fs::write(&path, &damaged).unwrap();
        assert!(
            service
                .save_session(SessionRequest {
                    project_id: id.clone(),
                    expected_project_revision: 0,
                    expected_session_revision: 0,
                    workspace: preset.clone()
                })
                .is_err()
        );
        assert!(
            service
                .rename(NameRequest {
                    project_id: id.clone(),
                    expected_revision: 0,
                    name: "bad".into()
                })
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), damaged);
        assert!(!path.with_extension("json.bak").exists());
        assert_eq!(service.list().unwrap().warnings.len(), 1);
        let settings = directory
            .path()
            .join("foundation/projects-v1/settings.json");
        let future = br#"{"schemaVersion":2,"lastProjectId":null,"future":true}"#;
        std::fs::write(&settings, future).unwrap();
        assert!(
            service
                .create(CreateRequest {
                    name: "blocked".into(),
                    root_path: "C:\\source".into(),
                    preset
                })
                .is_err()
        );
        assert_eq!(std::fs::read(settings).unwrap(), future);
    }
}

#[test]
fn repaired_workspace_is_read_only_and_can_be_saved_as_new() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let created = create(&service, preset.clone());
    let id = created.project.id;
    let path = document(&directory, &id, "project");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["preset"]["columns"] = 0.into();
    let original = serde_json::to_vec(&value).unwrap();
    std::fs::write(&path, &original).unwrap();
    let recovered = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Preset,
        })
        .unwrap();
    assert!(recovered.read_only);
    assert_eq!(recovered.workspace.columns, 50.0);
    assert!(
        service
            .save_session(SessionRequest {
                project_id: id,
                expected_project_revision: 0,
                expected_session_revision: 0,
                workspace: preset
            })
            .is_err()
    );
    let copied = service
        .create(CreateRequest {
            name: "recovered copy".into(),
            root_path: recovered.project.root_path,
            preset: recovered.workspace,
        })
        .unwrap();
    assert!(!copied.read_only);
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn validated_backup_recovers_without_overwriting_corrupt_original() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let created = create(&service, preset);
    let id = created.project.id;
    service
        .rename(NameRequest {
            project_id: id.clone(),
            expected_revision: 0,
            name: "new".into(),
        })
        .unwrap();
    let path = document(&directory, &id, "project");
    let backup = std::fs::read(path.with_extension("json.bak")).unwrap();
    std::fs::write(&path, b"corrupt").unwrap();
    let recovered = service
        .load(LoadRequest {
            project_id: id,
            source: Source::Preset,
        })
        .unwrap();
    assert!(recovered.read_only);
    assert_eq!(recovered.project.name, "original");
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
    assert_eq!(
        std::fs::read(path.with_extension("json.bak")).unwrap(),
        backup
    );
}

#[test]
fn future_session_blocks_autosave_and_returns_read_only_preset() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let created = create(&service, preset.clone());
    let id = created.project.id;
    let path = document(&directory, &id, "session");
    let original = br#"{"schemaVersion":2,"futureSessionField":true}"#;
    std::fs::write(&path, original).unwrap();
    let loaded = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Session,
        })
        .unwrap();
    assert!(loaded.read_only);
    assert_eq!(loaded.source, Source::Preset);
    let failure = service
        .save_session(SessionRequest {
            project_id: id,
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: preset,
        })
        .unwrap_err();
    assert_eq!(failure.code, "unsupported_schema");
    assert_eq!(std::fs::read(path).unwrap(), original);
}
