//! Test-only connection to the real queue, driver, cancellation and journal.
//! No IPC route; no system clipboard access; no replay/automatic rollback.
use super::*;
use crate::platform::{
    explorer::external_move::Session,
    file_operations::guarded_move::{GuardedMove, Probe, Stage},
};

pub(super) struct Pending {
    id: String,
    session: Session,
    guards: GuardedMove,
    probe: Probe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Notice {
    Suppressed,
    Confirmed,
    Uncertain,
}

pub(super) fn release_cancelled(state: &mut State) {
    if state.external.as_ref().is_some_and(|pending| {
        state
            .queue
            .snapshot(&pending.id)
            .is_some_and(|s| s.status == OperationStatus::Cancelled)
    }) {
        state.external = None; // closes STA session and drops handles, no move/no notice
        state.external_revocation = None;
        state.queue.external_reserved = false;
    }
}

impl OperationService {
    async fn submit_external(
        self: &Arc<Self>,
        session: Session,
        destination: String,
        probe: Probe,
    ) -> Result<OperationSnapshot, Fault> {
        let plan = self
            .lane
            .prepare_transfer(
                PrepareTransfer {
                    token: "external-move-prototype".into(),
                    project_id: None,
                    destination,
                    copy_name: None,
                    skip_existing: false,
                },
                TransferMode::Move,
                session.sources(),
            )
            .await?;
        let result = self
            .access(move |state| {
                state.queue.admission_ready()?;
                if !state.queue.is_idle()
                    || !state.queue.prepared.is_empty()
                    || state.external.is_some()
                    || state
                        .queue
                        .snapshots()
                        .iter()
                        .any(|s| s.status == OperationStatus::InterruptedUnknown)
                {
                    return Err(Fault::new(FaultCode::Busy));
                }
                let guards = GuardedMove::acquire(
                    plan.clone(),
                    &[
                        state.queue.journal.directory().to_owned(),
                        session.journal_root().to_owned(),
                    ],
                )?;
                let id = state.queue.prepare(plan)?;
                // Standard intent must be durable before protocol arm or any mutation.
                let snapshot = state.queue.submit(&id)?;
                state.external_revocation = Some((id.clone(), session.revocation()));
                state.external = Some(Pending {
                    id,
                    session,
                    guards,
                    probe,
                });
                state.queue.external_reserved = true;
                Ok(snapshot)
            })
            .await;
        self.wake.notify_one();
        result
    }

    pub(super) async fn execute_external(
        self: &Arc<Self>,
        work: &OperationWork,
    ) -> Result<bool, Fault> {
        let id = work.id.clone();
        let pending = self
            .access(move |state| {
                if state.external.as_ref().is_some_and(|p| p.id == id) {
                    Ok(state.external.take())
                } else {
                    Ok(None)
                }
            })
            .await?;
        let Some(pending) = pending else {
            return Ok(false);
        };
        let id = work.id.clone();
        let cancel = work.cancel.clone();
        let (pending, report) = tokio::task::spawn_blocking(move || {
            let report = match pending.session.bind(id, pending.guards.targets()) {
                Ok(_) => {
                    (pending.probe)(Stage::Armed);
                    pending
                        .guards
                        .execute(&cancel, &pending.session.cancellation(), &pending.probe)
                }
                Err(error) => OperationReport {
                    failure: Some(error),
                    counts: OutcomeCounts::default(),
                    items: vec![],
                    user_skipped: vec![],
                    user_decisions_only: None,
                    evidence: FinishEvidence {
                        perform_succeeded: false,
                        aborted: Some(false),
                        outcomes_complete: true,
                        cancellation_confirmed: false,
                    },
                },
            };
            (pending.probe)(Stage::BeforeResult);
            (pending, report)
        })
        .await
        .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let id = work.id.clone();
        let success = report.status() == OperationStatus::Succeeded;
        let snapshot = self
            .access(move |state| {
                let snapshot = state.queue.finish(&id, report)?;
                state.progress = None;
                Ok(snapshot)
            })
            .await?;
        let cancelled = work.cancel.clone();
        let (pending, notice) = tokio::task::spawn_blocking(move || {
            (pending.probe)(Stage::AfterResult);
            // Files and both journal directory chains stay protected here.
            // Persisted file outcomes never imply external acknowledgement.
            let notice = if snapshot.journal_saved && success && !cancelled.load(Ordering::Acquire)
            {
                match pending.session.finish() {
                    Ok(record) if record.notified() => Notice::Confirmed,
                    Ok(_) => Notice::Suppressed,
                    Err(_) => Notice::Uncertain, // terminal, never retried
                }
            } else {
                pending.session.close();
                Notice::Suppressed
            };
            (pending, notice)
        })
        .await
        .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        // Expose uncertainty while a provider is still blocked. Keep admission
        // and guards held; the durable timeout witness forbids restart replay.
        let id = work.id.clone();
        self.access(move |state| {
            state.external_notices.insert(id, notice);
            Ok(())
        })
        .await?;
        let notice = tokio::task::spawn_blocking(move || {
            let released = pending.session.wait_until_released();
            (pending.probe)(Stage::AfterNotice);
            drop(pending); // guards release only after the STA has released its object
            if released.is_ok() {
                notice
            } else {
                Notice::Uncertain
            }
        })
        .await
        .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let id = work.id.clone();
        self.access(move |state| {
            state.external_notices.insert(id, notice);
            state.queue.external_reserved = false;
            state.external_revocation = None;
            Ok(())
        })
        .await?;
        Ok(true)
    }
}

mod boundary_tests;
#[cfg(test)]
mod tests;
