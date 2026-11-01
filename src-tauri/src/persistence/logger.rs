use super::log_files::LogFiles;
use crate::domain::FaultCode;
use serde::Serialize;
use std::{
    fmt,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    AppStarted,
    DuplicateInstance,
    InstanceFocused,
    InstanceFocusFailed,
    ListingStarted,
    ListingFailed,
    CancelFailed,
    WindowStateLoaded,
    WindowStateSaved,
    WindowStateInvalid,
    WindowStateUnavailable,
    WindowRestoreFailed,
    AppStopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub event: EventKind,
    pub pane: Option<u8>,
    pub code: Option<FaultCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogError {
    Io,
    Busy,
    Timeout,
    Unavailable,
}
impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "log IO failed",
            Self::Busy => "log writer already active",
            Self::Timeout => "log drain timed out",
            Self::Unavailable => "log writer unavailable",
        })
    }
}
impl std::error::Error for LogError {}
impl From<std::io::Error> for LogError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

type Completion = (Mutex<Option<Result<(), LogError>>>, Condvar);
struct Shared {
    sender: Mutex<Option<SyncSender<LogEvent>>>,
    completion: Arc<Completion>,
    dropped: AtomicU64,
}

#[derive(Clone)]
pub struct BoundedLogger {
    shared: Arc<Shared>,
}

impl BoundedLogger {
    pub fn start(directory: PathBuf) -> Result<Self, LogError> {
        let (sender, receiver) = mpsc::sync_channel(128);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let completion = Arc::new((Mutex::new(None), Condvar::new()));
        let finished = Arc::clone(&completion);
        std::thread::Builder::new()
            .name("foundation-log".into())
            .spawn(move || {
                let result = match LogFiles::open(directory) {
                    Ok(files) => {
                        if ready_tx.send(Ok(())).is_err() {
                            return;
                        }
                        drain(&files, receiver)
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        Err(error)
                    }
                };
                if let Ok(mut status) = finished.0.lock() {
                    *status = Some(result);
                    finished.1.notify_all();
                }
            })?;
        wait_for_ready(ready_rx)?;
        Ok(Self {
            shared: Arc::new(Shared {
                sender: Mutex::new(Some(sender)),
                completion,
                dropped: AtomicU64::new(0),
            }),
        })
    }

    pub fn record(&self, event: LogEvent) {
        let sent = self.shared.sender.try_lock().ok().is_some_and(|sender| {
            sender
                .as_ref()
                .is_some_and(|sender| sender.try_send(event).is_ok())
        });
        if !sent {
            let _ =
                self.shared
                    .dropped
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                        Some(count.saturating_add(1))
                    });
        }
    }

    pub fn dropped_count(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    pub fn shutdown(&self, timeout: Duration) -> Result<(), LogError> {
        self.shared
            .sender
            .lock()
            .map_err(|_| LogError::Unavailable)?
            .take();
        let (lock, condition) = &*self.shared.completion;
        let status = lock.lock().map_err(|_| LogError::Unavailable)?;
        let (status, _) = condition
            .wait_timeout_while(status, timeout.min(Duration::from_secs(2)), |result| {
                result.is_none()
            })
            .map_err(|_| LogError::Unavailable)?;
        (*status).unwrap_or(Err(LogError::Timeout))
    }
}

fn wait_for_ready(receiver: mpsc::Receiver<Result<(), LogError>>) -> Result<(), LogError> {
    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => LogError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => LogError::Unavailable,
        })?
}

fn drain(files: &LogFiles, receiver: mpsc::Receiver<LogEvent>) -> Result<(), LogError> {
    for event in receiver {
        files.write(&event)?;
    }
    files.sync()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_wait_times_out_when_worker_never_responds() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert_eq!(wait_for_ready(receiver), Err(LogError::Timeout));
        assert!(sender.send(Ok(())).is_err());
    }

    #[test]
    fn ready_wait_preserves_startup_error_and_disconnect() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(Err(LogError::Busy)).unwrap();
        assert_eq!(wait_for_ready(receiver), Err(LogError::Busy));
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        assert_eq!(wait_for_ready(receiver), Err(LogError::Unavailable));
    }

    #[test]
    fn full_queue_drops_without_waiting_for_a_consumer() {
        let (sender, receiver) = mpsc::sync_channel(128);
        let logger = BoundedLogger {
            shared: Arc::new(Shared {
                sender: Mutex::new(Some(sender)),
                completion: Arc::new((Mutex::new(None), Condvar::new())),
                dropped: AtomicU64::new(0),
            }),
        };
        for _ in 0..140 {
            logger.record(LogEvent {
                event: EventKind::AppStarted,
                pane: None,
                code: None,
            });
        }
        assert_eq!(logger.dropped_count(), 12);
        assert_eq!(receiver.try_iter().count(), 128);
    }

    #[test]
    fn shutdown_reports_a_writer_that_cannot_finish() {
        let (sender, _receiver) = mpsc::sync_channel(128);
        let logger = BoundedLogger {
            shared: Arc::new(Shared {
                sender: Mutex::new(Some(sender)),
                completion: Arc::new((Mutex::new(None), Condvar::new())),
                dropped: AtomicU64::new(0),
            }),
        };
        assert_eq!(logger.shutdown(Duration::ZERO), Err(LogError::Timeout));
    }
}
