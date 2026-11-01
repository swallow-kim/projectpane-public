mod lifecycle;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub use runtime::*;

use crate::{
    domain::{Fault, FaultCode, operations::*},
    persistence::operations::{OperationJournal, RecoveredOperation},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
};

pub const MAX_QUEUED_OPERATIONS: usize = 32;
pub const MAX_PREPARED_OPERATIONS: usize = 32;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSnapshot {
    pub id: String,
    pub project_id: Option<String>,
    pub status: OperationStatus,
    pub counts: OutcomeCounts,
    pub journal_saved: bool,
}

struct Record {
    intent: OperationIntent,
    snapshot: OperationSnapshot,
    report: Option<OperationReport>,
    cancel: Arc<AtomicBool>,
}

pub struct OperationWork {
    pub id: String,
    pub plan: OperationPlan,
    pub cancel: Arc<AtomicBool>,
}

pub struct OperationQueue {
    journal: OperationJournal,
    run_id: String,
    next_id: u64,
    prepared: BTreeMap<String, OperationPlan>,
    records: BTreeMap<String, Record>,
    pending: VecDeque<String>,
    running: Option<String>,
    closing: bool,
    faulted: bool,
    #[cfg(test)]
    external_reserved: bool,
}

impl OperationQueue {
    pub fn new(
        journal: OperationJournal,
        recovered: Vec<RecoveredOperation>,
        run_id: String,
    ) -> Result<Self, Fault> {
        if !valid_token(&run_id) || run_id.len() > 48 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let records = recovered
            .into_iter()
            .map(|recovered| {
                let status = recovered.status();
                let report = recovered.completion.map(|completion| completion.report);
                let intent = recovered.intent;
                let snapshot = OperationSnapshot {
                    id: intent.id.clone(),
                    project_id: intent.plan.project_id.clone(),
                    status,
                    counts: report.as_ref().map_or_default(|report| report.counts),
                    journal_saved: report.is_some(),
                };
                (
                    intent.id.clone(),
                    Record {
                        intent,
                        snapshot,
                        report,
                        cancel: Arc::new(AtomicBool::new(false)),
                    },
                )
            })
            .collect();
        Ok(Self {
            journal,
            run_id,
            next_id: 0,
            prepared: BTreeMap::new(),
            records,
            pending: VecDeque::new(),
            running: None,
            closing: false,
            faulted: false,
            #[cfg(test)]
            external_reserved: false,
        })
    }

    pub fn prepare(&mut self, plan: OperationPlan) -> Result<String, Fault> {
        self.admission_ready()?;
        plan.validate()?;
        if self.prepared.len() >= MAX_PREPARED_OPERATIONS
            || self.journal.remaining() <= self.prepared.len()
        {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
        let id = format!("{}-{}", self.run_id, self.next_id);
        if self.records.contains_key(&id) {
            return Err(Fault::new(FaultCode::InvalidState));
        }
        let intent = OperationIntent {
            schema_version: 1,
            id: id.clone(),
            plan: plan.clone(),
        };
        intent.validate()?;
        self.prepared.insert(id.clone(), plan);
        Ok(id)
    }

    pub fn discard_prepared(&mut self, id: &str) {
        self.prepared.remove(id);
    }

    pub fn submit(&mut self, id: &str) -> Result<OperationSnapshot, Fault> {
        if let Some(record) = self.records.get(id) {
            return Ok(record.snapshot.clone());
        }
        self.admission_ready()?;
        if self.pending.len() >= MAX_QUEUED_OPERATIONS {
            return Err(Fault::new(FaultCode::Busy));
        }
        let plan = self
            .prepared
            .get(id)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?
            .clone();
        let intent = OperationIntent {
            schema_version: 1,
            id: id.into(),
            plan,
        };
        if self.journal.record_intent(&intent).is_err() {
            self.faulted = true;
            return Err(Fault::new(FaultCode::IoError));
        }
        self.prepared.remove(id);
        let snapshot = OperationSnapshot {
            id: id.into(),
            project_id: intent.plan.project_id.clone(),
            status: OperationStatus::Queued,
            counts: OutcomeCounts::default(),
            journal_saved: true,
        };
        self.records.insert(
            id.into(),
            Record {
                intent,
                snapshot: snapshot.clone(),
                report: None,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        self.pending.push_back(id.into());
        Ok(snapshot)
    }

    pub fn start_next(&mut self) -> Option<OperationWork> {
        if self.running.is_some() || self.faulted {
            return None;
        }
        let id = self.pending.pop_front()?;
        let record = self.records.get_mut(&id)?;
        record.snapshot.status = OperationStatus::Running;
        self.running = Some(id.clone());
        Some(OperationWork {
            id,
            plan: record.intent.plan.clone(),
            cancel: Arc::clone(&record.cancel),
        })
    }

    pub fn snapshot(&self, id: &str) -> Option<OperationSnapshot> {
        self.records.get(id).map(|record| record.snapshot.clone())
    }

    pub fn report(&self, id: &str) -> Option<&OperationReport> {
        self.records
            .get(id)
            .and_then(|record| record.report.as_ref())
    }

    pub fn snapshots(&self) -> Vec<OperationSnapshot> {
        self.records
            .values()
            .map(|record| record.snapshot.clone())
            .collect()
    }

    fn admission_ready(&self) -> Result<(), Fault> {
        #[cfg(test)]
        if self.external_reserved {
            return Err(Fault::new(FaultCode::Busy));
        }
        if self.faulted {
            return Err(Fault::new(FaultCode::IoError));
        }
        if self.closing {
            return Err(Fault::new(FaultCode::Busy));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
