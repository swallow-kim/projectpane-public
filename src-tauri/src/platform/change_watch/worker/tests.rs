use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

fn path(path: &std::path::Path) -> String {
    path.to_str().unwrap().into()
}
fn configured(watches: &mut Watches, generation: u32, paths: Vec<String>) -> u8 {
    match watches
        .request(WatchRequest::Replace { generation, paths })
        .unwrap()
    {
        WatchReply::Configured { watched, .. } => watched,
        _ => panic!("wrong response"),
    }
}
fn poll(watches: &mut Watches, generation: u32) -> u8 {
    match watches.request(WatchRequest::Poll { generation }).unwrap() {
        WatchReply::Polled { changed, .. } => changed,
        _ => panic!("wrong response"),
    }
}
fn changed(watches: &mut Watches, generation: u32) -> u8 {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let mask = poll(watches, generation);
        if mask != 0 {
            return mask;
        }
        assert!(Instant::now() < until, "native change did not arrive");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn actual_changes_share_a_handle_and_report_each_subscriber_without_names() {
    let fixture = tempfile::tempdir().unwrap();
    let mut watches = Watches::default();
    assert_eq!(
        configured(&mut watches, 1, vec![path(fixture.path()); 2]),
        1
    );
    assert_eq!(poll(&mut watches, 1), 0);
    let source = fixture.path().join("한글.txt");
    fs::write(&source, b"original").unwrap();
    assert_eq!(changed(&mut watches, 1), 3);
    fs::write(&source, b"changed and flushed").unwrap();
    assert_eq!(changed(&mut watches, 1), 3);
    let target = fixture.path().join("renamed.txt");
    fs::rename(&source, &target).unwrap();
    assert_eq!(changed(&mut watches, 1), 3);
    assert_eq!(fs::read(&target).unwrap(), b"changed and flushed");
    fs::remove_file(&target).unwrap();
    assert_eq!(changed(&mut watches, 1), 3);
    drop(watches);
    fixture.close().unwrap();
}

#[test]
fn malformed_replace_preserves_old_generation_and_stale_polls_do_not_rearm_it() {
    let fixture = tempfile::tempdir().unwrap();
    let mut watches = Watches::default();
    configured(&mut watches, 4, vec![path(fixture.path())]);
    assert!(
        watches
            .request(WatchRequest::Replace {
                generation: 5,
                paths: vec![path(fixture.path()), "relative".into()]
            })
            .is_err()
    );
    assert_eq!(watches.generation, 4);
    assert_eq!(watches.handles.len(), 1);
    fs::write(fixture.path().join("new"), b"fixture").unwrap();
    assert_eq!(
        watches
            .request(WatchRequest::Poll { generation: 5 })
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
    assert_eq!(changed(&mut watches, 4), 1);
    assert_eq!(configured(&mut watches, 5, vec![]), 0);
    assert_eq!(poll(&mut watches, 5), 0);
}

fn handle_count() -> u32 {
    // SAFETY: FFI category8: GetCurrentProcess returns a non-owned pseudo handle.
    let process = unsafe { GetCurrentProcess() };
    let mut count = 0;
    // SAFETY: FFI category8: process is live and count is an initialized writable DWORD.
    assert_ne!(unsafe { GetProcessHandleCount(process, &mut count) }, 0);
    count
}

#[test]
fn repeated_reconfiguration_releases_native_handles_and_bounds_active_slots() {
    // GetProcessHandleCount includes every parallel test's threads/files/COM.
    // Keep the exact before/+4/after assertions in their own test process.
    const ISOLATED: &str = "PROJECTPANE_WATCH_HANDLE_TEST_CHILD";
    if std::env::var_os(ISOLATED).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::change_watch::worker::tests::repeated_reconfiguration_releases_native_handles_and_bounds_active_slots",
                "--nocapture",
            ])
            .env(ISOLATED, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("WATCH_HANDLES generations=100 active=4")
        );
        return;
    }
    let fixture = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..4)
        .map(|index| {
            let directory = fixture.path().join(format!("slot-{index}"));
            fs::create_dir(&directory).unwrap();
            path(&directory)
        })
        .collect();
    let before = handle_count();
    let mut watches = Watches::default();
    for generation in 1..=100 {
        assert_eq!(configured(&mut watches, generation, paths.clone()), 4);
        assert_eq!(handle_count(), before + 4);
    }
    drop(watches);
    assert_eq!(handle_count(), before);
    eprintln!(
        "WATCH_HANDLES generations=100 active=4 before={before} after={}",
        handle_count()
    );
}

#[test]
fn nested_activity_never_invalidates_an_unrelated_directory_slot() {
    let fixture = tempfile::tempdir().unwrap();
    let watched = fixture.path().join("watched");
    let unrelated = fixture.path().join("unrelated");
    fs::create_dir(&watched).unwrap();
    fs::create_dir(&unrelated).unwrap();
    let nested = watched.join("child");
    fs::create_dir(&nested).unwrap();
    let file = nested.join("existing");
    fs::write(&file, b"before").unwrap();
    let mut watches = Watches::default();
    configured(&mut watches, 1, vec![path(&watched), path(&unrelated)]);
    fs::write(&file, b"after").unwrap();
    let mut observed = Vec::new();
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(20));
        let mask = poll(&mut watches, 1);
        assert_eq!(mask & 2, 0);
        observed.push(mask);
    }
    assert_eq!(fs::read(file).unwrap(), b"after");
    eprintln!("WATCH_NESTED_HINT masks={observed:?}; no exact delta or no-false-positive promise");
}
