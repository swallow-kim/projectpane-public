use super::{ItemOutcome, ItemStatus, MAX_OPERATION_RESULTS, OperationSource, UserSkipped};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, projects};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkipExistingTarget {
    pub source: String,
    pub target: OperationSource,
}

pub fn copy_target(
    source: &OperationSource,
    destination: &OperationSource,
) -> Result<String, Fault> {
    let name = source
        .path
        .rsplit('\\')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    super::validate_leaf_name(name)?;
    let target = format!("{}\\{name}", destination.path.trim_end_matches('\\'));
    projects::normalize_absolute_path(&target).map_err(|_| Fault::new(FaultCode::InvalidPath))
}

pub(super) fn validate(
    sources: &[OperationSource],
    destination: &OperationSource,
    skipped: &[SkipExistingTarget],
) -> Result<(), Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    if sources.len() > MAX_OPERATION_RESULTS || skipped.len() > sources.len() {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let source_ids: HashSet<_> = sources.iter().map(|s| (s.volume, s.file_index)).collect();
    let mut target_ids = HashSet::new();
    let mut skipped_sources = HashSet::new();
    for decision in skipped {
        decision.target.validate()?;
        let source = sources
            .iter()
            .find(|s| s.path == decision.source)
            .ok_or_else(invalid)?;
        if decision.target.kind != EntryKind::File
            || !copy_target(source, destination)?.eq_ignore_ascii_case(&decision.target.path)
            || !skipped_sources.insert(&decision.source)
            || source_ids.contains(&(decision.target.volume, decision.target.file_index))
            || !target_ids.insert((decision.target.volume, decision.target.file_index))
        {
            return Err(invalid());
        }
    }
    let mut paths = HashSet::new();
    let mut targets = HashSet::new();
    // Match the sink envelope plus the decision array and optional-field reserve.
    let mut bytes = 4096 + 34;
    for source in sources {
        let target = copy_target(source, destination)?;
        if source.kind != EntryKind::File
            || !paths.insert(source.path.to_ascii_lowercase())
            || !targets.insert(target.to_ascii_lowercase())
            || source.path.eq_ignore_ascii_case(&target)
        {
            return Err(invalid());
        }
        let size = match skipped.iter().find(|d| d.source == source.path) {
            Some(decision) => serde_json::to_vec(&UserSkipped {
                source: source.path.clone(),
                destination: decision.target.path.clone(),
            }),
            None => serde_json::to_vec(&ItemOutcome {
                source: source.path.clone(),
                destination: Some(target),
                status: ItemStatus::Skipped,
                hresult: i32::MIN,
            }),
        }
        .map_err(|_| invalid())?
        .len()
            + 1;
        bytes += size;
        if bytes >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
    }
    Ok(())
}
