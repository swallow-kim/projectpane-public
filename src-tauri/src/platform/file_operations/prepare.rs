use super::{capture_source, check_plan};
use crate::domain::{ActivationKind, ActivationTarget, EntryKind, Fault, FaultCode, operations::*};

pub(super) fn prepare(
    request: PrepareOperation,
    targets: Vec<ActivationTarget>,
) -> Result<OperationPlan, Fault> {
    request.validate()?;
    if targets.len()
        != request
            .selection()
            .map_or(0, |selection| selection.entry_ids.len())
    {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let mut sources = capture(targets)?;
    let action = match request.action {
        OperationAction::Archive {
            destination,
            name,
            extract,
            ..
        } => super::archive::prepare(sources, capture_source(&destination)?, name, extract)?,
        OperationAction::NewFolder { parent, name } => PreparedAction::NewFolder {
            parent: capture_source(&parent)?,
            name,
        },
        OperationAction::Rename { name, .. } => PreparedAction::Rename {
            source: sources
                .pop()
                .ok_or_else(|| Fault::new(FaultCode::InvalidRequest))?,
            name,
        },
        OperationAction::Copy { destination, .. } => {
            transfer_action(TransferMode::Copy, sources, &destination)?
        }
        OperationAction::CopySkipExisting { destination, .. } => {
            super::skip_existing::prepare(sources, capture_source(&destination)?)?
        }
        OperationAction::CopyAs {
            destination, name, ..
        } => PreparedAction::CopyAs {
            source: sources
                .pop()
                .ok_or_else(|| Fault::new(FaultCode::InvalidRequest))?,
            destination: capture_source(&destination)?,
            name,
        },
        OperationAction::Move { destination, .. } => {
            transfer_action(TransferMode::Move, sources, &destination)?
        }
        OperationAction::Recycle { .. } => super::folder_recycle::prepare(sources)?,
    };
    let plan = OperationPlan {
        project_id: request.project_id,
        action,
    };
    check_plan(&plan)?;
    Ok(plan)
}

pub(super) fn capture(targets: Vec<ActivationTarget>) -> Result<Vec<OperationSource>, Fault> {
    if targets.len() > MAX_OPERATION_SOURCES {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let mut sources = Vec::with_capacity(targets.len());
    let mut bytes = 4096;
    for target in targets {
        let source = capture_source(&target.path)?;
        let kind = match target.kind {
            ActivationKind::File => EntryKind::File,
            ActivationKind::Folder => EntryKind::Folder,
        };
        if source.kind != kind {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        bytes += bounded_json(&source)?.len() + 1;
        if bytes >= crate::domain::MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        sources.push(source);
    }
    Ok(sources)
}

fn transfer_action(
    mode: TransferMode,
    sources: Vec<OperationSource>,
    destination: &str,
) -> Result<PreparedAction, Fault> {
    let destination = capture_source(destination)?;
    Ok(match mode {
        TransferMode::Copy
            if sources
                .iter()
                .any(|source| source.kind == EntryKind::Folder) =>
        {
            super::copy_tree::prepare(sources, destination)?
        }
        TransferMode::Copy => PreparedAction::Copy {
            sources,
            destination,
        },
        TransferMode::Move
            if sources
                .iter()
                .any(|s| s.kind == EntryKind::Folder && s.volume != destination.volume) =>
        {
            super::move_tree::prepare(sources, destination)?
        }
        TransferMode::Move => PreparedAction::Move {
            sources,
            destination,
        },
    })
}

pub(super) fn from_transfer(
    request: PrepareTransfer,
    mode: TransferMode,
    sources: Vec<OperationSource>,
) -> Result<OperationPlan, Fault> {
    request.validate()?;
    for source in &sources {
        super::identity::recheck(source)?;
    }
    if (request.copy_name.is_some() || request.skip_existing) && mode != TransferMode::Copy {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let action = if request.skip_existing {
        super::skip_existing::prepare(sources, capture_source(&request.destination)?)?
    } else if let Some(name) = request.copy_name {
        if sources.len() != 1 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        PreparedAction::CopyAs {
            source: sources
                .into_iter()
                .next()
                .ok_or_else(|| Fault::new(FaultCode::InvalidRequest))?,
            destination: capture_source(&request.destination)?,
            name,
        }
    } else {
        transfer_action(mode, sources, &request.destination)?
    };
    let plan = OperationPlan {
        project_id: request.project_id,
        action,
    };
    check_plan(&plan)?;
    Ok(plan)
}
