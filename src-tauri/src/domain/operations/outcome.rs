use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    AwaitingUserDecision,
    Running,
    CancelRequested,
    Succeeded,
    PartiallySucceeded,
    Cancelled,
    Failed,
    InterruptedUnknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeCounts {
    pub changed: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unknown: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinishEvidence {
    pub perform_succeeded: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pub aborted: Option<bool>,
    pub outcomes_complete: bool,
    pub cancellation_confirmed: bool,
}

pub fn classify_outcome(counts: OutcomeCounts, evidence: FinishEvidence) -> OperationStatus {
    if !evidence.outcomes_complete
        || counts.unknown > 0
        || evidence.aborted.is_none()
        || (evidence.cancellation_confirmed && evidence.aborted != Some(true))
    {
        return OperationStatus::InterruptedUnknown;
    }
    if counts.changed > 0 {
        return if counts.skipped == 0
            && counts.failed == 0
            && evidence.perform_succeeded
            && evidence.aborted == Some(false)
        {
            OperationStatus::Succeeded
        } else {
            OperationStatus::PartiallySucceeded
        };
    }
    if evidence.cancellation_confirmed {
        return OperationStatus::Cancelled;
    }
    if counts.skipped > 0
        || counts.failed > 0
        || !evidence.perform_succeeded
        || evidence.aborted == Some(true)
    {
        return OperationStatus::Failed;
    }
    OperationStatus::InterruptedUnknown
}

impl OperationStatus {
    pub const fn after_restart(self) -> Self {
        match self {
            Self::Queued | Self::AwaitingUserDecision | Self::Running | Self::CancelRequested => {
                Self::InterruptedUnknown
            }
            Self::Succeeded
            | Self::PartiallySucceeded
            | Self::Cancelled
            | Self::Failed
            | Self::InterruptedUnknown => self,
        }
    }
}
