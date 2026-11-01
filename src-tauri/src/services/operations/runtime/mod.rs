mod driver;
#[cfg(test)]
mod external_move;
mod preview;
mod transfer;

use super::*;
use crate::{
    domain::{ActivationTarget, Fault, FaultCode},
    platform::file_operations::{NativeFileOperationLane, NativeProgress, NativeProgressSnapshot},
};
pub use preview::OperationPreview;
use std::{
    ffi::OsStr,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCloseState {
    Open,
    Deciding,
    Waiting,
    Stopping,
    Stopped,
    Blocked,
    ResultsUnsaved,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRuntimeState {
    pub operations: Vec<OperationSnapshot>,
    pub close_state: OperationCloseState,
    pub available: bool,
    pub progress: Option<RunningProgress>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProgress {
    pub operation_id: String,
    #[serde(flatten)]
    pub estimate: NativeProgressSnapshot,
}

struct State {
    #[cfg(test)]
    external: Option<external_move::Pending>,
    #[cfg(test)]
    external_notices: BTreeMap<String, external_move::Notice>,
    #[cfg(test)]
    external_revocation: Option<(String, crate::platform::explorer::external_move::Revocation)>,
    queue: OperationQueue,
    close: OperationCloseState,
    progress: Option<(String, Arc<NativeProgress>)>,
    transfer: Option<transfer::CapturedTransfer>,
    transfer_sequence: u64,
}

pub struct OperationService {
    state: Mutex<State>,
    lane: NativeFileOperationLane,
    wake: Notify,
    started: AtomicBool,
}

impl OperationService {
    pub fn open(root: &Path, override_path: Option<&OsStr>) -> Result<Arc<Self>, Fault> {
        let (journal, recovered) =
            OperationJournal::open(root, override_path).map_err(Fault::from)?;
        let queue = OperationQueue::new(
            journal,
            recovered,
            crate::services::id::new_id().map_err(Fault::from)?,
        )?;
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                #[cfg(test)]
                external: None,
                #[cfg(test)]
                external_notices: BTreeMap::new(),
                #[cfg(test)]
                external_revocation: None,
                queue,
                close: OperationCloseState::Open,
                progress: None,
                transfer: None,
                transfer_sequence: 0,
            }),
            lane: NativeFileOperationLane::start()?,
            wake: Notify::new(),
            started: AtomicBool::new(false),
        }))
    }

    async fn access<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&mut State) -> Result<T, Fault> + Send + 'static,
    ) -> Result<T, Fault> {
        let service = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let mut state = service
                .state
                .lock()
                .map_err(|_| Fault::new(FaultCode::InvalidState))?;
            f(&mut state)
        })
        .await
        .map_err(|_| Fault::new(FaultCode::InvalidState))?
    }

    pub async fn prepare(
        self: &Arc<Self>,
        request: PrepareOperation,
        targets: Vec<ActivationTarget>,
    ) -> Result<OperationPreview, Fault> {
        self.access(|state| state.queue.admission_ready()).await?;
        let plan = self.lane.prepare(request, targets).await?;
        self.access(move |state| {
            let id = state.queue.prepare(plan.clone())?;
            match OperationPreview::new(id.clone(), &plan) {
                Ok(preview) => Ok(preview),
                Err(error) => {
                    state.queue.discard_prepared(&id);
                    Err(error)
                }
            }
        })
        .await
    }

    pub async fn submit(self: &Arc<Self>, id: String) -> Result<OperationSnapshot, Fault> {
        let result = self
            .access(move |state| {
                let result = state.queue.submit(&id)?;
                if state
                    .transfer
                    .as_ref()
                    .and_then(|transfer| transfer.prepared.as_ref())
                    == Some(&id)
                {
                    state.transfer = None;
                }
                Ok(result)
            })
            .await;
        self.wake.notify_one();
        result
    }

    pub async fn discard(self: &Arc<Self>, id: String) -> Result<(), Fault> {
        self.access(move |state| {
            state.queue.discard_prepared(&id);
            if let Some(transfer) = &mut state.transfer
                && transfer.prepared.as_ref() == Some(&id)
            {
                transfer.prepared = None;
            }
            Ok(())
        })
        .await
    }

    pub async fn cancel(self: &Arc<Self>, id: String) -> Result<OperationSnapshot, Fault> {
        let result = self
            .access(move |state| {
                #[cfg(test)]
                if let Some((active, control)) = &state.external_revocation
                    && active == &id
                {
                    control.cancel();
                }
                state.queue.request_cancel(&id)
            })
            .await;
        self.wake.notify_one();
        result
    }

    pub async fn state(self: &Arc<Self>) -> Result<OperationRuntimeState, Fault> {
        self.access(|state| {
            Ok(OperationRuntimeState {
                operations: state.queue.snapshots(),
                close_state: state.close,
                available: !state.queue.faulted && state.close == OperationCloseState::Open,
                progress: state.progress.as_ref().and_then(|(id, tracker)| {
                    tracker.snapshot().map(|estimate| RunningProgress {
                        operation_id: id.clone(),
                        estimate,
                    })
                }),
            })
        })
        .await
    }

    pub async fn report(self: &Arc<Self>, id: String) -> Result<Option<OperationReport>, Fault> {
        self.access(move |state| Ok(state.queue.report(&id).cloned()))
            .await
    }

    pub async fn forget(self: &Arc<Self>, id: String) -> Result<(), Fault> {
        self.access(move |state| state.queue.forget(&id)).await
    }

    pub async fn request_close(
        self: &Arc<Self>,
        cancel: bool,
    ) -> Result<OperationCloseState, Fault> {
        let result = self
            .access(move |state| {
                if matches!(
                    state.close,
                    OperationCloseState::Open | OperationCloseState::Deciding
                ) {
                    state.close = OperationCloseState::Waiting;
                }
                state.queue.begin_close();
                if cancel {
                    #[cfg(test)]
                    if let Some((_, control)) = &state.external_revocation {
                        control.cancel();
                    }
                    for item in state.queue.snapshots() {
                        state.queue.request_cancel(&item.id)?;
                    }
                }
                Ok(state.close)
            })
            .await;
        self.wake.notify_one();
        result
    }

    pub async fn acknowledge_unsaved_results(self: &Arc<Self>) -> Result<(), Fault> {
        self.access(|state| {
            if state.close != OperationCloseState::ResultsUnsaved || !state.queue.faulted {
                return Err(Fault::new(FaultCode::InvalidState));
            }
            state.close = OperationCloseState::Stopped;
            Ok(())
        })
        .await
    }

    pub async fn keep_open(self: &Arc<Self>) -> Result<(), Fault> {
        self.access(|state| {
            if !matches!(
                state.close,
                OperationCloseState::Open
                    | OperationCloseState::Deciding
                    | OperationCloseState::Waiting
            ) {
                return Err(Fault::new(FaultCode::Busy));
            }
            state.close = OperationCloseState::Open;
            state.queue.keep_open();
            Ok(())
        })
        .await
    }

    pub async fn ask_close(self: &Arc<Self>) -> Result<OperationCloseState, Fault> {
        let result = self
            .access(|state| {
                if state.close == OperationCloseState::Open {
                    state.queue.begin_close();
                    state.close = if state.queue.is_idle() {
                        OperationCloseState::Waiting
                    } else {
                        OperationCloseState::Deciding
                    };
                }
                Ok(state.close)
            })
            .await;
        self.wake.notify_one();
        result
    }
}

#[cfg(test)]
mod skip_existing_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transfer_tests;
