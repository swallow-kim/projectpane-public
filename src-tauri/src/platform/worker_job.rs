use std::{
    io,
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use tokio::process::Child;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

pub(crate) struct WorkerJob(OwnedHandle);

impl WorkerJob {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: null attributes/name create an unnamed, non-inheritable job.
        // No other process receives a handle that could keep the job alive.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateJobObjectW returned a unique valid handle; ownership is
        // transferred exactly once, and OwnedHandle closes it on every exit path.
        let job = Self(unsafe { OwnedHandle::from_raw_handle(raw) });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(io::Error::other)?;
        // SAFETY: the live job has SET_ATTRIBUTES rights; the initialized SDK
        // structure, pointer, size and information class match for this call.
        let set = unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                ptr::from_ref(&limits).cast(),
                length,
            )
        };
        if set == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(crate) async fn assign(&self, child: &mut Child) -> io::Result<()> {
        let result = self.try_assign(child);
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    }

    pub(crate) fn assign_std(&self, child: &mut std::process::Child) -> io::Result<()> {
        // SAFETY: live owned process and job; child is waiting for stdin.
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) } == 0 {
            let error = io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(())
    }

    fn try_assign(&self, child: &Child) -> io::Result<()> {
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("worker already reaped"))?;
        // SAFETY: both handles remain owned and live across the call. Tokio's
        // spawned process handle has SET_QUOTA and TERMINATE rights. No request
        // has been sent: until assignment succeeds, the worker only awaits stdin.
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "worker_job_tests.rs"]
mod tests;
