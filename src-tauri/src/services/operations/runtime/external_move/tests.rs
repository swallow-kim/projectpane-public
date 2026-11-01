use super::*;
use crate::platform::explorer::external_move::tests::Fixture;
use std::{fs, time::Duration};

fn noop() -> Probe {
    Arc::new(|_| {})
}
async fn submit(service: &Arc<OperationService>, fixture: &Fixture, probe: Probe) -> String {
    service
        .submit_external(
            fixture.start(None, Duration::from_secs(30)).unwrap(),
            fixture.root.path().join("target").to_str().unwrap().into(),
            probe,
        )
        .await
        .unwrap()
        .id
}
async fn run(service: &Arc<OperationService>) -> Result<(), Fault> {
    service.request_close(false).await.unwrap();
    service.clone().run(0).await
}
fn assert_bytes(fixture: &Fixture, moved: usize) {
    for i in 0..fixture.paths.len() {
        let (present, absent) = if i < moved {
            (&fixture.targets[i], &fixture.paths[i])
        } else {
            (&fixture.paths[i], &fixture.targets[i])
        };
        assert_eq!(fs::read(present).unwrap(), b"owned synthetic bytes");
        assert!(!Path::new(absent).exists());
    }
}

#[tokio::test]
async fn integrated_move_keeps_handles_through_result_and_notice_and_binds_operation_id() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let paths = fixture.paths.clone();
    let targets = fixture.targets.clone();
    let source_dir = fixture.root.path().join("source");
    let target_dir = fixture.root.path().join("target");
    let journals = [
        service
            .state
            .lock()
            .unwrap()
            .queue
            .journal
            .directory()
            .to_owned(),
        fixture.root.path().join("journal"),
    ];
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::Armed {
            for path in &paths {
                assert!(fs::write(path, b"forbidden overwrite").is_err());
                assert!(fs::rename(path, format!("{path}.other")).is_err());
            }
        }
        if matches!(stage, Stage::AfterResult | Stage::AfterNotice) {
            for path in &targets {
                assert!(
                    Path::new(path).exists(),
                    "expected moved target before testing its guard"
                );
                assert!(fs::write(path, b"forbidden overwrite").is_err());
                assert!(fs::rename(path, format!("{path}.other")).is_err());
            }
        }
        if matches!(
            stage,
            Stage::Armed | Stage::AfterResult | Stage::AfterNotice
        ) {
            for path in [&source_dir, &target_dir]
                .into_iter()
                .chain(journals.iter())
            {
                assert!(
                    fs::rename(path, path.with_extension("replaced")).is_err(),
                    "directory must remain pinned: {} at {stage:?}",
                    path.display()
                );
            }
        }
    });
    let id = submit(&service, &fixture, probe).await;
    assert_eq!(
        service.submit(id.clone()).await.unwrap().status,
        OperationStatus::Queued
    );
    // New work cannot enter while the admitted external transaction owns guards.
    assert_eq!(
        service
            .access(|state| state.queue.admission_ready())
            .await
            .unwrap_err()
            .code,
        FaultCode::Busy
    );
    run(&service).await.unwrap();
    assert_eq!(
        service.report(id.clone()).await.unwrap().unwrap().status(),
        OperationStatus::Succeeded
    );
    assert_bytes(&fixture, 2);
    fixture.released();
    for path in &fixture.targets {
        fs::OpenOptions::new().write(true).open(path).unwrap();
    }
    assert_eq!(
        fixture
            .trace
            .lock()
            .unwrap()
            .calls
            .iter()
            .map(|v| v.2)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let mut armed = false;
    for entry in fs::read_dir(fixture.root.path().join("journal")).unwrap() {
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        if record["phase"] == "armed" {
            assert_eq!(record["operation"], id);
            armed = true;
        }
    }
    assert!(armed);
    assert_eq!(
        service.state.lock().unwrap().external_notices[&id],
        Notice::Confirmed
    );
    drop(service);
    let reopened = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        reopened.submit(id).await.unwrap().status,
        OperationStatus::Succeeded
    );
    run(&reopened).await.unwrap();
    assert_bytes(&fixture, 2);
    assert_eq!(fixture.trace.lock().unwrap().calls.len(), 2);
}

#[tokio::test]
async fn queued_cancellation_releases_without_execution_or_notice() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let id = submit(&service, &fixture, noop()).await;
    assert_eq!(
        service.cancel(id.clone()).await.unwrap().status,
        OperationStatus::Cancelled
    );
    run(&service).await.unwrap();
    for _ in 0..100 {
        if fixture.trace.lock().unwrap().dropped != 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_bytes(&fixture, 0);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
    assert_eq!(
        service.submit(id).await.unwrap().status,
        OperationStatus::Cancelled
    );
}

#[tokio::test]
async fn partial_cancel_keeps_completed_destination_and_remaining_original_no_ack() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let weak = Arc::downgrade(&service);
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::AfterItem(0) {
            let service = weak.upgrade().unwrap();
            let mut state = service.state.lock().unwrap();
            let id = state.queue.running.clone().unwrap();
            state.queue.request_cancel(&id).unwrap();
        }
    });
    let id = submit(&service, &fixture, probe).await;
    run(&service).await.unwrap();
    let report = service.report(id.clone()).await.unwrap().unwrap();
    assert_eq!(report.status(), OperationStatus::PartiallySucceeded);
    assert_eq!((report.counts.changed, report.counts.skipped), (1, 1));
    assert_bytes(&fixture, 1);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
    drop(service);
    let recovered = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        recovered.submit(id).await.unwrap().status,
        OperationStatus::PartiallySucceeded
    );
    run(&recovered).await.unwrap();
    assert_bytes(&fixture, 1);
}

#[tokio::test]
async fn collision_created_after_arm_is_never_replaced_and_batch_stops() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let collision = fixture.targets[1].clone();
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::AfterItem(0) {
            fs::write(&collision, b"other synthetic owner").unwrap();
        }
    });
    let id = submit(&service, &fixture, probe).await;
    run(&service).await.unwrap();
    assert_eq!(
        service.report(id).await.unwrap().unwrap().status(),
        OperationStatus::InterruptedUnknown
    );
    assert_eq!(
        fs::read(&fixture.targets[0]).unwrap(),
        b"owned synthetic bytes"
    );
    assert_eq!(
        fs::read(&fixture.paths[1]).unwrap(),
        b"owned synthetic bytes"
    );
    assert_eq!(
        fs::read(&fixture.targets[1]).unwrap(),
        b"other synthetic owner"
    );
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
}

#[tokio::test]
async fn result_journal_failure_preserves_moved_files_and_suppresses_notice() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let weak = Arc::downgrade(&service);
    let journal = service
        .state
        .lock()
        .unwrap()
        .queue
        .journal
        .directory()
        .to_owned();
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::BeforeResult {
            let service = weak.upgrade().unwrap();
            let id = service.state.lock().unwrap().queue.running.clone().unwrap();
            // Force create_new failure without removing or rewriting any journal.
            fs::write(
                journal.join(format!("{id}.result.json")),
                b"synthetic failed writer",
            )
            .unwrap();
        }
    });
    let id = submit(&service, &fixture, probe).await;
    assert_eq!(run(&service).await.unwrap_err().code, FaultCode::IoError);
    assert!(
        !service
            .state()
            .await
            .unwrap()
            .operations
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .journal_saved
    );
    assert_bytes(&fixture, 2);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
    drop(service);
    let recovered = OperationService::open(data.path(), None).unwrap();
    assert_eq!(
        recovered.submit(id).await.unwrap().status,
        OperationStatus::InterruptedUnknown
    );
    run(&recovered).await.unwrap();
    assert_bytes(&fixture, 2);
}

#[tokio::test]
async fn protocol_record_and_notice_failures_do_not_repeat_moves_or_notice() {
    for (record_failure, notice_failure, moved, calls) in [
        (Some(2), None, 0, 0),
        (Some(3), None, 2, 0),
        (Some(4), None, 2, 1),
        (Some(5), None, 2, 2),
        (None, Some(1), 2, 1),
        (None, Some(2), 2, 2),
    ] {
        let fixture = Fixture::new();
        fixture.trace.lock().unwrap().fail_at = notice_failure;
        let data = tempfile::tempdir().unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        let session = fixture
            .start(record_failure, Duration::from_secs(30))
            .unwrap();
        let id = service
            .submit_external(
                session,
                fixture.root.path().join("target").to_str().unwrap().into(),
                noop(),
            )
            .await
            .unwrap()
            .id;
        run(&service).await.unwrap();
        assert_bytes(&fixture, moved);
        assert_eq!(
            service.state.lock().unwrap().external_notices[&id],
            if moved == 0 {
                Notice::Suppressed
            } else {
                Notice::Uncertain
            }
        );
        assert_eq!(fixture.trace.lock().unwrap().calls.len(), calls);
        fixture.released();
        drop(service);
        let recovered = OperationService::open(data.path(), None).unwrap();
        recovered.submit(id).await.unwrap();
        run(&recovered).await.unwrap();
        assert_bytes(&fixture, moved);
        assert_eq!(fixture.trace.lock().unwrap().calls.len(), calls);
    }
}

#[tokio::test]
async fn intent_write_failure_cannot_arm_or_move() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    {
        let state = service.state.lock().unwrap();
        let next = format!(
            "{}-{}.intent.json",
            state.queue.run_id,
            state.queue.next_id + 1
        );
        fs::write(
            state.queue.journal.directory().join(next),
            b"synthetic occupied intent",
        )
        .unwrap();
    }
    assert_eq!(
        service
            .submit_external(
                fixture.start(None, Duration::from_secs(30)).unwrap(),
                fixture.root.path().join("target").to_str().unwrap().into(),
                noop()
            )
            .await
            .unwrap_err()
            .code,
        FaultCode::IoError
    );
    assert_eq!(run(&service).await.unwrap_err().code, FaultCode::IoError);
    assert_bytes(&fixture, 0);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    for entry in fs::read_dir(fixture.root.path().join("journal")).unwrap() {
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        assert_ne!(record["phase"], "armed");
    }
}

#[tokio::test]
async fn process_interruption_recovers_evidence_without_replay() {
    const CHILD: &str = "PROJECTPANE_PRIVATE_INTEGRATION_CRASH";
    if let Some(control) = std::env::var_os(CHILD) {
        let control = std::path::PathBuf::from(control);
        let stage: usize = std::env::var("PROJECTPANE_PRIVATE_CRASH_STAGE")
            .unwrap()
            .parse()
            .unwrap();
        let fixture = Fixture::in_root(tempfile::tempdir_in(&control).unwrap());
        let data = tempfile::tempdir_in(&control).unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        let trace = fixture.trace.clone();
        let receipt = control.join("crash-notices.json");
        let probe: Probe = Arc::new(move |current| {
            let stop = [
                Stage::AfterItem(0),
                Stage::BeforeResult,
                Stage::AfterResult,
                Stage::AfterNotice,
            ][stage];
            if current == stop {
                fs::write(
                    &receipt,
                    serde_json::to_vec(&trace.lock().unwrap().calls.len()).unwrap(),
                )
                .unwrap();
                std::process::exit(73); // only this deliberately spawned fixture process
            }
        });
        let id = submit(&service, &fixture, probe).await;
        fs::write(
            control.join("owned-fixture.json"),
            serde_json::to_vec(&serde_json::json!({
                "data": data.path(), "paths": fixture.paths, "targets": fixture.targets, "id": id,
            }))
            .unwrap(),
        )
        .unwrap();
        run(&service).await.unwrap();
        panic!("crash boundary was not reached");
    }
    for stage in 0..4 {
        let control = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "services::operations::runtime::external_move::tests::process_interruption_recovers_evidence_without_replay", "--nocapture"])
            .env(CHILD, control.path()).env("PROJECTPANE_PRIVATE_CRASH_STAGE", stage.to_string())
            .output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(control.path().join("owned-fixture.json")).unwrap())
                .unwrap();
        let data = Path::new(record["data"].as_str().unwrap());
        assert!(data.starts_with(control.path()));
        let check = || {
            for i in 0..2 {
                let source = Path::new(record["paths"][i].as_str().unwrap());
                let target = Path::new(record["targets"][i].as_str().unwrap());
                assert!(source.starts_with(control.path()) && target.starts_with(control.path()));
                let moved = stage != 0 || i == 0;
                assert_eq!(
                    fs::read(if moved { target } else { source }).unwrap(),
                    b"owned synthetic bytes"
                );
                assert!(!(if moved { source } else { target }).exists());
            }
        };
        check();
        let recovered = OperationService::open(data, None).unwrap();
        let id = record["id"].as_str().unwrap().to_owned();
        for _ in 0..2 {
            assert_eq!(
                recovered.submit(id.clone()).await.unwrap().status,
                if stage < 2 {
                    OperationStatus::InterruptedUnknown
                } else {
                    OperationStatus::Succeeded
                }
            );
        }
        assert!(recovered.state.lock().unwrap().external.is_none());
        assert!(recovered.state.lock().unwrap().external_notices.is_empty());
        run(&recovered).await.unwrap();
        check();
        let notices: usize =
            serde_json::from_slice(&fs::read(control.path().join("crash-notices.json")).unwrap())
                .unwrap();
        assert_eq!(notices, if stage == 3 { 2 } else { 0 });
    }
}

#[tokio::test]
async fn expired_session_before_first_item_cannot_move_or_notify() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let session = fixture.start(None, Duration::from_secs(1)).unwrap();
    let expired = session.cancellation();
    let armed = Arc::new(AtomicBool::new(false));
    let observed = armed.clone();
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::Armed {
            observed.store(true, Ordering::Release);
            let start = std::time::Instant::now();
            while !expired.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(expired.load(Ordering::Acquire));
        }
    });
    service
        .submit_external(
            session,
            fixture.root.path().join("target").to_str().unwrap().into(),
            probe,
        )
        .await
        .unwrap();
    run(&service).await.unwrap();
    assert!(armed.load(Ordering::Acquire));
    assert_bytes(&fixture, 0);
    fixture.released();
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
}

#[tokio::test]
async fn close_cancel_after_last_move_suppresses_notice_without_rolling_back() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let weak = Arc::downgrade(&service);
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::AfterItem(1) {
            let service = weak.upgrade().unwrap();
            // Exercise the actual asynchronous close API from this blocking worker.
            tokio::runtime::Handle::current()
                .block_on(service.request_close(true))
                .unwrap();
        }
    });
    let id = submit(&service, &fixture, probe).await;
    service.clone().run(0).await.unwrap();
    assert_eq!(
        service.report(id.clone()).await.unwrap().unwrap().status(),
        OperationStatus::Succeeded
    );
    assert_eq!(
        service.state.lock().unwrap().external_notices[&id],
        Notice::Suppressed
    );
    assert_bytes(&fixture, 2);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
}

#[tokio::test]
async fn session_expiry_during_batch_stops_remaining_item() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let session = fixture.start(None, Duration::from_secs(1)).unwrap();
    let expired = session.cancellation();
    let probe: Probe = Arc::new(move |stage| {
        if stage == Stage::AfterItem(0) {
            let start = std::time::Instant::now();
            while !expired.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(expired.load(Ordering::Acquire));
        }
    });
    let id = service
        .submit_external(
            session,
            fixture.root.path().join("target").to_str().unwrap().into(),
            probe,
        )
        .await
        .unwrap()
        .id;
    run(&service).await.unwrap();
    assert_eq!(
        service.report(id).await.unwrap().unwrap().status(),
        OperationStatus::PartiallySucceeded
    );
    assert_bytes(&fixture, 1);
    assert!(fixture.trace.lock().unwrap().calls.is_empty());
    fixture.released();
}

#[tokio::test]
async fn delayed_com_notice_keeps_admission_and_handles_until_actual_release() {
    let fixture = Fixture::new();
    let data = tempfile::tempdir().unwrap();
    let service = OperationService::open(data.path(), None).unwrap();
    let gate = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    fixture.trace.lock().unwrap().notice_gate = Some(gate.clone());
    let id = submit(&service, &fixture, noop()).await;
    service.request_close(false).await.unwrap();
    let task = tokio::spawn(service.clone().run(0));
    let started = std::time::Instant::now();
    while fixture.trace.lock().unwrap().calls.is_empty()
        && started.elapsed() < Duration::from_secs(3)
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // Always release our private fake provider before assertions can unwind.
    let called = fixture.trace.lock().unwrap().calls.len();
    let admission = service.access(|state| state.queue.admission_ready()).await;
    let protected = fixture
        .targets
        .iter()
        .all(|path| fs::OpenOptions::new().write(true).open(path).is_err());
    let running = !task.is_finished();
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    task.await.unwrap().unwrap();
    assert_eq!(called, 1);
    assert_eq!(admission.unwrap_err().code, FaultCode::Busy);
    assert!(protected && running);
    assert_eq!(
        service.state.lock().unwrap().external_notices[&id],
        Notice::Confirmed
    );
    assert_bytes(&fixture, 2);
    fixture.released();
}

#[tokio::test]
async fn guarded_admission_rejects_locked_changed_and_aliased_sources() {
    use std::os::windows::fs::OpenOptionsExt;
    for mode in 0..3 {
        let fixture = Fixture::new();
        let data = tempfile::tempdir().unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        let session = fixture.start(None, Duration::from_secs(30)).unwrap();
        let original = fixture.paths[0].clone();
        let mut lock = None;
        if mode == 0 {
            lock = Some(
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(&original)
                    .unwrap(),
            );
        }
        if mode == 1 {
            fs::rename(&original, format!("{original}.saved")).unwrap();
            fs::write(&original, b"different identity").unwrap();
        }
        if mode == 2 {
            fs::hard_link(&original, format!("{original}.alias")).unwrap();
        }
        assert!(
            service
                .submit_external(
                    session,
                    fixture.root.path().join("target").to_str().unwrap().into(),
                    noop()
                )
                .await
                .is_err()
        );
        drop(lock);
        run(&service).await.unwrap();
        assert!(service.state().await.unwrap().operations.is_empty());
        assert!(fixture.trace.lock().unwrap().calls.is_empty());
        assert!(!Path::new(&fixture.targets[0]).exists());
        assert_eq!(
            fs::read(&fixture.paths[1]).unwrap(),
            b"owned synthetic bytes"
        );
    }
}
