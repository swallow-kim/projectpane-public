use crate::{
    domain::{Fault, FaultCode},
    platform::change_watch::{WatchRequest, WatchSession},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchUpdate {
    pub changed: u8,
    pub unavailable: u8,
}

#[derive(Default)]
struct State {
    session: Option<WatchSession>,
    paths: Vec<String>,
    generation: u32,
    last: Option<Instant>,
    failed: bool,
}

/// One application-owned worker. Busy requests are rejected, never queued.
pub struct WatchHub {
    executable: PathBuf,
    state: Mutex<State>,
    closed: AtomicBool,
}

impl WatchHub {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            state: Mutex::new(State::default()),
            closed: AtomicBool::new(false),
        }
    }

    pub async fn poll(&self, paths: Vec<String>) -> Result<WatchUpdate, Fault> {
        WatchRequest::Replace {
            generation: 1,
            paths: paths.clone(),
        }
        .validate()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        if self.closed.load(Ordering::Acquire) || state.failed {
            return Err(Fault::new(FaultCode::WorkerUnavailable));
        }
        if state
            .last
            .is_some_and(|last| last.elapsed() < Duration::from_millis(400))
        {
            return Err(Fault::new(FaultCode::Busy));
        }
        state.last = Some(Instant::now());
        // Cancellation must leave a terminal state and drop the sole worker owner.
        state.failed = true;
        let mut session = state.session.take();
        let changed = if paths.is_empty() {
            if let Some(session) = session.take() {
                session.stop().await?;
            }
            0
        } else {
            if session.is_none() {
                session = Some(WatchSession::spawn(&self.executable).await?);
            }
            let worker = session
                .as_mut()
                .ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
            if paths != state.paths {
                state.generation = state
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
                worker.replace(state.generation, paths.clone()).await?;
                // Revalidate once after admission to cover the listing-before-watch gap.
                (1u8 << paths.len()) - 1
            } else {
                worker.poll().await?
            }
        };
        if self.closed.load(Ordering::Acquire) {
            return Err(Fault::new(FaultCode::Cancelled));
        }
        let unavailable = session.as_ref().map_or(0, WatchSession::unavailable);
        state.paths = paths;
        state.session = session;
        state.failed = false;
        Ok(WatchUpdate {
            changed: changed & !unavailable,
            unavailable,
        })
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut state) = self.state.try_lock() {
            state.session.take();
        }
        // An in-flight exchange owns the Job, observes closed on return, and has a 2s deadline.
    }
}
