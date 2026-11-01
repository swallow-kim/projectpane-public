//! Read-only interpretation of staged replacement evidence; never mutation authority.
mod model;
mod record;
#[cfg(test)]
mod tests;
mod validate;

use crate::domain::{Fault, FaultCode, MAX_FRAME};
pub use model::Phase as RecoveryPhase;
#[cfg(windows)]
pub(crate) use model::{Content as ObservedContent, Observation as FileObservation};
use model::{Inspection, Observation, Phase};
pub use record::RecoveryRecord;
use serde::Serialize;

/// Observed layout, not a successful operation or permission to discard a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAssessment {
    Unknown,
    OriginalAtTarget,
    OriginalRetained,
    DestinationOccupiedOriginalRetained,
    InstalledWithOriginalRetained,
}

/// Interprets bounded JSON containing a record and independently collected observations.
/// Does not open paths, verify observation provenance, recover, replay or delete files.
///
/// # Errors
/// Rejects malformed/version-mismatched records, invalid identities/paths and frame overflow.
pub fn inspect_replacement(bytes: &[u8]) -> Result<RecoveryAssessment, Fault> {
    if bytes.len() >= MAX_FRAME {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    let input: Inspection =
        serde_json::from_slice(bytes).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
    validate::inspection(&input)?;
    Ok(classify(&input))
}

fn classify(input: &Inspection) -> RecoveryAssessment {
    use RecoveryAssessment::{
        DestinationOccupiedOriginalRetained, InstalledWithOriginalRetained, OriginalAtTarget,
        OriginalRetained, Unknown,
    };
    let record = &input.record;
    let seen = &input.observations;
    let mut identities = std::collections::HashSet::new();
    for observed in [
        &seen.parent,
        &seen.artifacts,
        &seen.target,
        &seen.staged,
        &seen.backup,
    ] {
        match observed {
            Observation::Absent | Observation::Inaccessible => {}
            Observation::Present {
                volume, file_index, ..
            } => {
                if !identities.insert((*volume, *file_index)) {
                    return Unknown;
                }
            }
        }
    }
    if !seen.parent.matches(&record.parent, None)
        || !seen.artifacts.matches(&record.artifacts, None)
    {
        return Unknown;
    }
    let original_at_target = seen
        .target
        .matches(&record.original.object, Some(&record.original.content));
    let original_at_backup = seen
        .backup
        .matches(&record.original.object, Some(&record.original.content));
    let staged_at_target = seen
        .target
        .matches(&record.staged.object, Some(&record.staged.content));
    let staged_at_stage = seen
        .staged
        .matches(&record.staged.object, Some(&record.staged.content));
    match record.phase {
        Phase::Ready => {
            if original_at_target && staged_at_stage && seen.backup == Observation::Absent {
                OriginalAtTarget
            } else {
                Unknown
            }
        }
        Phase::PreserveRequested | Phase::OriginalPreserved | Phase::InstallRequested => {
            if record.phase == Phase::PreserveRequested
                && original_at_target
                && staged_at_stage
                && seen.backup == Observation::Absent
            {
                return OriginalAtTarget;
            }
            if !original_at_backup {
                return Unknown;
            }
            if record.phase == Phase::InstallRequested
                && staged_at_target
                && seen.staged == Observation::Absent
            {
                return InstalledWithOriginalRetained;
            }
            if !staged_at_stage {
                return Unknown;
            }
            match &seen.target {
                Observation::Absent => OriginalRetained,
                Observation::Inaccessible => Unknown,
                Observation::Present { .. } => {
                    if original_at_target || staged_at_target {
                        Unknown
                    } else {
                        DestinationOccupiedOriginalRetained
                    }
                }
            }
        }
        Phase::InstallObserved => {
            if original_at_backup && staged_at_target && seen.staged == Observation::Absent {
                InstalledWithOriginalRetained
            } else {
                Unknown
            }
        }
    }
}
