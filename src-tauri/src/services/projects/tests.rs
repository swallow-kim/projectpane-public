use super::*;
use crate::{domain::projects::*, persistence::projects::ProjectStore};

fn fixture() -> (tempfile::TempDir, ProjectService, Workspace) {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(directory.path(), None).unwrap();
    let project = parse_project(include_bytes!(
        "../../../../tests/fixtures/projects/project-v1.json"
    ))
    .unwrap();
    (directory, ProjectService::new(store), project.preset)
}
fn create(service: &ProjectService, preset: Workspace) -> Loaded {
    service
        .create(CreateRequest {
            name: "original".into(),
            root_path: "C:\\source".into(),
            preset,
        })
        .unwrap()
}
fn document(directory: &tempfile::TempDir, id: &str, kind: &str) -> std::path::PathBuf {
    directory
        .path()
        .join("foundation/projects-v1")
        .join(format!("{id}.{kind}.json"))
}
#[test]
fn create_load_and_session_keep_preset_separate() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, preset) = fixture();
    let created = service
        .create(CreateRequest {
            name: "한글".into(),
            root_path: "C:\\자료".into(),
            preset: preset.clone(),
        })
        .unwrap();
    let id = created.project.id.clone();
    let mut workspace = preset.clone();
    workspace.columns = 65.0;
    let saved = service
        .save_session(SessionRequest {
            project_id: id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: workspace.clone(),
        })
        .unwrap();
    assert_eq!(saved.session_revision, 1);
    let session = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Session,
        })
        .unwrap();
    assert_eq!(session.workspace, workspace);
    let original = service
        .load(LoadRequest {
            project_id: id,
            source: Source::Preset,
        })
        .unwrap();
    assert_eq!(original.workspace, preset);
    assert_eq!(original.session_revision, 1);
}

#[test]
fn rename_duplicate_remove_preserve_source_and_orphan_documents() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, service, preset) = fixture();
    let source = directory.path().join("user-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("keep.txt"), b"source bytes").unwrap();
    let project = service
        .create(CreateRequest {
            name: "original".into(),
            root_path: source.to_str().unwrap().into(),
            preset,
        })
        .unwrap();
    let renamed = service
        .rename(NameRequest {
            project_id: project.project.id.clone(),
            expected_revision: 0,
            name: "renamed".into(),
        })
        .unwrap();
    assert_eq!(renamed.revision, 1);
    let copy = service
        .duplicate(NameRequest {
            project_id: renamed.id.clone(),
            expected_revision: 1,
            name: "copy".into(),
        })
        .unwrap();
    assert_ne!(copy.project.id, renamed.id);
    assert_eq!(copy.project.root_path, renamed.root_path);
    let original = document(&directory, &renamed.id, "project");
    let bytes = std::fs::read(&original).unwrap();
    service
        .remove(RemoveRequest {
            project_id: renamed.id.clone(),
            expected_revision: 1,
        })
        .unwrap();
    assert_eq!(std::fs::read(original).unwrap(), bytes);
    assert_eq!(
        std::fs::read(source.join("keep.txt")).unwrap(),
        b"source bytes"
    );
    let catalog = service.list().unwrap();
    assert_eq!(catalog.projects.len(), 1);
    assert_eq!(catalog.last_project_id, Some(copy.project.id));
}

#[test]
fn session_and_project_compare_and_swap_reject_stale_concurrent_writes() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, preset) = fixture();
    let id = create(&service, preset.clone()).project.id;
    let service = std::sync::Arc::new(service);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [60.0, 70.0]
        .into_iter()
        .map(|columns| {
            let service = std::sync::Arc::clone(&service);
            let barrier = std::sync::Arc::clone(&barrier);
            let id = id.clone();
            let mut workspace = preset.clone();
            workspace.columns = columns;
            std::thread::spawn(move || {
                barrier.wait();
                service.save_session(SessionRequest {
                    project_id: id,
                    expected_project_revision: 0,
                    expected_session_revision: 0,
                    workspace,
                })
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let stale = service
        .save_session(SessionRequest {
            project_id: id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: preset.clone(),
        })
        .unwrap_err();
    assert_eq!(stale.code, "revision_conflict");
    service
        .save_preset(PresetRequest {
            project_id: id.clone(),
            expected_revision: 0,
            workspace: preset.clone(),
        })
        .unwrap();
    let stale = service
        .save_session(SessionRequest {
            project_id: id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 1,
            workspace: preset.clone(),
        })
        .unwrap_err();
    assert_eq!(stale.code, "revision_conflict");
    let stale = service
        .save_preset(PresetRequest {
            project_id: id,
            expected_revision: 0,
            workspace: preset,
        })
        .unwrap_err();
    assert_eq!(stale.code, "revision_conflict");
}

mod recovery;
mod safety;

#[test]
fn rename_and_preset_save_keep_distinct_last_session_workspace() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (_directory, service, preset) = fixture();
    let id = create(&service, preset.clone()).project.id;
    let mut workspace = preset.clone();
    workspace.columns = 67.0;
    service
        .save_session(SessionRequest {
            project_id: id.clone(),
            expected_project_revision: 0,
            expected_session_revision: 0,
            workspace: workspace.clone(),
        })
        .unwrap();
    service
        .rename(NameRequest {
            project_id: id.clone(),
            expected_revision: 0,
            name: "renamed".into(),
        })
        .unwrap();
    let after_rename = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Session,
        })
        .unwrap();
    assert_eq!(after_rename.workspace, workspace);
    service
        .save_preset(PresetRequest {
            project_id: id.clone(),
            expected_revision: 1,
            workspace: preset.clone(),
        })
        .unwrap();
    let after_preset = service
        .load(LoadRequest {
            project_id: id.clone(),
            source: Source::Session,
        })
        .unwrap();
    assert_eq!(after_preset.workspace, workspace);
    assert!(!after_preset.read_only);
    service
        .save_session(SessionRequest {
            project_id: id,
            expected_project_revision: 2,
            expected_session_revision: 1,
            workspace,
        })
        .unwrap();
}
