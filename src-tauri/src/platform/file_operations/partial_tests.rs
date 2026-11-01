use super::{NativeFileOperationLane, capture_source, check_plan};
use crate::domain::{MAX_FRAME, operations::*};
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::Win32::UI::Shell::COPYENGINE_E_SHARING_VIOLATION_SRC;

fn source(path: &Path) -> OperationSource {
    capture_source(path.to_str().unwrap()).unwrap()
}

#[tokio::test]
#[cfg_attr(
    miri,
    ignore = "requires real Windows Shell COM and file sharing semantics"
)]
async fn actual_copy_is_partial_when_last_source_denies_content_sharing() {
    let report = copy_with_exclusive_source(2).await;
    assert_eq!(report.status(), OperationStatus::PartiallySucceeded);
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 2,
            skipped: 0,
            failed: 1,
            unknown: 0
        }
    );
    assert!(report.evidence.outcomes_complete);
}

#[tokio::test]
#[cfg_attr(
    miri,
    ignore = "requires real Windows Shell COM and file sharing semantics"
)]
async fn actual_copy_keeps_unobserved_suffix_unknown_when_middle_source_is_locked() {
    let report = copy_with_exclusive_source(1).await;
    assert_eq!(report.status(), OperationStatus::InterruptedUnknown);
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 1,
            skipped: 0,
            failed: 1,
            unknown: 1
        }
    );
    assert!(!report.evidence.outcomes_complete);
}

async fn copy_with_exclusive_source(locked_index: usize) -> OperationReport {
    // Given: distinct owned sources, with one source locked after capture.
    let fixture = tempfile::tempdir().unwrap();
    let destination = fixture.path().join("destination");
    fs::create_dir(&destination).unwrap();
    let names = ["01-first.bin", "02-second.bin", "03-third.bin"];
    let payloads = [vec![0x19; 4097], vec![0xa7; 8193], vec![0x53; 16385]];
    let sources: Vec<_> = names
        .iter()
        .zip(&payloads)
        .map(|(name, bytes)| {
            let path = fixture.path().join(name);
            fs::write(&path, bytes).unwrap();
            source(&path)
        })
        .collect();
    let destination_identity = source(&destination);
    let plan = OperationPlan {
        project_id: None,
        action: PreparedAction::Copy {
            sources: sources.clone(),
            destination: destination_identity.clone(),
        },
    };
    check_plan(&plan).unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&sources[locked_index].path)
        .unwrap();
    let denied = fs::read(&sources[locked_index].path)
        .unwrap_err()
        .raw_os_error();
    let preflight = check_plan(&plan);
    eprintln!(
        "P4_PARTIAL_PREFLIGHT {}",
        serde_json::json!({"contentReadError": denied, "globalCheckPassed": preflight.is_ok()})
    );
    assert_eq!(denied, Some(32));
    preflight.unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let lane = NativeFileOperationLane::start().unwrap();

    // When: the existing STA lane performs the real Shell operation with the lock held.
    let result = lane.execute(plan, 0, Arc::clone(&cancel)).await;
    drop(locked);
    lane.shutdown().await.unwrap();

    // Then: retain sanitized native outcomes before asserting the exact contract.
    let report = result.unwrap();
    let observations: Vec<_> = report
        .items
        .iter()
        .map(|item| {
            serde_json::json!({
                "sourceIndex": sources.iter().position(|source| source.path == item.source),
                "destinationName": item.destination.as_deref().and_then(|path| Path::new(path).file_name()).and_then(|name| name.to_str()),
                "status": item.status,
                "hresult": item.hresult,
            })
        })
        .collect();
    let completion = OperationCompletion {
        schema_version: 1,
        id: "native-partial-copy".into(),
        report: report.clone(),
    };
    let completion_bytes = serde_json::to_vec(&completion).unwrap().len();
    eprintln!(
        "P4_PARTIAL_NATIVE {}",
        serde_json::json!({
            "lockedSourceIndex": locked_index,
            "status": report.status(), "counts": report.counts, "evidence": report.evidence,
            "postCopyItemOrder": observations, "completionBytes": completion_bytes,
        })
    );
    assert!(report.failure.is_none());
    assert!(!report.evidence.perform_succeeded);
    assert!(!report.evidence.cancellation_confirmed);
    assert!(!cancel.load(Ordering::Acquire));
    assert_eq!(report.evidence.aborted, Some(false));
    assert_eq!(report.items.len(), locked_index + 1);
    for (index, item) in report.items.iter().enumerate() {
        assert_eq!(item.source, sources[index].path);
        if index < locked_index {
            assert_eq!(item.status, ItemStatus::Changed);
            assert_eq!(item.hresult, 0);
            assert_eq!(
                item.destination.as_deref(),
                destination.join(names[index]).to_str()
            );
            assert_eq!(
                fs::read(destination.join(names[index])).unwrap(),
                payloads[index]
            );
        } else {
            assert_eq!(item.status, ItemStatus::Failed);
            assert_eq!(item.hresult, COPYENGINE_E_SHARING_VIOLATION_SRC.0);
            assert!(item.destination.is_none());
            assert!(!destination.join(names[index]).exists());
        }
    }
    for (index, original) in sources.iter().enumerate() {
        assert_eq!(fs::read(&original.path).unwrap(), payloads[index]);
        assert_eq!(source(Path::new(&original.path)), *original);
        if index >= locked_index {
            assert!(!destination.join(names[index]).exists());
        }
    }
    assert_eq!(source(&destination), destination_identity);
    assert_eq!(fs::read_dir(&destination).unwrap().count(), locked_index);
    assert!(report.items.len() <= MAX_OPERATION_RESULTS);
    assert!(completion_bytes < MAX_FRAME);
    completion.validate().unwrap();
    fixture.close().unwrap();
    eprintln!(
        "P4_PARTIAL_VERIFIED bytes=exact identities=preserved lock=released lane=joined fixture=removed"
    );
    report
}
