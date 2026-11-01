use super::{OperationReport, OutcomeCounts};
use crate::domain::{Fault, FaultCode};
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserSkipped {
    pub source: String,
    pub destination: String,
}

pub(super) fn deserialize_decisions<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<UserSkipped>, D::Error> {
    let decisions = Vec::<UserSkipped>::deserialize(deserializer)?;
    if decisions.is_empty() {
        return Err(D::Error::custom("userSkipped must be omitted when empty"));
    }
    Ok(decisions)
}

pub(super) fn deserialize_only<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    match bool::deserialize(deserializer)? {
        true => Ok(Some(true)),
        false => Err(D::Error::custom(
            "userDecisionsOnly must be true or omitted",
        )),
    }
}

pub(super) fn validate(report: &OperationReport) -> Result<(), Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    let native_sources: HashSet<_> = report
        .items
        .iter()
        .map(|item| item.source.to_ascii_lowercase())
        .collect();
    let mut sources = HashSet::new();
    let mut destinations = HashSet::new();
    for decision in &report.user_skipped {
        for path in [&decision.source, &decision.destination] {
            if path.is_empty() || path.len() > 32768 || path.contains('\0') {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
        }
        if decision.source.eq_ignore_ascii_case(&decision.destination)
            || !sources.insert(decision.source.to_ascii_lowercase())
            || !destinations.insert(decision.destination.to_ascii_lowercase())
            || native_sources.contains(&decision.source.to_ascii_lowercase())
        {
            return Err(invalid());
        }
    }
    if report.user_decisions_only.is_some() {
        let skipped = u32::try_from(report.user_skipped.len()).map_err(|_| invalid())?;
        if report.user_decisions_only != Some(true)
            || skipped == 0
            || !report.items.is_empty()
            || report.failure.is_some()
            || report.counts
                != (OutcomeCounts {
                    skipped,
                    ..OutcomeCounts::default()
                })
            || report.evidence.perform_succeeded
            || report.evidence.aborted.is_some()
            || !report.evidence.outcomes_complete
            || report.evidence.cancellation_confirmed
        {
            return Err(invalid());
        }
    } else if !report.user_skipped.is_empty()
        && (report.failure.is_some()
            || (report.items.is_empty()
                && report.counts.changed == 0
                && report.counts.failed == 0
                && report.counts.unknown == 0))
    {
        return Err(invalid());
    }
    Ok(())
}

impl OperationReport {
    /// Bind confirmed copy outcomes and decision provenance to their immutable intent.
    pub fn validate_for_plan(&self, plan: &super::OperationPlan) -> Result<(), Fault> {
        self.validate()?;
        let invalid = || Fault::new(FaultCode::InvalidRequest);
        let mut confirmed_sources = HashSet::new();
        for item in &self.items {
            if item.status != super::ItemStatus::Changed {
                continue;
            }
            let expected = match &plan.action {
                super::PreparedAction::CopyAs {
                    source,
                    destination,
                    name,
                } => {
                    if item.source != source.path {
                        return Err(invalid());
                    }
                    format!("{}\\{name}", destination.path.trim_end_matches('\\'))
                }
                super::PreparedAction::CopySkipExisting {
                    sources,
                    destination,
                    skipped,
                } => {
                    let source = sources
                        .iter()
                        .find(|source| source.path == item.source)
                        .ok_or_else(invalid)?;
                    if skipped
                        .iter()
                        .any(|decision| decision.source == item.source)
                    {
                        return Err(invalid());
                    }
                    super::copy_target(source, destination)?
                }
                _ => continue,
            };
            if item.destination.as_deref() != Some(expected.as_str())
                || !confirmed_sources.insert(&item.source)
            {
                return Err(invalid());
            }
        }
        match &plan.action {
            super::PreparedAction::CopySkipExisting {
                sources, skipped, ..
            } => {
                if self.user_skipped.is_empty() {
                    if skipped.is_empty()
                        || (self.items.is_empty()
                            && self.counts == OutcomeCounts::default()
                            && !self.evidence.perform_succeeded)
                    {
                        return Ok(());
                    }
                    return Err(invalid());
                }
                if self.user_skipped.len() != skipped.len()
                    || self.user_skipped.iter().any(|decision| {
                        !skipped.iter().any(|approved| {
                            approved.source == decision.source
                                && approved.target.path == decision.destination
                        })
                    })
                    || (self.user_decisions_only == Some(true)) != (sources.len() == skipped.len())
                    || self
                        .items
                        .iter()
                        .any(|item| !sources.iter().any(|source| source.path == item.source))
                {
                    return Err(invalid());
                }
            }
            _ => {
                if !self.user_skipped.is_empty() || self.user_decisions_only.is_some() {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
