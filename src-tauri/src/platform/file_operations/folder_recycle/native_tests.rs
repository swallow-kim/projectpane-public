use super::super::{NativeFileOperationLane, execute, sink, sink_state};
use super::*;
use sha2::{Digest, Sha256};
use std::sync::{Arc, atomic::AtomicBool};
use windows::{
    Win32::UI::Shell::{
        IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
        TSF_DELETE_RECYCLE_IF_POSSIBLE,
    },
    core::{HRESULT, HSTRING},
};

fn plan(root: &Path) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action: FolderRecycleSnapshot::capture(root.to_str().unwrap())
            .unwrap()
            .into_action(),
    }
}

fn shell(path: &Path) -> IShellItem {
    // SAFETY: COM category 8: callers retain the STA guard and the UTF-16 path
    // outlives this synchronous call; the returned interface stays on this STA.
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }.unwrap()
}

#[test]
fn folder_callbacks_refuse_children_stale_trees_and_missing_destination() {
    let _apartment = execute::Apartment::initialize().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    let child = root.join("child");
    fs::write(&child, b"keep").unwrap();
    let context = sink_state::SinkContext::new(1, Arc::new(AtomicBool::new(false)), Arc::default());
    let state = std::rc::Rc::clone(&context.state);
    let sink: IFileOperationProgressSink = sink::ProgressSink {
        context,
        plan: plan(&root),
        tree: Default::default(),
        recycle: Default::default(),
        moving_tree: Default::default(),
    }
    .into();
    let flag = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
    // SAFETY: COM category 8: owned STA interfaces; these direct callbacks do
    // not dispatch deletion. Null destination intentionally represents absent proof.
    unsafe {
        assert!(sink.PreDeleteItem(0, &shell(&root)).is_err());
        assert!(sink.PreDeleteItem(flag, &shell(&child)).is_err());
        fs::write(root.join("extra"), b"extra").unwrap();
        assert!(sink.PreDeleteItem(flag, &shell(&root)).is_err());
        fs::rename(root.join("extra"), fixture.path().join("held")).unwrap();
        assert!(sink.PreDeleteItem(flag, &shell(&root)).is_ok());
        assert!(sink.PreDeleteItem(flag, &shell(&root)).is_err());
        assert!(
            sink.PostDeleteItem(flag, &shell(&root), HRESULT(0), None)
                .is_err()
        );
    }
    assert_eq!(state.borrow().report.counts.unknown, 1);
    assert_eq!(state.borrow().report.counts.changed, 0);
    assert_eq!(fs::read(child).unwrap(), b"keep");
}

#[tokio::test]
#[ignore = "requires fresh PROJECTPANE_FOLDER_RECYCLE_FIXTURE; recycles only its owned folder"]
async fn real_folder_recycle_preserves_nested_identities_hashes_and_sentinel() {
    let fixture = std::path::PathBuf::from(
        std::env::var_os("PROJECTPANE_FOLDER_RECYCLE_FIXTURE").expect("explicit owned fixture"),
    );
    assert!(fixture.is_absolute() && !fixture.exists());
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("p4-folder-recycle-")
    );
    assert!(
        !fixture
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    );
    capture_source(fixture.parent().unwrap().to_str().unwrap()).unwrap();
    fs::create_dir(&fixture).unwrap();
    let root = fixture.join("한글 폴더");
    fs::create_dir_all(root.join("중첩/empty")).unwrap();
    let payload: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
    fs::write(root.join("중첩/payload"), &payload).unwrap();
    fs::write(fixture.join("sentinel"), b"untouched").unwrap();
    let snapshot = FolderRecycleSnapshot::capture(root.to_str().unwrap()).unwrap();
    let operation = plan(&root);
    let lane = NativeFileOperationLane::start().unwrap();
    let cancelled = lane
        .execute(operation.clone(), 0, Arc::new(AtomicBool::new(true)))
        .await
        .unwrap();
    assert_eq!(cancelled.status(), OperationStatus::Cancelled);
    assert_eq!(fs::read(root.join("중첩/payload")).unwrap(), payload);
    fs::write(root.join("extra"), b"held").unwrap();
    assert!(
        lane.execute(operation.clone(), 0, Arc::new(AtomicBool::new(false)))
            .await
            .is_err()
    );
    fs::rename(root.join("extra"), fixture.join("held")).unwrap();
    let result = lane
        .execute(operation, 0, Arc::new(AtomicBool::new(false)))
        .await;
    fs::write(
        fixture.join("native-result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    lane.shutdown().await.unwrap();
    let report = result.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.counts.changed, 1);
    assert_eq!(report.items.len(), 1);
    assert!(report.evidence.outcomes_complete);
    let destination = report.items[0].destination.as_ref().unwrap();
    snapshot.confirm_recycled(destination).unwrap();
    let moved = fs::read(Path::new(destination).join("중첩/payload")).unwrap();
    assert_eq!(Sha256::digest(&moved), Sha256::digest(&payload));
    assert!(Path::new(destination).join("중첩/empty").is_dir());
    assert_eq!(fs::read(fixture.join("sentinel")).unwrap(), b"untouched");
    assert_eq!(fs::read(fixture.join("held")).unwrap(), b"held");
    let evidence = serde_json::json!({
        "status": report.status(), "counts": report.counts,
        "evidence": report.evidence, "hresult": report.items[0].hresult,
        "rootAndDescendantIdentities": true, "originalAbsent": !root.exists(),
        "emptyFolderPreserved": true, "sentinelPreserved": true,
        "sha256": format!("{:x}", Sha256::digest(&moved)), "bytes": moved.len(),
    });
    fs::write(
        fixture.join("proof.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
