use super::{
    capture_source, copy_tree,
    identity::recheck,
    move_tree,
    preflight::absent,
    sink_state::{SinkContext, shell_path},
};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::Path,
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::E_ABORT,
        UI::Shell::{COPYENGINE_S_DONT_PROCESS_CHILDREN, COPYENGINE_S_NOT_HANDLED, IShellItem},
    },
    core::{HRESULT, PCWSTR, Ref},
};

#[derive(Default)]
struct State {
    created: HashMap<String, OperationSource>,
    pending: HashMap<String, OperationSource>,
    moved: HashSet<String>,
}
#[derive(Clone, Default)]
pub(super) struct TreeMoveProgress(Rc<RefCell<State>>);
#[cfg(test)]
type AfterMoveHook = Option<Box<dyn Fn(&str)>>;
#[cfg(test)]
thread_local! { pub(super) static AFTER_MOVE: RefCell<AfterMoveHook> = RefCell::new(None); }

fn parts(
    plan: &OperationPlan,
) -> Result<(&[OperationSource], &[OperationSource], &OperationSource), Fault> {
    if let PreparedAction::MoveTree {
        roots,
        descendants,
        destination,
    } = &plan.action
    {
        Ok((roots, descendants, destination))
    } else {
        Err(Fault::new(FaultCode::InvalidRequest))
    }
}
impl TreeMoveProgress {
    pub fn before(
        &self,
        plan: &OperationPlan,
        source: &Ref<'_, IShellItem>,
        parent: &Ref<'_, IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        let source = shell_path(source)?;
        let parent = shell_path(parent)?;
        // SAFETY: optional NUL-terminated name remains live for this callback.
        let name = if name.is_null() {
            None
        } else {
            Some(unsafe { name.to_string()? })
        };
        self.check_before(plan, &source, &parent, name.as_deref())
            .map_err(|error| {
                #[cfg(test)]
                eprintln!("move pre rejected {source} -> {parent}: {error:?}");
                let _ = error;
                E_ABORT.into()
            })
    }
    pub(super) fn check_before(
        &self,
        plan: &OperationPlan,
        actual: &str,
        parent: &str,
        name: Option<&str>,
    ) -> Result<(), Fault> {
        let (roots, descendants, destination) = parts(plan)?;
        let source = roots
            .iter()
            .chain(descendants)
            .find(|s| s.path.eq_ignore_ascii_case(actual))
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        move_tree::compatible(source)?;
        recheck(destination)?;
        let state = self.0.borrow();
        if state.moved.contains(&source.path) || state.pending.contains_key(&source.path) {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        // Every remaining ancestor must still be the planned directory. Check
        // directory streams again at each boundary, not just during preparation.
        for ancestor in roots
            .iter()
            .chain(descendants)
            .filter(|s| s.kind == EntryKind::Folder && Path::new(actual).starts_with(&s.path))
        {
            move_tree::compatible(ancestor)?;
        }
        if source.kind == EntryKind::Folder {
            let remaining: Vec<_> = descendants
                .iter()
                .filter(|s| !state.moved.contains(&s.path))
                .cloned()
                .collect();
            copy_tree::check_children(source, &remaining)?;
        }
        let expected = copy_tree::target(source, roots, destination)?;
        let actual_parent = capture_source(parent)?;
        if expected.parent() != Some(Path::new(&actual_parent.path))
            || (actual_parent != *destination
                && state.created.get(&actual_parent.path) != Some(&actual_parent))
        {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        if let Some(name) = name.filter(|s| !s.is_empty()) {
            validate_leaf_name(name)?;
            if expected.file_name().and_then(|s| s.to_str()) != Some(name) {
                return Err(Fault::new(FaultCode::StaleRequest));
            }
        }
        absent(&expected)
    }
    pub fn after(
        &self,
        plan: &OperationPlan,
        context: &SinkContext,
        source: Ref<'_, IShellItem>,
        destination: Ref<'_, IShellItem>,
        result: HRESULT,
    ) -> windows::core::Result<()> {
        let path = shell_path(&source)?;
        let (roots, descendants, target_root) = parts(plan).map_err(|_| E_ABORT)?;
        let expected_source = roots
            .iter()
            .chain(descendants)
            .find(|s| s.path.eq_ignore_ascii_case(&path))
            .ok_or(E_ABORT)?;
        let target = shell_path(&destination)
            .ok()
            .and_then(|p| capture_source(&p).ok());
        let valid_target = target.as_ref().is_some_and(|target| {
            target.kind == expected_source.kind
                && target.volume == target_root.volume
                && copy_tree::target(expected_source, roots, target_root)
                    .ok()
                    .as_deref()
                    == Some(Path::new(&target.path))
        });
        if result == COPYENGINE_S_NOT_HANDLED
            && expected_source.kind == EntryKind::Folder
            && valid_target
        {
            // Shell emits NOT_HANDLED when it creates a folder, BEFORE moving children.
            // No successful folder outcome is recorded until PerformOperations ends.
            let target = target.unwrap();
            let mut state = self.0.borrow_mut();
            if state.pending.insert(path, target.clone()).is_some() {
                return Err(E_ABORT.into());
            }
            state.created.insert(target.path.clone(), target);
            drop(state);
            return context.check_cancel();
        }
        let absent_source = std::fs::symlink_metadata(&expected_source.path)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        let confirmed = valid_target
            && absent_source
            && (result.0 == 0 || result == COPYENGINE_S_DONT_PROCESS_CHILDREN);
        if confirmed {
            self.0
                .borrow_mut()
                .moved
                .insert(expected_source.path.clone());
        }
        #[cfg(test)]
        if confirmed {
            AFTER_MOVE.with(|hook| {
                if let Some(hook) = hook.borrow().as_ref() {
                    hook(&expected_source.path);
                }
            });
        }
        if result.is_ok() && !confirmed {
            context.state.borrow_mut().report.evidence.outcomes_complete = false;
        }
        // A nominal S_OK without observed relocation must remain unknown too.
        let effective = if result.0 == 0 && !confirmed {
            HRESULT(1)
        } else {
            result
        };
        context.record(source, destination, effective, confirmed)?;
        if result.is_ok() && !confirmed {
            return Err(E_ABORT.into());
        }
        // Reject newly observed directory data before returning control to the
        // Shell's remaining recursion/cleanup. No application-side deletion.
        for folder in roots
            .iter()
            .chain(descendants)
            .filter(|s| s.kind == EntryKind::Folder && !self.0.borrow().moved.contains(&s.path))
        {
            if recheck(folder)
                .and_then(|_| move_tree::no_directory_streams(&folder.path))
                .is_err()
            {
                context.state.borrow_mut().report.evidence.outcomes_complete = false;
                return Err(E_ABORT.into());
            }
        }
        Ok(())
    }
    pub fn finish(&self, context: &SinkContext) {
        let pending = self.0.borrow();
        for (source, target) in &pending.pending {
            let confirmed = std::fs::symlink_metadata(source)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                && capture_source(&target.path).as_ref() == Ok(target);
            let result = context.state.borrow_mut().push(ItemOutcome {
                source: source.clone(),
                destination: Some(target.path.clone()),
                status: if confirmed {
                    ItemStatus::Changed
                } else {
                    ItemStatus::Unknown
                },
                hresult: COPYENGINE_S_NOT_HANDLED.0,
            });
            if result.is_err() {
                context.state.borrow_mut().report.evidence.outcomes_complete = false;
                break;
            }
        }
    }
}
