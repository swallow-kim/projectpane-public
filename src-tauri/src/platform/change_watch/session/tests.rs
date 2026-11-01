use super::*;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

async fn stalled() -> (WatchSession, OwnedHandle) {
    let mut command = Command::new("cmd.exe");
    command.args(["/D", "/C", "set /P watch_first= & set /P watch_second="]);
    let session = WatchSession::spawn_command(command).await.unwrap();
    // SAFETY: FFI category8: this PID belongs to the live child just created above;
    // only wait permission is requested, without inheritance.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, session.process_id().unwrap()) };
    assert!(!handle.is_null());
    // SAFETY: handle category7: OpenProcess returned a new valid owned handle.
    (session, unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn exited(process: &OwnedHandle) {
    // SAFETY: handle category7: process remains owned throughout this bounded wait.
    assert_eq!(
        unsafe { WaitForSingleObject(process.as_raw_handle(), 5_000) },
        WAIT_OBJECT_0
    );
}

#[tokio::test]
async fn timeout_discards_the_pipe_and_reaps_the_stalled_owned_process() {
    let (mut session, process) = stalled().await;
    assert_eq!(
        session.replace(1, vec![]).await.unwrap_err().code,
        FaultCode::Timeout
    );
    assert_eq!(session.process_id(), None);
    exited(&process);
    assert_eq!(
        session.replace(2, vec![]).await.unwrap_err().code,
        FaultCode::InvalidState
    );
}

#[tokio::test]
async fn dropping_an_exchange_future_kills_the_worker_and_prevents_pipe_reuse() {
    let (mut session, process) = stalled().await;
    assert!(
        timeout(Duration::from_millis(50), session.replace(1, vec![]))
            .await
            .is_err()
    );
    assert_eq!(session.process_id(), None);
    exited(&process);
    assert_eq!(
        session.replace(2, vec![]).await.unwrap_err().code,
        FaultCode::InvalidState
    );
}
