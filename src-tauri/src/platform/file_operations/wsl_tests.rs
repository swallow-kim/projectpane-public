//! Opt-in tests. Only fresh synthetic children of the explicitly supplied WSL parent.
use super::*;
use crate::domain::operations::*;
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn fixture() -> (std::path::PathBuf, std::path::PathBuf) {
    let parent =
        std::env::var("PROJECTPANE_WSL_FIXTURE_PARENT").expect("explicit WSL fixture parent");
    assert_eq!(parent, r"\\wsl.localhost\Ubuntu\home\example-user");
    let wsl = tempfile::Builder::new()
        .prefix("projectpane-issue1-synthetic-")
        .tempdir_in(parent)
        .unwrap()
        .keep();
    let local = tempfile::Builder::new()
        .prefix("projectpane-wsl-synthetic-")
        .tempdir()
        .unwrap()
        .keep();
    eprintln!(
        "owned WSL fixture: {}\nowned Windows fixture: {}",
        wsl.display(),
        local.display()
    );
    (local, wsl)
}
async fn execute(plan: OperationPlan, cancel: bool) -> OperationReport {
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(plan, 0, Arc::new(AtomicBool::new(cancel)))
        .await;
    lane.shutdown().await.unwrap();
    result.unwrap()
}
fn verify(root: &Path, payload: &[u8]) {
    assert_eq!(fs::read(root.join("한글 space.bin")).unwrap(), payload);
    assert!(root.join("empty").is_dir());
}
#[tokio::test]
#[ignore = "requires explicitly authorized WSL synthetic parent"]
async fn wsl_copy_round_trip_collision_and_precancel_preserve_originals() {
    let (local, wsl) = fixture();
    let source = local.join("source");
    fs::create_dir_all(source.join("empty")).unwrap();
    let payload: Vec<u8> = (0..65537).map(|i| (i % 251) as u8).collect();
    fs::write(source.join("한글 space.bin"), &payload).unwrap();
    for (from, to) in [(&source, &wsl), (&wsl.join("source"), &local.join("back"))] {
        fs::create_dir_all(to).unwrap();
        let identity = capture_source(from.to_str().unwrap()).unwrap();
        let plan = directory_copy_tests::copy_plan(&[from], to).expect("WSL copy prepare");
        let report = execute(plan, false).await;
        eprintln!("WSL copy report {report:?}");
        assert_eq!(report.status(), OperationStatus::Succeeded);
        assert_eq!(report.counts.changed, 3);
        assert!(report.evidence.outcomes_complete);
        verify(from, &payload);
        verify(&to.join("source"), &payload);
        assert_eq!(capture_source(from.to_str().unwrap()).unwrap(), identity);
        assert!(directory_copy_tests::copy_plan(&[from], to).is_err());
    }
    let canceled = wsl.join("cancel-target");
    fs::create_dir(&canceled).unwrap();
    let plan = directory_copy_tests::copy_plan(&[&source], &canceled).unwrap();
    let report = execute(plan, true).await;
    assert_ne!(report.status(), OperationStatus::Succeeded);
    assert_eq!(fs::read_dir(canceled).unwrap().count(), 0);
    verify(&source, &payload);
}

#[tokio::test]
#[ignore = "requires explicitly authorized WSL synthetic parent"]
async fn wsl_copy_partial_failure_and_case_collision_keep_sources() {
    use std::os::windows::fs::OpenOptionsExt;
    let (local, wsl) = fixture();
    let paths: Vec<_> = ["01-first", "02-locked", "03-last"]
        .iter()
        .map(|n| {
            let path = local.join(n);
            fs::write(&path, n.as_bytes()).unwrap();
            path
        })
        .collect();
    let refs: Vec<_> = paths.iter().map(|p| p.as_path()).collect();
    let plan = directory_copy_tests::copy_plan(&refs, &wsl).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&paths[1])
        .unwrap();
    let report = execute(plan, false).await;
    drop(lock);
    eprintln!("WSL partial report {report:?}");
    assert_eq!(report.status(), OperationStatus::InterruptedUnknown);
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 1,
            failed: 1,
            unknown: 1,
            skipped: 0
        }
    );
    for path in &paths {
        assert_eq!(
            fs::read(path).unwrap(),
            path.file_name().unwrap().to_str().unwrap().as_bytes()
        );
    }
    assert_eq!(fs::read(wsl.join("01-first")).unwrap(), b"01-first");
    let names = wsl.join("case-sensitive");
    fs::create_dir(&names).unwrap();
    fs::write(names.join("A.txt"), b"upper").unwrap();
    fs::write(names.join("a.txt"), b"lower").unwrap();
    assert_eq!(
        fs::read_dir(&names).unwrap().count(),
        2,
        "fixture requires a case-sensitive Linux folder"
    );
    let target = local.join("case-target");
    fs::create_dir(&target).unwrap();
    assert!(directory_copy_tests::copy_plan(&[&names], &target).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert_eq!(fs::read(names.join("A.txt")).unwrap(), b"upper");
    assert_eq!(fs::read(names.join("a.txt")).unwrap(), b"lower");
}

fn move_plan(paths: &[&Path], destination: &Path) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action: PreparedAction::Move {
            sources: paths
                .iter()
                .map(|p| capture_source(p.to_str().unwrap()).unwrap())
                .collect(),
            destination: capture_source(destination.to_str().unwrap()).unwrap(),
        },
    }
}

#[tokio::test]
#[ignore = "requires explicitly authorized WSL synthetic parent"]
async fn wsl_same_volume_move_keeps_file_and_descendant_identity() {
    let (local, wsl) = fixture();
    let folder = wsl.join("한글 folder");
    let destination = wsl.join("destination");
    fs::create_dir_all(folder.join("empty")).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(folder.join("body.txt"), b"same Linux object").unwrap();
    let file = wsl.join("ordinary.txt");
    fs::write(&file, b"ordinary").unwrap();
    let before: Vec<_> = [&folder, &folder.join("body.txt"), &file]
        .into_iter()
        .map(|p| capture_source(p.to_str().unwrap()).unwrap())
        .collect();
    let plan = move_plan(&[&folder, &file], &destination);
    check_plan(&plan).unwrap();
    let report = execute(plan, false).await;
    eprintln!("same WSL move: {report:?}");
    assert_eq!(report.status(), OperationStatus::Succeeded);
    assert_eq!(report.counts.changed, 2);
    for (old, path) in before.iter().zip([
        destination.join("한글 folder"),
        destination.join("한글 folder/body.txt"),
        destination.join("ordinary.txt"),
    ]) {
        let arrived = capture_source(path.to_str().unwrap()).unwrap();
        assert_eq!(
            (old.volume, old.file_index),
            (arrived.volume, arrived.file_index)
        );
        assert!(!Path::new(&old.path).exists());
    }
    assert_eq!(
        fs::read(destination.join("한글 folder/body.txt")).unwrap(),
        b"same Linux object"
    );
    assert!(destination.join("한글 folder/empty").is_dir());
    assert_eq!(
        fs::read(destination.join("ordinary.txt")).unwrap(),
        b"ordinary"
    );
    // Cross-provider moves remain rejected, even when IDs happen to collide.
    let cross = move_plan(&[&destination.join("ordinary.txt")], &local);
    assert!(check_plan(&cross).is_err());
    let child = move_plan(&[&destination], &destination.join("한글 folder/empty"));
    assert!(check_plan(&child).is_err());
}

#[test]
#[ignore = "requires explicitly authorized WSL synthetic parent"]
fn wsl_move_collision_race_cancel_partial_and_uncertainty_do_not_replay() {
    use std::sync::atomic::Ordering;
    let (_, wsl) = fixture();
    let destination = wsl.join("destination");
    fs::create_dir(&destination).unwrap();
    let files: Vec<_> = ["first.txt", "second.txt", "third.txt"]
        .into_iter()
        .map(|n| {
            let p = wsl.join(n);
            fs::write(&p, n.as_bytes()).unwrap();
            p
        })
        .collect();
    let OperationPlan {
        action: PreparedAction::Move {
            sources,
            destination: dest,
        },
        ..
    } = move_plan(
        &files.iter().map(|p| p.as_path()).collect::<Vec<_>>(),
        &destination,
    )
    else {
        unreachable!()
    };
    let cancel = AtomicBool::new(true);
    let progress = NativeProgress::default();
    let report = wsl_move::execute(&sources, &dest, &cancel, &progress).unwrap();
    assert_eq!(report.status(), OperationStatus::Cancelled);
    assert!(files.iter().all(|p| p.exists()));
    cancel.store(false, Ordering::Release);
    let report =
        wsl_move::execute_with_probe(&sources, &dest, &cancel, &progress, &|index, after| {
            if index == 1 && !after {
                fs::write(destination.join("second.txt"), b"collision owner").unwrap();
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 1,
            failed: 1,
            skipped: 1,
            unknown: 0
        }
    );
    assert_eq!(report.status(), OperationStatus::PartiallySucceeded);
    assert_eq!(
        fs::read(destination.join("second.txt")).unwrap(),
        b"collision owner"
    );
    assert_eq!(fs::read(&files[1]).unwrap(), b"second.txt");
    assert_eq!(fs::read(&files[2]).unwrap(), b"third.txt");
    let report =
        wsl_move::execute_with_probe(&sources[2..], &dest, &cancel, &progress, &|_, after| {
            if after {
                cancel.store(true, Ordering::Release);
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(report.counts.changed, 1); // cancel is not undo
    assert_eq!(
        fs::read(destination.join("third.txt")).unwrap(),
        b"third.txt"
    );
    assert!(wsl_move::execute(&sources, &dest, &AtomicBool::new(false), &progress).is_err());
    // No replay after restart: existing journal policy maps pending to unknown.
    assert_eq!(
        OperationStatus::Running.after_restart(),
        OperationStatus::InterruptedUnknown
    );
}

#[test]
#[ignore = "requires explicitly authorized WSL synthetic parent"]
fn wsl_move_cancel_between_items_and_lost_observation_preserve_results() {
    use std::sync::atomic::Ordering;
    let (_, wsl) = fixture();
    let destination = wsl.join("destination");
    fs::create_dir(&destination).unwrap();
    let files: Vec<_> = ["first", "second"]
        .iter()
        .map(|name| {
            let p = wsl.join(name);
            fs::write(&p, name).unwrap();
            p
        })
        .collect();
    let OperationPlan {
        action: PreparedAction::Move {
            sources,
            destination: dest,
        },
        ..
    } = move_plan(&[&files[0], &files[1]], &destination)
    else {
        unreachable!()
    };
    let cancel = AtomicBool::new(false);
    let report = wsl_move::execute_with_probe(
        &sources,
        &dest,
        &cancel,
        &NativeProgress::default(),
        &|_, after| {
            if after {
                cancel.store(true, Ordering::Release);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(report.status(), OperationStatus::PartiallySucceeded);
    assert!(report.evidence.cancellation_confirmed);
    assert_eq!(
        report.counts,
        OutcomeCounts {
            changed: 1,
            skipped: 1,
            failed: 0,
            unknown: 0
        }
    );
    assert_eq!(fs::read(destination.join("first")).unwrap(), b"first");
    assert_eq!(fs::read(&files[1]).unwrap(), b"second");
    cancel.store(false, Ordering::Release);
    // Inject an observation failure after success. Keep the protection handle:
    // an actual concurrent rename was refused with sharing violation (32).
    let survivor = destination.join("second");
    let report = wsl_move::execute_with_probe(
        &sources[1..],
        &dest,
        &cancel,
        &NativeProgress::default(),
        &|_, after| {
            if after {
                return Err(crate::domain::Fault::new(crate::domain::FaultCode::IoError));
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(report.status(), OperationStatus::InterruptedUnknown);
    assert_eq!(report.counts.unknown, 1);
    assert_eq!(fs::read(&survivor).unwrap(), b"second");
    assert!(wsl_move::execute(&sources[1..], &dest, &cancel, &NativeProgress::default()).is_err());
    assert_eq!(fs::read(&survivor).unwrap(), b"second");
}
