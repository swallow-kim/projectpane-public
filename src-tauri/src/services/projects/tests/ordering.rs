use super::*;

fn ids(service: &ProjectService) -> Vec<String> {
    service
        .list()
        .unwrap()
        .projects
        .into_iter()
        .map(|project| project.id)
        .collect()
}
fn catalog(directory: &tempfile::TempDir) -> std::path::PathBuf {
    directory
        .path()
        .join("foundation/projects-v1/manifest.json")
}

#[test]
fn reorder_persists_only_manifest_order_and_preserves_selection_and_documents() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let a = create(&service, preset.clone());
    let b = create(&service, preset.clone());
    service
        .save_session(SessionRequest {
            project_id: a.project.id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: preset,
        })
        .unwrap();
    let settings = directory
        .path()
        .join("foundation/projects-v1/settings.json");
    let before_settings = std::fs::read(&settings).unwrap();
    let before_a = std::fs::read(document(&directory, &a.project.id, "project")).unwrap();
    let before_session = std::fs::read(document(&directory, &a.project.id, "session")).unwrap();
    let expected = ids(&service);
    let reverse = vec![b.project.id.clone(), a.project.id.clone()];
    let result = service
        .reorder(ReorderRequest {
            expected_order: expected,
            project_ids: reverse.clone(),
        })
        .unwrap();
    assert_eq!(result.project_ids, reverse);
    assert_eq!(ids(&service), reverse);
    assert_eq!(std::fs::read(&settings).unwrap(), before_settings);
    assert_eq!(
        std::fs::read(document(&directory, &a.project.id, "project")).unwrap(),
        before_a
    );
    assert_eq!(
        std::fs::read(document(&directory, &a.project.id, "session")).unwrap(),
        before_session
    );
    let reopened = ProjectService::new(ProjectStore::new(directory.path(), None).unwrap());
    assert_eq!(ids(&reopened), reverse);
    assert_eq!(reopened.list().unwrap().last_project_id, Some(b.project.id));
}

#[test]
fn reorder_rejects_stale_or_invalid_permutations_without_changing_manifest() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let a = create(&service, preset.clone());
    let b = create(&service, preset);
    let expected = ids(&service);
    let before = std::fs::read(catalog(&directory)).unwrap();
    for project_ids in [
        vec![a.project.id.clone()],
        vec![a.project.id.clone(), a.project.id.clone()],
        vec![
            a.project.id.clone(),
            "00000000-0000-0000-0000-000000000000".into(),
        ],
        vec![a.project.id.clone(), "../outside".into()],
    ] {
        assert_eq!(
            service
                .reorder(ReorderRequest {
                    expected_order: expected.clone(),
                    project_ids
                })
                .unwrap_err()
                .code,
            "invalid_document"
        );
        assert_eq!(std::fs::read(catalog(&directory)).unwrap(), before);
    }
    let reversed = vec![b.project.id, a.project.id];
    service
        .reorder(ReorderRequest {
            expected_order: expected.clone(),
            project_ids: reversed.clone(),
        })
        .unwrap();
    let after = std::fs::read(catalog(&directory)).unwrap();
    assert_eq!(
        service
            .reorder(ReorderRequest {
                expected_order: expected,
                project_ids: reversed
            })
            .unwrap_err()
            .code,
        "revision_conflict"
    );
    assert_eq!(std::fs::read(catalog(&directory)).unwrap(), after);
}

#[test]
fn reorder_refuses_catalog_membership_changes_and_write_failure_preserves_order() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    create(&service, preset.clone());
    create(&service, preset.clone());
    let stale = ids(&service);
    create(&service, preset);
    let before = std::fs::read(catalog(&directory)).unwrap();
    assert_eq!(
        service
            .reorder(ReorderRequest {
                expected_order: stale.clone(),
                project_ids: stale.into_iter().rev().collect()
            })
            .unwrap_err()
            .code,
        "revision_conflict"
    );
    assert_eq!(std::fs::read(catalog(&directory)).unwrap(), before);

    // Corrupt backup must not be overwritten merely to save a new order.
    let backup = catalog(&directory).with_extension("json.bak");
    std::fs::write(&backup, b"not a manifest").unwrap();
    let current = ids(&service);
    assert!(
        service
            .reorder(ReorderRequest {
                expected_order: current.clone(),
                project_ids: current.iter().rev().cloned().collect()
            })
            .is_err()
    );
    assert_eq!(ids(&service), current);
    assert_eq!(std::fs::read(catalog(&directory)).unwrap(), before);
    assert_eq!(std::fs::read(backup).unwrap(), b"not a manifest");
}

#[test]
fn reorder_noop_does_not_rewrite_or_require_a_backup() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    create(&service, preset);
    let expected = ids(&service);
    let before = std::fs::read(catalog(&directory)).unwrap();
    service
        .reorder(ReorderRequest {
            expected_order: expected.clone(),
            project_ids: expected.clone(),
        })
        .unwrap();
    assert_eq!(ids(&service), expected);
    assert_eq!(std::fs::read(catalog(&directory)).unwrap(), before);
    assert!(!catalog(&directory).with_extension("json.bak").exists());
}

#[test]
fn concurrent_reorders_cannot_overwrite_each_others_order() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, preset) = fixture();
    create(&service, preset.clone());
    create(&service, preset.clone());
    create(&service, preset);
    let expected = ids(&service);
    let first = vec![
        expected[2].clone(),
        expected[0].clone(),
        expected[1].clone(),
    ];
    let second = vec![
        expected[1].clone(),
        expected[2].clone(),
        expected[0].clone(),
    ];
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let run = |order: Vec<String>| {
            barrier.wait();
            service.reorder(ReorderRequest {
                expected_order: expected.clone(),
                project_ids: order,
            })
        };
        let first_order = first.clone();
        let a = scope.spawn(move || run(first_order));
        let second_order = second.clone();
        let b = scope.spawn(move || run(second_order));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_ne!(a.is_ok(), b.is_ok());
    let failure = if let Err(error) = &a {
        error
    } else {
        b.as_ref().unwrap_err()
    };
    assert!(matches!(failure.code, "busy" | "revision_conflict"));
    assert_eq!(ids(&service), if a.is_ok() { first } else { second });
}
