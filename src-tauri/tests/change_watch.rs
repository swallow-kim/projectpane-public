#![cfg(all(windows, not(miri)))]

use projectpane_core::{domain::FaultCode, platform::change_watch::WatchSession};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    },
};

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_projectpane-read-worker"))
}
fn path(path: &Path) -> String {
    path.to_str().unwrap().into()
}

fn process_handle(pid: u32) -> OwnedHandle {
    // SAFETY: FFI category8: pid was obtained from our live spawned worker;
    // request only synchronization rights and no inheritance.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!raw.is_null());
    // SAFETY: handle category7: OpenProcess returned a new valid owned handle.
    unsafe { OwnedHandle::from_raw_handle(raw) }
}
fn verify_exit(handle: &OwnedHandle) {
    // SAFETY: handle category7: handle stays owned for the bounded process wait.
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 5_000) },
        WAIT_OBJECT_0
    );
}

#[tokio::test]
async fn crashed_live_worker_is_discarded_and_an_independent_session_still_watches() {
    let fixture = tempfile::tempdir().unwrap();
    let watched = path(fixture.path());
    let mut session = WatchSession::spawn(executable()).await.unwrap();
    assert_eq!(session.replace(1, vec![watched.clone()]).await.unwrap(), 1);
    assert_eq!(session.poll().await.unwrap(), 0);
    let pid = session.process_id().unwrap();
    // SAFETY: FFI category8: pid is our live, configured notification worker.
    // Retain its owned handle so termination cannot target a subsequently reused PID.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
    assert!(!raw.is_null());
    // SAFETY: handle category7: OpenProcess returned a newly owned valid handle.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: FFI category8: only our retained test worker handle is terminated.
    assert_ne!(unsafe { TerminateProcess(process.as_raw_handle(), 17) }, 0);
    verify_exit(&process);
    let failure = session.poll().await.unwrap_err();
    assert!(matches!(
        failure.code,
        FaultCode::IoError | FaultCode::ProtocolError
    ));
    assert_eq!(session.process_id(), None);
    assert_eq!(
        session.poll().await.unwrap_err().code,
        FaultCode::InvalidState
    );
    assert_eq!(
        session
            .replace(2, vec![watched.clone()])
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidState
    );
    session.stop().await.unwrap();

    let mut independent = WatchSession::spawn(executable()).await.unwrap();
    let independent_process = process_handle(independent.process_id().unwrap());
    assert_eq!(independent.replace(1, vec![watched]).await.unwrap(), 1);
    fs::write(fixture.path().join("after-crash.txt"), b"preserved").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while independent.poll().await.unwrap() == 0 {
        assert!(
            Instant::now() < deadline,
            "independent notification timed out"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    independent.stop().await.unwrap();
    verify_exit(&independent_process);
    assert_eq!(
        fs::read(fixture.path().join("after-crash.txt")).unwrap(),
        b"preserved"
    );
    eprintln!(
        "WATCH_CHILD_CRASH fault={:?} terminal=true independent_notification=true both_exited=true",
        failure.code
    );
}

#[tokio::test]
async fn actual_worker_shares_replaces_rejects_stale_and_exits_on_eof() {
    let fixture = tempfile::tempdir().unwrap();
    let first = fixture.path().join("first");
    let second = fixture.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let mut session = WatchSession::spawn(executable()).await.unwrap();
    assert_eq!(
        session
            .replace(1, vec![path(&first), path(&first), path(&second)])
            .await
            .unwrap(),
        2
    );
    assert_eq!(session.poll().await.unwrap(), 0);
    fs::write(second.join("한글.txt"), b"new").unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let mask = session.poll().await.unwrap();
        if mask != 0 {
            assert_eq!(mask, 4);
            break;
        }
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        session.replace(1, vec![]).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    assert!(session.process_id().is_some());
    assert_eq!(session.replace(2, vec![]).await.unwrap(), 0);
    fs::write(first.join("after-stop"), b"unchanged").unwrap();
    assert_eq!(session.poll().await.unwrap(), 0);
    session.stop().await.unwrap();
    assert_eq!(fs::read(second.join("한글.txt")).unwrap(), b"new");
    eprintln!("WATCH_WORKER shared=2 slots=3 changed=4 stale=refused clear=0 eof=joined");
}

#[tokio::test]
async fn invalid_configuration_is_rejected_and_missing_paths_do_not_kill_the_worker() {
    let fixture = tempfile::tempdir().unwrap();
    let mut session = WatchSession::spawn(executable()).await.unwrap();
    let pid = session.process_id();
    let process = process_handle(pid.unwrap());
    assert_eq!(
        session
            .replace(1, vec![path(fixture.path()); 5])
            .await
            .unwrap_err()
            .code,
        FaultCode::ResourceLimit
    );
    assert_eq!(session.process_id(), pid);
    assert_eq!(
        session
            .replace(1, vec![path(fixture.path())])
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        session
            .replace(2, vec![path(&fixture.path().join("missing"))])
            .await
            .unwrap(),
        0
    );
    assert_eq!(session.unavailable(), 1);
    assert_eq!(session.process_id(), pid);
    assert_eq!(session.replace(3, vec![]).await.unwrap(), 0);
    session.stop().await.unwrap();
    verify_exit(&process);
}

#[tokio::test]
#[ignore = "owned helper subprocess used only by forced_parent_exit_reclaims_watch_worker"]
async fn watch_parent_fixture() {
    use std::io::Write;
    let root = std::env::var("PROJECTPANE_OWNED_WATCH_FIXTURE").unwrap();
    let mut watch = WatchSession::spawn(executable()).await.unwrap();
    watch.replace(1, vec![root]).await.unwrap();
    println!("OWNED_WATCH_READY {}", watch.process_id().unwrap());
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    watch.stop().await.unwrap();
}

#[tokio::test]
async fn forced_parent_exit_reclaims_watch_worker() {
    use std::process::Stdio;
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, BufReader},
        process::Command,
        time::timeout,
    };
    let fixture = tempfile::tempdir().unwrap();
    let mut host = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "watch_parent_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("PROJECTPANE_OWNED_WATCH_FIXTURE", fixture.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x00000008)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = BufReader::new(host.stdout.take().unwrap());
    let ready = async {
        for _ in 0..8 {
            let mut line = String::new();
            (&mut output).take(1024).read_line(&mut line).await.unwrap();
            if let Some(pid) = line.trim().strip_prefix("OWNED_WATCH_READY ") {
                return pid.parse::<u32>().unwrap();
            }
        }
        panic!("watch child did not become ready");
    };
    let pid = timeout(Duration::from_secs(10), ready).await.unwrap();
    let worker = process_handle(pid);
    host.kill().await.unwrap();
    host.wait().await.unwrap();
    verify_exit(&worker);
    fixture.close().unwrap();
    eprintln!("WATCH_FORCED_PARENT native notification worker exited after owner kill");
}
