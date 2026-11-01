use crate::{
    domain::{Fault, FaultCode},
    platform::worker_job::WorkerJob,
};
use std::{
    io::{self, Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

pub(super) struct ArchivePipe<'a> {
    child: Child,
    job: Option<WorkerJob>,
    receiver: Receiver<io::Result<Vec<u8>>>,
    buffered: Vec<u8>,
    offset: usize,
    cancel: &'a AtomicBool,
    started: Instant,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl<'a> ArchivePipe<'a> {
    pub(super) fn start(
        request: &impl serde::Serialize,
        cancel: &'a AtomicBool,
    ) -> Result<Self, Fault> {
        let powershell = std::path::PathBuf::from(
            std::env::var_os("SystemRoot")
                .ok_or_else(|| Fault::new(FaultCode::WorkerUnavailable))?,
        )
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let job = WorkerJob::new().map_err(Fault::from)?;
        let mut child = Command::new(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("archive.ps1"),
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if cfg!(test) {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .spawn()
            .map_err(Fault::from)?;
        if let Err(error) = job.assign_std(&mut child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Fault::new(FaultCode::IoError))?;
        let (send, receiver) = mpsc::sync_channel(2);
        let reader = std::thread::spawn(move || {
            loop {
                let mut buffer = vec![0; 65536];
                let result = stdout.read(&mut buffer);
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        buffer.truncate(n);
                        if send.send(Ok(buffer)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = send.send(Err(e));
                        break;
                    }
                }
            }
        });
        let mut pipe = Self {
            child,
            job: Some(job),
            receiver,
            buffered: vec![],
            offset: 0,
            cancel,
            started: Instant::now(),
            reader: Some(reader),
        };
        let bytes =
            serde_json::to_vec(request).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
        pipe.child
            .stdin
            .take()
            .ok_or_else(|| Fault::new(FaultCode::IoError))?
            .write_all(&bytes)
            .map_err(Fault::from)?;
        Ok(pipe)
    }
    pub(super) fn finish(&mut self) -> Result<(), Fault> {
        let mut extra = [0];
        if self.read(&mut extra).map_err(Fault::from)? != 0 {
            return Err(Fault::new(FaultCode::ProtocolError));
        }
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(Fault::new(FaultCode::Cancelled));
            }
            if self.started.elapsed() > Duration::from_secs(1800) {
                return Err(Fault::new(FaultCode::Timeout));
            }
            if let Some(status) = self.child.try_wait().map_err(Fault::from)? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(Fault::new(FaultCode::IoError))
                };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Read for ArchivePipe<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.offset == self.buffered.len() {
            if self.cancel.load(Ordering::Acquire) {
                return Err(io::Error::other("cancelled"));
            }
            if self.started.elapsed() > Duration::from_secs(1800) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "archive_timeout"));
            }
            match self.receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(buffer) => {
                    self.buffered = buffer?;
                    self.offset = 0;
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = output.len().min(self.buffered.len() - self.offset);
        output[..n].copy_from_slice(&self.buffered[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}
impl Drop for ArchivePipe<'_> {
    fn drop(&mut self) {
        drop(self.job.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Drain the bounded channel so a blocked sender can observe pipe closure.
        while self
            .receiver
            .recv_timeout(Duration::from_millis(100))
            .is_ok()
        {}
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
