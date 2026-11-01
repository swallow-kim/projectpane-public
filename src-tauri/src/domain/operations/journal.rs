use super::{
    FinishEvidence, OperationPlan, OperationStatus, OutcomeCounts, classify_outcome, valid_token,
};
use crate::domain::{Fault, FaultCode, MAX_FRAME};
use serde::{Deserialize, Serialize};

pub const MAX_OPERATION_HISTORY: usize = 256;
pub const MAX_OPERATION_RESULTS: usize = 2048;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationIntent {
    pub schema_version: u32,
    pub id: String,
    pub plan: OperationPlan,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Changed,
    Skipped,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemOutcome {
    pub source: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub destination: Option<String>,
    pub status: ItemStatus,
    pub hresult: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Fault>,
    pub counts: OutcomeCounts,
    pub evidence: FinishEvidence,
    pub items: Vec<ItemOutcome>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "super::decisions::deserialize_decisions"
    )]
    pub user_skipped: Vec<super::UserSkipped>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::decisions::deserialize_only"
    )]
    pub user_decisions_only: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationCompletion {
    pub schema_version: u32,
    pub id: String,
    pub report: OperationReport,
}

impl OperationIntent {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.schema_version != 1 || !valid_token(&self.id) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        self.plan.validate()?;
        bounded_json(self).map(|_| ())
    }
}

impl OperationReport {
    pub fn status(&self) -> OperationStatus {
        match self.user_decisions_only {
            Some(true) if self.validate().is_ok() => OperationStatus::Succeeded,
            Some(_) => OperationStatus::InterruptedUnknown,
            None => classify_outcome(self.counts, self.evidence),
        }
    }

    pub fn validate(&self) -> Result<(), Fault> {
        if self.failure.is_some()
            && (self.counts != OutcomeCounts::default()
                || !self.items.is_empty()
                || self.evidence.perform_succeeded
                || self.evidence.cancellation_confirmed
                || self.evidence.aborted == Some(true))
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        if self.items.len() + self.user_skipped.len() > MAX_OPERATION_RESULTS {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        super::decisions::validate(self)?;
        let mut counts = OutcomeCounts {
            skipped: u32::try_from(self.user_skipped.len())
                .map_err(|_| Fault::new(FaultCode::ResourceLimit))?,
            ..OutcomeCounts::default()
        };
        for item in &self.items {
            if (item.status == ItemStatus::Changed && item.hresult < 0)
                || (item.status == ItemStatus::Failed && item.hresult >= 0)
            {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
            if item.source.is_empty()
                || item.source.contains('\0')
                || item.source.len() > 32768
                || item.destination.as_ref().is_some_and(|path| {
                    path.is_empty() || path.len() > 32768 || path.contains('\0')
                })
            {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            match item.status {
                ItemStatus::Changed => counts.changed += 1,
                ItemStatus::Skipped => counts.skipped += 1,
                ItemStatus::Failed => counts.failed += 1,
                ItemStatus::Unknown => counts.unknown += 1,
            }
        }
        let actual = [
            counts.changed,
            counts.skipped,
            counts.failed,
            counts.unknown,
        ];
        let claimed = [
            self.counts.changed,
            self.counts.skipped,
            self.counts.failed,
            self.counts.unknown,
        ];
        if actual.iter().zip(claimed).any(|(a, b)| *a > b)
            || (self.evidence.outcomes_complete && counts != self.counts)
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        bounded_json(self).map(|_| ())
    }
}

impl OperationCompletion {
    pub fn validate(&self) -> Result<(), Fault> {
        if self.schema_version != 1 || !valid_token(&self.id) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        self.report.validate()?;
        bounded_json(self).map(|_| ())
    }
}

pub(crate) fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>, Fault> {
    let bytes = serde_json::to_vec(value).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
    if bytes.len() >= MAX_FRAME {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    Ok(bytes)
}
