use super::*;
use std::sync::atomic::Ordering;

impl OperationQueue {
    pub fn forget(&mut self, id: &str) -> Result<(), Fault> {
        self.admission_ready()?;
        let Some(record) = self.records.get(id) else {
            return Ok(());
        };
        if !record.snapshot.journal_saved
            || !matches!(
                record.snapshot.status,
                OperationStatus::Succeeded
                    | OperationStatus::PartiallySucceeded
                    | OperationStatus::Cancelled
                    | OperationStatus::Failed
            )
        {
            return Err(Fault::new(FaultCode::Busy));
        }
        let completion = OperationCompletion {
            schema_version: 1,
            id: id.into(),
            report: record
                .report
                .clone()
                .ok_or_else(|| Fault::new(FaultCode::InvalidState))?,
        };
        if self.journal.retire(&completion).is_err() {
            self.faulted = true;
            return Err(Fault::new(FaultCode::IoError));
        }
        self.records.remove(id);
        Ok(())
    }

    pub fn request_cancel(&mut self, id: &str) -> Result<OperationSnapshot, Fault> {
        let record = self
            .records
            .get_mut(id)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        match record.snapshot.status {
            OperationStatus::Queued => {
                self.pending.retain(|queued| queued != id);
                self.complete_record(
                    id,
                    OperationReport {
                        failure: None,
                        user_skipped: vec![],
                        user_decisions_only: None,
                        counts: OutcomeCounts::default(),
                        items: vec![],
                        evidence: FinishEvidence {
                            perform_succeeded: false,
                            aborted: Some(true),
                            outcomes_complete: true,
                            cancellation_confirmed: true,
                        },
                    },
                )
            }
            OperationStatus::Running
            | OperationStatus::CancelRequested
            | OperationStatus::AwaitingUserDecision => {
                record.cancel.store(true, Ordering::Release);
                record.snapshot.status = OperationStatus::CancelRequested;
                Ok(record.snapshot.clone())
            }
            OperationStatus::Succeeded
            | OperationStatus::PartiallySucceeded
            | OperationStatus::Cancelled
            | OperationStatus::Failed
            | OperationStatus::InterruptedUnknown => Ok(record.snapshot.clone()),
        }
    }

    pub fn finish(
        &mut self,
        id: &str,
        report: OperationReport,
    ) -> Result<OperationSnapshot, Fault> {
        if self.running.as_deref() != Some(id) {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        report.validate_for_plan(
            &self
                .records
                .get(id)
                .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?
                .intent
                .plan,
        )?;
        let result = self.complete_record(id, report)?;
        self.running = None;
        Ok(result)
    }

    pub fn begin_close(&mut self) {
        self.closing = true;
        self.prepared.clear();
    }

    pub fn keep_open(&mut self) {
        self.closing = false;
    }

    pub fn is_idle(&self) -> bool {
        self.running.is_none() && self.pending.is_empty()
    }

    fn complete_record(
        &mut self,
        id: &str,
        report: OperationReport,
    ) -> Result<OperationSnapshot, Fault> {
        let record = self
            .records
            .get_mut(id)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        let completion = OperationCompletion {
            schema_version: 1,
            id: id.into(),
            report,
        };
        let saved = self.journal.record_completion(&completion).is_ok();
        self.faulted |= !saved;
        record.snapshot.status = completion.report.status();
        record.snapshot.counts = completion.report.counts;
        record.snapshot.journal_saved = saved;
        record.report = Some(completion.report);
        Ok(record.snapshot.clone())
    }
}
