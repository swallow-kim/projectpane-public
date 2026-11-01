use super::*;
use crate::domain::{EntryKind, FaultCode, operations::*};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
use windows::{
    Win32::UI::Shell::{
        IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
        TSF_DELETE_RECYCLE_IF_POSSIBLE,
    },
    core::{HRESULT, HSTRING},
};

fn source(path: &Path) -> OperationSource {
    capture_source(path.to_str().unwrap()).unwrap()
}

fn plan(sources: Vec<OperationSource>) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action: PreparedAction::Recycle { sources },
    }
}

fn shell(path: &Path) -> IShellItem {
    // SAFETY: COM category 8: tests hold an initialized STA and a live UTF-16
    // path for this synchronous call; the returned interface stays on this thread.
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }.unwrap()
}

#[test]
fn recycle_callbacks_refuse_permanent_deletion_unplanned_aliases_and_replacements() {
    let _apartment = execute::Apartment::initialize().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("source.txt");
    let alias = fixture.path().join("alias.txt");
    fs::write(&original, b"owned payload").unwrap();
    fs::hard_link(&original, &alias).unwrap();
    let captured = source(&original);
    let operation = plan(vec![captured.clone()]);
    assert!(check_plan(&operation).is_ok());
    let context = sink_state::SinkContext::new(1, Arc::new(AtomicBool::new(false)), Arc::default());
    let sink: IFileOperationProgressSink = sink::ProgressSink {
        context,
        plan: operation.clone(),
        tree: Default::default(),
        recycle: Default::default(),
        moving_tree: Default::default(),
    }
    .into();
    let flag = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
    // SAFETY: COM category 8: live sink/items belong to this STA. Direct callbacks
    // validate the contract only; they do not dispatch a Shell file operation.
    unsafe {
        assert!(sink.PreDeleteItem(0, &shell(&original)).is_err());
        assert!(sink.PreDeleteItem(flag, &shell(&alias)).is_err());
        assert!(sink.PreDeleteItem(flag, &shell(fixture.path())).is_err());
        assert!(sink.PreDeleteItem(flag, &shell(&original)).is_ok());
        assert!(sink.PreDeleteItem(flag, &shell(&original)).is_err());
    }
    let held = fixture.path().join("held.txt");
    fs::rename(&original, &held).unwrap();
    fs::write(&original, b"replacement").unwrap();
    assert_eq!(
        check_plan(&operation).unwrap_err().code,
        FaultCode::StaleRequest
    );
    // SAFETY: COM category 8: same STA and owned live interfaces as above.
    unsafe {
        assert!(sink.PreDeleteItem(flag, &shell(&original)).is_err());
    }
    assert_eq!(fs::read(&original).unwrap(), b"replacement");
    assert_eq!(fs::read(&held).unwrap(), b"owned payload");
    assert_eq!(fs::read(&alias).unwrap(), b"owned payload");
}

#[test]
fn recycle_success_without_returned_recycled_identity_is_unknown() {
    let _apartment = execute::Apartment::initialize().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("source.txt");
    fs::write(&original, b"keep").unwrap();
    for (flags, result) in [
        (0, HRESULT(0)),
        (TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, HRESULT(0)),
        (
            0,
            windows::Win32::UI::Shell::COPYENGINE_S_DONT_PROCESS_CHILDREN,
        ),
        (
            TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32,
            windows::Win32::UI::Shell::COPYENGINE_S_DONT_PROCESS_CHILDREN,
        ),
    ] {
        let context =
            sink_state::SinkContext::new(1, Arc::new(AtomicBool::new(false)), Arc::default());
        let state = std::rc::Rc::clone(&context.state);
        let sink: IFileOperationProgressSink = sink::ProgressSink {
            context,
            plan: plan(vec![source(&original)]),
            tree: Default::default(),
            recycle: Default::default(),
            moving_tree: Default::default(),
        }
        .into();
        // SAFETY: COM category 8: direct callbacks with owned STA interfaces and
        // an explicit null destination. No native deletion is executed by this test.
        unsafe {
            sink.PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, &shell(&original))
                .unwrap();
            assert!(
                sink.PostDeleteItem(flags, &shell(&original), result, None)
                    .is_err()
            );
        }
        let state = state.borrow();
        assert_eq!(state.report.counts.changed, 0);
        assert_eq!(state.report.counts.unknown, 1);
        assert_eq!(state.report.items[0].hresult, result.0);
        assert_eq!(state.report.items[0].destination, None);
        assert_eq!(fs::read(&original).unwrap(), b"keep");
    }
}

#[test]
fn recycle_rejects_folders_bin_paths_and_unbounded_result_reservations() {
    let fixture = tempfile::tempdir().unwrap();
    assert_eq!(
        check_plan(&plan(vec![source(fixture.path())]))
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    let bin = fixture.path().join("$Recycle.Bin");
    fs::create_dir(&bin).unwrap();
    let inside = bin.join("keep.txt");
    fs::write(&inside, b"keep").unwrap();
    let mut already_recycled = source(&inside);
    already_recycled.path = format!("{}$Recycle.Bin\\owned-fixture", &already_recycled.path[..3]);
    assert_eq!(
        check_plan(&plan(vec![already_recycled])).unwrap_err().code,
        FaultCode::InvalidRequest
    );
    assert!(check_plan(&plan(vec![source(&inside)])).is_ok());
    let sources = (0..64)
        .map(|i| {
            let file = fixture.path().join(format!("owned-{i}.txt"));
            fs::write(&file, b"keep").unwrap();
            source(&file)
        })
        .collect();
    assert_eq!(
        check_plan(&plan(sources)).unwrap_err().code,
        FaultCode::PayloadLimit
    );
    assert_eq!(fs::read(inside).unwrap(), b"keep");
}

#[tokio::test]
#[ignore = "requires a fresh PROJECTPANE_RECYCLE_FIXTURE; moves only its owned file to the Recycle Bin"]
async fn real_recycle_retains_exact_bytes_and_cancelled_or_stale_sources() {
    let root = std::path::PathBuf::from(
        std::env::var_os("PROJECTPANE_RECYCLE_FIXTURE").expect("explicit owned fixture"),
    );
    assert!(root.is_absolute() && !root.exists());
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("p4-recycle-")
    );
    assert!(
        !root
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    );
    assert_eq!(source(root.parent().unwrap()).kind, EntryKind::Folder);
    fs::create_dir(&root).unwrap();
    let original = root.join("휴지통 한글.txt");
    let payload: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
    fs::write(&original, &payload).unwrap();
    let captured = source(&original);
    let lane = NativeFileOperationLane::start().unwrap();
    let cancelled = lane
        .execute(
            plan(vec![captured.clone()]),
            0,
            Arc::new(AtomicBool::new(true)),
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status(), OperationStatus::Cancelled);
    assert_eq!(fs::read(&original).unwrap(), payload);
    let stale_path = root.join("stale.txt");
    fs::write(&stale_path, b"held original").unwrap();
    let stale = source(&stale_path);
    fs::rename(&stale_path, root.join("held.txt")).unwrap();
    fs::write(&stale_path, b"replacement").unwrap();
    assert_eq!(
        lane.execute(plan(vec![stale]), 0, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    let result = lane
        .execute(
            plan(vec![captured.clone()]),
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    fs::write(
        root.join("native-result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    let report = result.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.counts.changed, 1);
    assert!(report.evidence.outcomes_complete);
    assert!(!original.exists());
    let recycled_path = report.items[0].destination.as_ref().unwrap();
    let recycled = capture_source(recycled_path).unwrap();
    assert_eq!(
        (recycled.volume, recycled.file_index),
        (captured.volume, captured.file_index)
    );
    assert_eq!(fs::read(recycled_path).unwrap(), payload);
    assert_eq!(fs::read(&stale_path).unwrap(), b"replacement");
    assert_eq!(fs::read(root.join("held.txt")).unwrap(), b"held original");
    lane.shutdown().await.unwrap();
}
