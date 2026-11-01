use super::{capture_source, identity::recheck, preflight::absent};
use crate::domain::{Fault, FaultCode, operations::*};

pub(super) fn prepare(
    sources: Vec<OperationSource>,
    destination: OperationSource,
) -> Result<PreparedAction, Fault> {
    let mut plan = OperationPlan {
        project_id: None,
        action: PreparedAction::CopySkipExisting {
            sources,
            destination,
            skipped: vec![],
        },
    };
    plan.validate()?;
    if let PreparedAction::CopySkipExisting {
        sources,
        destination,
        skipped,
    } = &mut plan.action
    {
        for source in sources {
            let target = copy_target(source, destination)?;
            match absent(std::path::Path::new(&target)) {
                Ok(()) => {}
                Err(error) if error.code == FaultCode::DestinationExists => {
                    regular_file(&target)?;
                    skipped.push(SkipExistingTarget {
                        source: source.path.clone(),
                        target: capture_source(&target)?,
                    });
                }
                Err(error) => return Err(error),
            }
        }
    }
    plan.validate()?;
    Ok(plan.action)
}

pub(super) fn check(
    sources: &[OperationSource],
    destination: &OperationSource,
    skipped: &[SkipExistingTarget],
) -> Result<(), Fault> {
    recheck(destination).map_err(|_| Fault::new(FaultCode::StaleRequest))?;
    for source in sources {
        recheck(source).map_err(|_| Fault::new(FaultCode::StaleRequest))?;
        regular_file(&source.path)?;
        match skipped
            .iter()
            .find(|decision| decision.source == source.path)
        {
            Some(decision) => {
                recheck(&decision.target).map_err(|_| Fault::new(FaultCode::StaleRequest))?;
                regular_file(&decision.target.path)?;
            }
            None => absent(std::path::Path::new(&copy_target(source, destination)?))?,
        }
    }
    Ok(())
}

fn regular_file(path: &str) -> Result<(), Fault> {
    if !std::fs::symlink_metadata(path)
        .map_err(Fault::from)?
        .is_file()
    {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    Ok(())
}

/// Called only after the complete immutable plan has passed dispatch preflight.
pub(super) fn dispatch(plan: OperationPlan) -> (OperationPlan, Vec<UserSkipped>) {
    match plan.action {
        PreparedAction::CopySkipExisting {
            sources,
            destination,
            skipped,
        } => {
            let sources = sources
                .into_iter()
                .filter(|source| {
                    !skipped
                        .iter()
                        .any(|decision| decision.source == source.path)
                })
                .collect();
            let decisions = skipped
                .into_iter()
                .map(|decision| UserSkipped {
                    source: decision.source,
                    destination: decision.target.path,
                })
                .collect();
            (
                OperationPlan {
                    project_id: plan.project_id,
                    action: PreparedAction::Copy {
                        sources,
                        destination,
                    },
                },
                decisions,
            )
        }
        action => (
            OperationPlan {
                project_id: plan.project_id,
                action,
            },
            vec![],
        ),
    }
}

pub(super) fn decision_only(decisions: Vec<UserSkipped>) -> Result<OperationReport, Fault> {
    let report = OperationReport {
        failure: None,
        counts: OutcomeCounts {
            skipped: u32::try_from(decisions.len())
                .map_err(|_| Fault::new(FaultCode::ResourceLimit))?,
            ..OutcomeCounts::default()
        },
        evidence: FinishEvidence {
            perform_succeeded: false,
            aborted: None,
            outcomes_complete: true,
            cancellation_confirmed: false,
        },
        items: vec![],
        user_skipped: decisions,
        user_decisions_only: Some(true),
    };
    report.validate()?;
    Ok(report)
}
