use super::*;

enum Next {
    Work(OperationWork, Arc<NativeProgress>),
    Stop,
    StorageFailed,
    Wait,
}

struct RunGuard(Arc<OperationService>);
impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock()
            && !matches!(
                state.close,
                OperationCloseState::Stopped | OperationCloseState::ResultsUnsaved
            )
        {
            state.queue.faulted = true;
            state.close = OperationCloseState::Blocked;
        }
    }
}

impl OperationService {
    pub async fn run(self: Arc<Self>, owner: usize) -> Result<(), Fault> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(Fault::new(FaultCode::InvalidState));
        }
        let _guard = RunGuard(Arc::clone(&self));
        loop {
            let notified = self.wake.notified();
            match self
                .access(|state| {
                    if state.queue.faulted {
                        return Ok(Next::StorageFailed);
                    }
                    #[cfg(test)]
                    external_move::release_cancelled(state);
                    if let Some(work) = state.queue.start_next() {
                        let progress = Arc::default();
                        state.progress = Some((work.id.clone(), Arc::clone(&progress)));
                        return Ok(Next::Work(work, progress));
                    }
                    if state.close == OperationCloseState::Waiting && state.queue.is_idle() {
                        state.close = OperationCloseState::Stopping;
                        return Ok(Next::Stop);
                    }
                    Ok(Next::Wait)
                })
                .await?
            {
                Next::Work(work, progress) => {
                    #[cfg(test)]
                    if self.execute_external(&work).await? {
                        continue;
                    }
                    let result = self
                        .lane
                        .execute_observed(work.plan, owner, work.cancel, progress)
                        .await;
                    self.access(move |state| {
                        let (report, lost) = match result {
                            Ok(report) => (report, false),
                            Err(error) => {
                                let lost = matches!(
                                    error.code,
                                    FaultCode::WorkerUnavailable | FaultCode::InvalidState
                                );
                                (
                                    OperationReport {
                                        failure: Some(error),
                                        user_skipped: vec![],
                                        user_decisions_only: None,
                                        counts: OutcomeCounts::default(),
                                        items: vec![],
                                        evidence: FinishEvidence {
                                            perform_succeeded: false,
                                            aborted: if lost { None } else { Some(false) },
                                            outcomes_complete: !lost,
                                            cancellation_confirmed: false,
                                        },
                                    },
                                    lost,
                                )
                            }
                        };
                        state.queue.finish(&work.id, report)?;
                        state.progress = None;
                        if lost {
                            state.queue.faulted = true;
                            state.close = OperationCloseState::Blocked;
                            return Err(Fault::new(FaultCode::WorkerUnavailable));
                        }
                        Ok(())
                    })
                    .await?;
                }
                Next::Stop => {
                    self.lane.shutdown().await?;
                    self.access(|state| {
                        state.close = OperationCloseState::Stopped;
                        Ok(())
                    })
                    .await?;
                    return Ok(());
                }
                Next::StorageFailed => {
                    self.lane.shutdown().await?;
                    self.access(|state| {
                        state.close = OperationCloseState::ResultsUnsaved;
                        Ok(())
                    })
                    .await?;
                    return Err(Fault::new(FaultCode::IoError));
                }
                Next::Wait => notified.await,
            }
        }
    }
}
