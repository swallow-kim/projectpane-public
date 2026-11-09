#[cfg(windows)]
use super::worker_job;
use crate::domain::*;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex as AsyncMutex, watch},
    time::timeout,
};
pub(crate) struct Pipes {
    child: Child,
    #[cfg(windows)]
    _job: worker_job::WorkerJob,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub(crate) struct Session {
    pub(crate) identity: Identity,
    pub(crate) pipes: AsyncMutex<Pipes>,
    pub(crate) cancelled: watch::Sender<bool>,
}

impl Session {
    pub(crate) async fn stop_confirmed(&self) -> Result<(), Fault> {
        self.cancelled.send_replace(true);
        let mut pipes = self.pipes.lock().await;
        if pipes.child.try_wait().map_err(Fault::from)?.is_none() {
            pipes.child.kill().await.map_err(Fault::from)?;
        }
        pipes.child.wait().await.map_err(Fault::from)?;
        Ok(())
    }

    pub(crate) async fn stop(&self) {
        self.cancelled.send_replace(true);
        let mut pipes = self.pipes.lock().await;
        let _ = pipes.child.kill().await;
        let _ = pipes.child.wait().await;
    }

    pub(crate) async fn exchange(
        &self,
        request: Request,
        deadline: Duration,
    ) -> Result<Reply, Fault> {
        match self.exchange_response(request, deadline).await? {
            WorkerResponse::Batch(batch) => Ok(Reply {
                identity: self.identity.clone(),
                batch,
            }),
            WorkerResponse::Located(_) | WorkerResponse::Target(_) | WorkerResponse::Targets(_) => {
                Err(Fault::new(FaultCode::ProtocolError))
            }
        }
    }

    pub(crate) async fn exchange_response(
        &self,
        request: Request,
        deadline: Duration,
    ) -> Result<WorkerResponse, Fault> {
        // No waiting IPC queue: only one outstanding request per worker.
        let mut pipes = self
            .pipes
            .try_lock()
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        let mut cancel = self.cancelled.subscribe();
        if *cancel.borrow() {
            return Err(Fault::new(FaultCode::Cancelled));
        }
        let io = async {
            let mut data =
                serde_json::to_vec(&request).map_err(|_| Fault::new(FaultCode::ProtocolError))?;
            if data.len() >= MAX_FRAME {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            data.push(b'\n');
            pipes.input.write_all(&data).await?;
            pipes.input.flush().await?;
            let mut line = Vec::new();
            (&mut pipes.output)
                .take(MAX_FRAME as u64 + 1)
                .read_until(b'\n', &mut line)
                .await?;
            if line.len() > MAX_FRAME || !line.ends_with(b"\n") {
                return Err(Fault::new(FaultCode::ProtocolError));
            }
            let response = serde_json::from_slice::<Result<WorkerResponse, Fault>>(&line)
                .map_err(|_| Fault::new(FaultCode::ProtocolError))??;
            response.validate(&request)?;
            Ok(response)
        };
        let result = tokio::select! {
            biased;
            _ = cancel.changed() => Err(Fault::new(FaultCode::Cancelled)),
            result = timeout(deadline, io) => result.unwrap_or_else(|_| Err(Fault::new(FaultCode::Timeout))),
        };
        if result.as_ref().is_err_and(|fault| {
            !matches!(
                fault.code,
                FaultCode::InvalidRequest
                    | FaultCode::InvalidPath
                    | FaultCode::NotFound
                    | FaultCode::AccessDenied
            )
        }) {
            self.cancelled.send_replace(true);
            let _ = pipes.child.kill().await;
            let _ = pipes.child.wait().await;
        }
        result
    }
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;

impl Session {
    pub(crate) async fn spawn(executable: &Path, identity: Identity) -> Result<Self, Fault> {
        let mut command = Command::new(executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        // DETACHED_PROCESS avoids creating a hidden console/conhost. Explicit
        // stdin/stdout pipe handles remain usable; do not combine CREATE_NO_WINDOW.
        command.creation_flags(0x00000008);
        #[cfg(windows)]
        let job =
            worker_job::WorkerJob::new().map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let mut child = command
            .spawn()
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        #[cfg(windows)]
        if job.assign(&mut child).await.is_err() {
            return Err(Fault::new(FaultCode::WorkerUnavailable));
        }
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Fault::new(FaultCode::WorkerUnavailable))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| Fault::new(FaultCode::WorkerUnavailable))?;
        Ok(Self {
            identity,
            pipes: AsyncMutex::new(Pipes {
                child,
                #[cfg(windows)]
                _job: job,
                input,
                output: BufReader::new(output),
            }),
            cancelled: watch::channel(false).0,
        })
    }
}
