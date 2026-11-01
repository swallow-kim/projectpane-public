use super::{
    capture_source, copy_tree, identity::recheck, preflight::absent, sink_state::shell_path,
};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{cell::RefCell, collections::HashMap, path::Path};
use windows::{
    Win32::{Foundation::E_ABORT, UI::Shell::IShellItem},
    core::{PCWSTR, Ref},
};

#[derive(Default)]
pub(super) struct TreeCopyProgress {
    created: RefCell<HashMap<String, OperationSource>>,
}

impl TreeCopyProgress {
    pub fn before(
        &self,
        plan: &OperationPlan,
        source: &Ref<'_, IShellItem>,
        parent: &Ref<'_, IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        let source = shell_path(source)?;
        let parent = shell_path(parent)?;
        // SAFETY: COM category 8: the synchronous callback owns a NUL-terminated
        // optional UTF-16 name. Convert it here; no pointer escapes the callback.
        let name = if name.is_null() {
            None
        } else {
            Some(unsafe { name.to_string()? })
        };
        self.check_before(plan, &source, &parent, name.as_deref())
            .map_err(|_| E_ABORT.into())
    }

    pub(super) fn check_before(
        &self,
        plan: &OperationPlan,
        actual: &str,
        parent: &str,
        name: Option<&str>,
    ) -> Result<(), Fault> {
        let PreparedAction::CopyTree {
            roots,
            descendants,
            destination,
        } = &plan.action
        else {
            return Err(Fault::new(FaultCode::InvalidRequest));
        };
        let actual_source = capture_source(actual)?;
        let source = roots
            .iter()
            .chain(descendants)
            .find(|source| **source == actual_source)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        recheck(destination)?;
        for path in Path::new(&source.path).ancestors() {
            if let Some(ancestor) = roots
                .iter()
                .chain(descendants)
                .find(|entry| Path::new(&entry.path) == path)
            {
                recheck(ancestor)?;
            }
        }
        if source.kind == EntryKind::Folder {
            copy_tree::check_children(source, descendants)?;
        }
        let expected = copy_tree::target(source, roots, destination)?;
        let actual_parent = capture_source(parent)?;
        if expected.parent() != Some(Path::new(&actual_parent.path)) {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        if actual_parent != *destination {
            let created = self
                .created
                .try_borrow()
                .map_err(|_| Fault::new(FaultCode::InvalidState))?;
            if created.get(&actual_parent.path) != Some(&actual_parent) {
                return Err(Fault::new(FaultCode::StaleRequest));
            }
        }
        if let Some(name) = name.filter(|value| !value.is_empty()) {
            validate_leaf_name(name)?;
            if expected.file_name().and_then(|name| name.to_str()) != Some(name) {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
        }
        absent(&expected)
    }

    pub fn after(
        &self,
        plan: &OperationPlan,
        source: &Ref<'_, IShellItem>,
        target: &Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        let source = shell_path(source)?;
        let path = shell_path(target)?;
        self.check_after(plan, &source, &path)
            .map_err(|_| E_ABORT.into())
    }

    pub(super) fn check_after(
        &self,
        plan: &OperationPlan,
        source: &str,
        path: &str,
    ) -> Result<(), Fault> {
        let PreparedAction::CopyTree {
            roots,
            descendants,
            destination,
        } = &plan.action
        else {
            return Err(Fault::new(FaultCode::InvalidRequest));
        };
        let actual_source = capture_source(source)?;
        let source = roots
            .iter()
            .chain(descendants)
            .find(|item| **item == actual_source)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        let actual = capture_source(path)?;
        if Path::new(&actual.path) != copy_tree::target(source, roots, destination)?
            || actual.kind != source.kind
            || actual.volume != destination.volume
        {
            return Err(Fault::new(FaultCode::StaleRequest));
        }
        if actual.kind == EntryKind::Folder {
            let mut created = self
                .created
                .try_borrow_mut()
                .map_err(|_| Fault::new(FaultCode::InvalidState))?;
            if created.insert(actual.path.clone(), actual).is_some() {
                return Err(Fault::new(FaultCode::InvalidState));
            }
        }
        Ok(())
    }
}
