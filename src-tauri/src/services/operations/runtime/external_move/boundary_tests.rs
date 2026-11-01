use super::*;
use crate::platform::explorer::external_move::tests::Fixture;
use std::{
    fs::{self, File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    time::Duration,
};
use windows_sys::Win32::{
    Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT},
    System::{
        IO::DeviceIoControl,
        Ioctl::{FSCTL_DELETE_REPARSE_POINT, FSCTL_SET_REPARSE_POINT},
    },
};

// Real junction mutation, confined to two directories under this fixture root.
// No privilege adjustment, registry changes, symbolic-link policy or OS setup.
struct Junction(File);
impl Junction {
    fn set(root: &Path, path: &Path, target: &Path) -> Self {
        assert!(path.starts_with(root) && target.starts_with(root) && path != target);
        let substitute: Vec<_> = format!(r"\??\{}", target.display())
            .encode_utf16()
            .collect();
        let print: Vec<_> = target.to_str().unwrap().encode_utf16().collect();
        let data_len = 8 + (substitute.len() + print.len() + 2) * 2;
        let mut bytes = Vec::new();
        bytes.extend(0xa0000003u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
        bytes.extend((data_len as u16).to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        for value in [
            0,
            substitute.len() * 2,
            (substitute.len() + 1) * 2,
            print.len() * 2,
        ] {
            bytes.extend((value as u16).to_le_bytes());
        }
        for unit in substitute.into_iter().chain([0]).chain(print).chain([0]) {
            bytes.extend(unit.to_le_bytes());
        }
        let file = OpenOptions::new()
            .write(true)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
        let mut returned = 0;
        // SAFETY: private fixture handle and bounded initialized mount-point data;
        // synchronous call owns no buffers beyond its return.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            ok,
            0,
            "junction setup failed: {:?}",
            std::io::Error::last_os_error()
        );
        Self(file)
    }
}
impl Drop for Junction {
    fn drop(&mut self) {
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&0xa0000003u32.to_le_bytes());
        let mut returned = 0;
        // Remove only the reparse metadata created above, not any target contents.
        let ok = unsafe {
            DeviceIoControl(
                self.0.as_raw_handle(),
                FSCTL_DELETE_REPARSE_POINT,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "synthetic junction cleanup failed");
    }
}

#[tokio::test]
async fn destination_junction_race_cannot_redirect_native_handle_move() {
    for stage in [Stage::Armed, Stage::BeforeRename(0)] {
        let fixture = Fixture::new();
        let data = tempfile::tempdir().unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        let root = fixture.root.path().to_owned();
        let target = root.join("target");
        let redirect = root.join("redirect");
        fs::create_dir(&redirect).unwrap();
        fs::write(redirect.join("owner.txt"), b"unrelated synthetic owner").unwrap();
        let junction = Arc::new(Mutex::new(None));
        let held = junction.clone();
        let redirected = redirect.clone();
        let target_path = target.clone();
        let probe: Probe = Arc::new(move |current| {
            if current == stage {
                *held.lock().unwrap() = Some(Junction::set(&root, &target_path, &redirected));
            }
        });
        let id = service
            .submit_external(
                fixture.start(None, Duration::from_secs(30)).unwrap(),
                target.to_str().unwrap().into(),
                probe,
            )
            .await
            .unwrap()
            .id;
        service.request_close(false).await.unwrap();
        service.clone().run(0).await.unwrap();
        let status = service.report(id).await.unwrap().unwrap().status();
        let other_entries = fs::read_dir(&redirect).unwrap().count();
        drop(junction.lock().unwrap().take());
        assert_eq!(
            other_entries, 1,
            "native move must not follow a replaced destination junction"
        );
        assert_eq!(
            fs::read(redirect.join("owner.txt")).unwrap(),
            b"unrelated synthetic owner"
        );
        assert_eq!(status, OperationStatus::Failed);
        for i in 0..2 {
            let source = Path::new(&fixture.paths[i]);
            let target = Path::new(&fixture.targets[i]);
            assert!(
                source.exists() && !target.exists(),
                "reparse conflict must preserve the original path"
            );
            assert_eq!(
                fs::read(if source.exists() { source } else { target }).unwrap(),
                b"owned synthetic bytes"
            );
        }
        assert!(fixture.trace.lock().unwrap().calls.is_empty());
        fixture.released();
    }
}

#[tokio::test]
async fn late_com_response_after_timeout_never_dispatches_paste_success() {
    let fixture = Fixture::new();
    let session = fixture.start(None, Duration::from_secs(30)).unwrap();
    session
        .bind("late-provider".into(), fixture.targets.clone())
        .unwrap();
    for i in 0..2 {
        fs::rename(&fixture.paths[i], &fixture.targets[i]).unwrap();
    }
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    fixture.trace.lock().unwrap().notice_gate = Some(gate.clone());
    assert!(session.finish_with_timeout(Duration::from_secs(1)).is_err());
    assert!(session.finish_with_timeout(Duration::from_secs(1)).is_err());
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    session.wait_until_released().unwrap();
    assert_eq!(
        fixture
            .trace
            .lock()
            .unwrap()
            .calls
            .iter()
            .map(|v| v.2)
            .collect::<Vec<_>>(),
        [0]
    );
    let records: Vec<serde_json::Value> = fs::read_dir(fixture.root.path().join("journal"))
        .unwrap()
        .map(|entry| serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap())
        .collect();
    assert!(
        records
            .iter()
            .any(|r| r["phase"] == "notice_uncertain" && r["operation"] == "late-provider")
    );
    assert!(!records.iter().any(|r| r["phase"] == "notified"));
    fixture.released();
}

#[tokio::test]
async fn timeout_witness_write_failure_still_revokes_notice_without_overwrite() {
    let fixture = Fixture::new();
    let session = fixture.start(None, Duration::from_secs(30)).unwrap();
    session
        .bind("unwritable-witness".into(), fixture.targets.clone())
        .unwrap();
    for i in 0..2 {
        fs::rename(&fixture.paths[i], &fixture.targets[i]).unwrap();
    }
    let journal = fixture.root.path().join("journal");
    let record: serde_json::Value = serde_json::from_slice(
        &fs::read(
            fs::read_dir(&journal)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap(),
    )
    .unwrap();
    let witness = journal.join(format!(
        "{}-timeout.json",
        record["session"].as_str().unwrap()
    ));
    fs::write(&witness, b"owned occupied witness").unwrap();
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    fixture.trace.lock().unwrap().notice_gate = Some(gate.clone());
    let first = session.finish_with_timeout(Duration::from_secs(1));
    let duplicate = session.finish_with_timeout(Duration::from_secs(1));
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    session.wait_until_released().unwrap();
    assert!(first.is_err() && duplicate.is_err());
    assert_eq!(fs::read(witness).unwrap(), b"owned occupied witness");
    assert_eq!(
        fixture
            .trace
            .lock()
            .unwrap()
            .calls
            .iter()
            .map(|v| v.2)
            .collect::<Vec<_>>(),
        [0]
    );
    for path in &fixture.targets {
        assert_eq!(fs::read(path).unwrap(), b"owned synthetic bytes");
    }
    fixture.released();
}

#[tokio::test]
async fn cancel_and_close_revoke_a_blocked_notice_before_the_timeout() {
    for close in [false, true] {
        let fixture = Fixture::new();
        let data = tempfile::tempdir().unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        fixture.trace.lock().unwrap().notice_gate = Some(gate.clone());
        let id = service
            .submit_external(
                fixture.start(None, Duration::from_secs(30)).unwrap(),
                fixture.root.path().join("target").to_str().unwrap().into(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap()
            .id;
        service.request_close(false).await.unwrap();
        let task = tokio::spawn(service.clone().run(0));
        let started = std::time::Instant::now();
        while fixture.trace.lock().unwrap().calls.is_empty()
            && started.elapsed() < Duration::from_secs(3)
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let calls = fixture.trace.lock().unwrap().calls.len();
        if close {
            service.request_close(true).await.unwrap();
        } else {
            service.cancel(id.clone()).await.unwrap();
        }
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        task.await.unwrap().unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            fixture
                .trace
                .lock()
                .unwrap()
                .calls
                .iter()
                .map(|v| v.2)
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(
            service.state.lock().unwrap().external_notices[&id],
            Notice::Suppressed
        );
        for path in &fixture.targets {
            assert_eq!(fs::read(path).unwrap(), b"owned synthetic bytes");
        }
        fixture.released();
    }
}

#[tokio::test]
async fn permanently_blocked_provider_preserves_timeout_and_never_replays_after_process_exit() {
    const CHILD: &str = "PROJECTPANE_PRIVATE_COM_HANG";
    if let Some(control) = std::env::var_os(CHILD) {
        let control = std::path::PathBuf::from(control);
        let block_at: usize = std::env::var("PROJECTPANE_PRIVATE_COM_CALL")
            .unwrap()
            .parse()
            .unwrap();
        let fixture = Fixture::in_root(tempfile::tempdir_in(&control).unwrap());
        let data = tempfile::tempdir_in(&control).unwrap();
        let service = OperationService::open(data.path(), None).unwrap();
        {
            let mut trace = fixture.trace.lock().unwrap();
            trace.notice_gate = Some(Arc::new((Mutex::new(false), std::sync::Condvar::new())));
            trace.block_at = Some(block_at); // this fake never returns from that call
        }
        let id = service
            .submit_external(
                fixture.start(None, Duration::from_secs(30)).unwrap(),
                fixture.root.path().join("target").to_str().unwrap().into(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap()
            .id;
        let task = tokio::spawn(service.clone().run(0));
        let started = std::time::Instant::now();
        while fixture.trace.lock().unwrap().calls.len() < block_at
            && started.elapsed() < Duration::from_secs(3)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(fixture.trace.lock().unwrap().calls.len(), block_at);
        service.request_close(true).await.unwrap(); // responsive state API; cannot retract in-flight COM
        while service.state.lock().unwrap().external_notices.get(&id) != Some(&Notice::Uncertain)
            && started.elapsed() < Duration::from_secs(8)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            service.state.lock().unwrap().external_notices[&id],
            Notice::Uncertain
        );
        assert!(
            !task.is_finished(),
            "in-process COM cannot safely be forced to return"
        );
        assert_eq!(
            service
                .access(|state| state.queue.admission_ready())
                .await
                .unwrap_err()
                .code,
            FaultCode::Busy
        );
        for path in &fixture.targets {
            assert!(OpenOptions::new().write(true).open(path).is_err());
        }
        let journal = fixture.root.path().join("journal");
        let timeout = fs::read_dir(&journal)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with("-timeout.json")
            })
            .unwrap();
        let witness: serde_json::Value =
            serde_json::from_slice(&fs::read(&timeout).unwrap()).unwrap();
        assert_eq!(witness["phase"], "notice_uncertain");
        assert_eq!(witness["operation"], id);
        fs::write(
            control.join("owned-fixture.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": id, "data": data.path(), "paths": fixture.paths, "targets": fixture.targets,
                "timeout": timeout, "calls": fixture.trace.lock().unwrap().calls.len(),
            }))
            .unwrap(),
        )
        .unwrap();
        // Terminate only this deliberately isolated test process. This is evidence
        // for a future host boundary, not a product kill/hang recovery feature.
        std::process::exit(79);
    }
    for call in [1, 2] {
        let control = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "services::operations::runtime::external_move::boundary_tests::permanently_blocked_provider_preserves_timeout_and_never_replays_after_process_exit", "--nocapture"])
            .env(CHILD, control.path()).env("PROJECTPANE_PRIVATE_COM_CALL", call.to_string())
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        // The only force-termination target is the exact Child handle created here.
        let start = std::time::Instant::now();
        let exit = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(15) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("owned COM test child exceeded its watchdog");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(exit.code(), Some(79));
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(control.path().join("owned-fixture.json")).unwrap())
                .unwrap();
        let data = Path::new(record["data"].as_str().unwrap());
        let timeout = Path::new(record["timeout"].as_str().unwrap());
        assert!(data.starts_with(control.path()) && timeout.starts_with(control.path()));
        let before = fs::read(timeout).unwrap();
        let recovered = OperationService::open(data, None).unwrap();
        for _ in 0..2 {
            assert_eq!(
                recovered
                    .submit(record["id"].as_str().unwrap().into())
                    .await
                    .unwrap()
                    .status,
                OperationStatus::Succeeded
            );
        }
        assert!(recovered.state.lock().unwrap().external.is_none());
        recovered.request_close(false).await.unwrap();
        recovered.clone().run(0).await.unwrap();
        assert_eq!(fs::read(timeout).unwrap(), before);
        assert_eq!(record["calls"], call);
        for i in 0..2 {
            let source = Path::new(record["paths"][i].as_str().unwrap());
            let target = Path::new(record["targets"][i].as_str().unwrap());
            assert!(source.starts_with(control.path()) && target.starts_with(control.path()));
            assert!(!source.exists());
            assert_eq!(fs::read(target).unwrap(), b"owned synthetic bytes");
        }
    }
}
