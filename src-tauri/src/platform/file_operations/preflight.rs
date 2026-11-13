use super::identity::{capture_source, recheck};
use crate::domain::{
    EntryKind, Fault, FaultCode,
    operations::{OperationPlan, OperationSource, PreparedAction},
};
use std::{fs, io, path::Path};

pub fn check_plan(plan: &OperationPlan) -> Result<(), Fault> {
    plan.validate()?;
    match &plan.action {
        PreparedAction::MoveTree {
            roots,
            descendants,
            destination,
        } => super::move_tree::check(roots, descendants, destination)?,
        PreparedAction::Archive {
            roots,
            descendants,
            destination,
            name,
            extract,
        } => super::archive::check(roots, descendants, destination, name, *extract)?,
        PreparedAction::NewFolder { parent, name } => {
            recheck(parent)?;
            absent(&Path::new(&parent.path).join(name))?;
        }
        PreparedAction::Rename { source, name } => {
            mutable_source(source)?;
            let path = Path::new(&source.path);
            let parent = path
                .parent()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
            let target = parent.join(name);
            if target == path {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
            match absent(&target) {
                Ok(()) => {}
                Err(error) if error.code == FaultCode::DestinationExists => {
                    let old_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
                    let target = capture_source(
                        target
                            .to_str()
                            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
                    )?;
                    if !old_name.eq_ignore_ascii_case(name)
                        || (source.volume, source.file_index) != (target.volume, target.file_index)
                    {
                        return Err(error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        PreparedAction::CopySkipExisting {
            sources,
            destination,
            skipped,
        } => {
            if !super::identity::fixed_local(&destination.path)
                || sources
                    .iter()
                    .any(|s| !super::identity::fixed_local(&s.path))
            {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            super::skip_existing::check(sources, destination, skipped)?;
        }
        PreparedAction::CopyAs {
            source,
            destination,
            name,
        } => {
            if !super::identity::fixed_local(&source.path)
                || !super::identity::fixed_local(&destination.path)
            {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            mutable_source(source)?;
            recheck(destination)?;
            absent(&Path::new(&destination.path).join(name))?;
        }
        PreparedAction::Copy {
            sources,
            destination,
        }
        | PreparedAction::Move {
            sources,
            destination,
        } => {
            recheck(destination)?;
            if matches!(plan.action, PreparedAction::Move { .. })
                && super::wsl_move::compatible(sources, destination)
            {
                return super::wsl_move::check(sources, destination);
            }
            if matches!(plan.action, PreparedAction::Move { .. })
                && (!super::identity::fixed_local(&destination.path)
                    || sources
                        .iter()
                        .any(|s| !super::identity::fixed_local(&s.path)))
            {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
            let folders: Vec<_> = sources
                .iter()
                .filter(|source| source.kind == EntryKind::Folder)
                .collect();
            if !folders.is_empty() {
                if !matches!(plan.action, PreparedAction::Move { .. })
                    || folders.iter().any(|source| {
                        source.volume != destination.volume
                            || !source
                                .path
                                .get(..2)
                                .zip(destination.path.get(..2))
                                .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
                    })
                {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                let roots = std::iter::once(Path::new(&destination.path)).chain(
                    sources
                        .iter()
                        .filter_map(|source| Path::new(&source.path).parent()),
                );
                let mut checked = std::collections::HashSet::new();
                for root in roots {
                    for ancestor in root.ancestors() {
                        if !checked.insert(ancestor) {
                            continue;
                        }
                        let ancestor = capture_source(
                            ancestor
                                .to_str()
                                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
                        )?;
                        if folders.iter().any(|source| {
                            (source.volume, source.file_index)
                                == (ancestor.volume, ancestor.file_index)
                        }) {
                            return Err(Fault::new(FaultCode::InvalidRequest));
                        }
                    }
                }
            }
            for source in sources {
                mutable_source(source)?;
                let name = Path::new(&source.path)
                    .file_name()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
                absent(&Path::new(&destination.path).join(name))?;
            }
        }
        PreparedAction::Recycle { sources } => super::recycle::check(sources)?,
        PreparedAction::RecycleBatch { roots, descendants } => {
            super::folder_recycle::check_batch(roots, descendants)?;
        }
        PreparedAction::RecycleTree { root, descendants } => {
            super::FolderRecycleSnapshot::from_prepared(root, descendants)?;
        }
        PreparedAction::CopyTree {
            roots,
            descendants,
            destination,
        } => {
            super::copy_tree::check(roots, descendants, destination)?;
        }
    }
    Ok(())
}

fn mutable_source(source: &OperationSource) -> Result<(), Fault> {
    recheck(source)?;
    if Path::new(&source.path).file_name().is_none() {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    Ok(())
}

pub(super) fn absent(path: &Path) -> Result<(), Fault> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Fault::from(error)),
        Ok(_) => Err(Fault::new(FaultCode::DestinationExists)),
    }
}
