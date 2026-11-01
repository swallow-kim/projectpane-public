use super::{WatchReply, WatchRequest, contracts::WATCH_FRAME};
use crate::{
    domain::{Fault, FaultCode},
    platform::worker_job::WorkerJob,
};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(2);

struct Pipes {
    child: Child,
    _job: WorkerJob,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

/// One isolated watch worker; callers must bound ownership and polling frequency.
/// No automatic restart or background task. Dropped/cancelled exchanges kill their worker.
pub struct WatchSession {
    pipes: Option<Pipes>,
    generation: u32,
    slots: usize,
    unavailable: u8,
}

impl WatchSession {
    pub async fn spawn(executable: &Path) -> Result<Self, Fault> {
        let mut command = Command::new(executable);
        command.arg("--watch");
        Self::spawn_command(command).await
    }

    async fn spawn_command(mut command: Command) -> Result<Self, Fault> {
        let job = WorkerJob::new().map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x00000008)
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        job.assign(&mut child)
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Fault::new(FaultCode::WorkerUnavailable))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| Fault::new(FaultCode::WorkerUnavailable))?;
        Ok(Self {
            pipes: Some(Pipes {
                child,
                _job: job,
                input,
                output: BufReader::new(output),
            }),
            generation: 0,
            slots: 0,
            unavailable: 0,
        })
    }

    pub fn process_id(&self) -> Option<u32> {
        self.pipes.as_ref().and_then(|pipes| pipes.child.id())
    }

    pub fn unavailable(&self) -> u8 {
        self.unavailable
    }

    pub async fn replace(&mut self, generation: u32, paths: Vec<String>) -> Result<u8, Fault> {
        let request = WatchRequest::Replace { generation, paths };
        request.validate()?;
        if generation <= self.generation {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        let slots = match &request {
            WatchRequest::Replace { paths, .. } => paths.len(),
            _ => 0,
        };
        match self.exchange(request).await? {
            WatchReply::Configured {
                watched,
                unavailable,
                ..
            } => {
                self.generation = generation;
                self.slots = slots;
                self.unavailable = unavailable;
                Ok(watched)
            }
            _ => Err(Fault::new(FaultCode::ProtocolError)),
        }
    }

    pub async fn poll(&mut self) -> Result<u8, Fault> {
        let request = WatchRequest::Poll {
            generation: self.generation,
        };
        request.validate()?;
        match self.exchange(request).await? {
            WatchReply::Polled {
                changed,
                unavailable,
                ..
            } => {
                self.unavailable = unavailable;
                Ok(changed)
            }
            _ => Err(Fault::new(FaultCode::ProtocolError)),
        }
    }

    async fn exchange(&mut self, request: WatchRequest) -> Result<WatchReply, Fault> {
        // The future owns the pipes before its first await; cancellation cannot
        // leave a partially sent request available for a subsequent exchange.
        let mut pipes = self
            .pipes
            .take()
            .ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
        let io = async {
            let mut frame =
                serde_json::to_vec(&request).map_err(|_| Fault::new(FaultCode::ProtocolError))?;
            frame.push(b'\n');
            if u64::try_from(frame.len()).map_or(true, |length| length > WATCH_FRAME) {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            pipes.input.write_all(&frame).await?;
            pipes.input.flush().await?;
            let mut response = Vec::new();
            (&mut pipes.output)
                .take(WATCH_FRAME + 1)
                .read_until(b'\n', &mut response)
                .await?;
            if u64::try_from(response.len()).map_or(true, |length| length > WATCH_FRAME)
                || !response.ends_with(b"\n")
            {
                return Err(Fault::new(FaultCode::ProtocolError));
            }
            let reply = serde_json::from_slice::<Result<WatchReply, Fault>>(&response)
                .map_err(|_| Fault::new(FaultCode::ProtocolError))??;
            reply.validate(&request, self.slots)?;
            Ok(reply)
        };
        match timeout(DEADLINE, io).await {
            Ok(Ok(reply)) => {
                self.pipes = Some(pipes);
                Ok(reply)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(Fault::new(FaultCode::Timeout)),
        }
    }

    /// Normal EOF releases notification handles; a failed deadline drops the kill-on-close job.
    pub async fn stop(mut self) -> Result<(), Fault> {
        let Some(mut pipes) = self.pipes.take() else {
            return Ok(());
        };
        let close = async {
            pipes.input.shutdown().await?;
            drop(pipes.input);
            let status = pipes.child.wait().await?;
            if status.success() {
                Ok(())
            } else {
                Err(Fault::new(FaultCode::WorkerUnavailable))
            }
        };
        timeout(DEADLINE, close)
            .await
            .map_err(|_| Fault::new(FaultCode::Timeout))?
    }
}

#[cfg(all(test, not(miri)))]
mod tests;
