#![cfg(windows)]

use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Command, Stdio},
};
use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};

#[test]
fn unassigned_worker_exits_on_eof_before_first_request() {
    // Given: the production worker, detached but not yet assigned to a job.
    let mut child = Command::new(env!("CARGO_BIN_EXE_projectpane-read-worker"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x00000008)
        .spawn()
        .unwrap();
    // When: the parent's only input pipe closes without sending a request.
    drop(child.stdin.take());
    // SAFETY: Child owns a valid process handle throughout this bounded wait.
    let waited = unsafe { WaitForSingleObject(child.as_raw_handle(), 5_000) };
    if waited != WAIT_OBJECT_0 {
        let _ = child.kill();
        let _ = child.wait();
    }
    // Then: EOF terminates the worker without job membership or a filesystem call.
    assert_eq!(waited, WAIT_OBJECT_0);
    assert!(child.wait().unwrap().success());
}
