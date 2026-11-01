use super::*;
use std::process::Stdio;
use tokio::process::Command;
use windows_sys::Win32::{
    Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT},
    System::JobObjects::JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
};

#[test]
fn job_handle_cannot_be_inherited() {
    // Given: the exact job constructor used before spawning a worker.
    let job = WorkerJob::new().unwrap();
    let mut flags = 0;
    // When: query the live handle's inheritance flags.
    // SAFETY: job owns the handle and flags is a valid writable DWORD.
    let result = unsafe { GetHandleInformation(job.0.as_raw_handle(), &mut flags) };
    // Then: spawning cannot pass this handle to the child.
    assert_ne!(result, 0);
    assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
}

#[tokio::test]
async fn rejected_assignment_terminates_and_reaps_child() {
    // Given: a job with zero process capacity, which Windows must reject.
    let job = WorkerJob::new().unwrap();
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    // SAFETY: live job; initialized SDK structure with matching class and size.
    let result = unsafe {
        SetInformationJobObject(
            job.0.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            ptr::from_ref(&limits).cast(),
            u32::try_from(size_of_val(&limits)).unwrap(),
        )
    };
    assert_ne!(result, 0);
    let mut child = Command::new("cmd.exe")
        .args(["/D", "/C", "set /P fixture="])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x00000008)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // When: the same assignment/cleanup operation used by Broker is rejected.
    let error = job.assign(&mut child).await.unwrap_err();
    // Then: preserve the OS error and finish reaping before returning.
    assert!(error.raw_os_error().is_some());
    assert_eq!(child.id(), None);
    assert!(child.try_wait().unwrap().is_some());
}
