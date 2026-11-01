use super::{
    capture_source, check_plan, identity::recheck, sink::ProgressSink_Impl, sink_state::shell_path,
};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{E_ABORT, E_FAIL},
        UI::Shell::IShellItem,
    },
    core::{HRESULT, PCWSTR, Ref},
};

pub(super) fn check_before(
    plan: &OperationPlan,
    actual: &str,
    target: (&str, Option<&str>),
) -> Result<(), Fault> {
    let PreparedAction::CopyAs {
        source,
        destination,
        name,
    } = &plan.action
    else {
        return Err(Fault::new(FaultCode::InvalidRequest));
    };
    check_plan(plan)?;
    if capture_source(actual)? != *source
        || capture_source(target.0)? != *destination
        || target.1 != Some(name.as_str())
    {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    Ok(())
}

pub(super) fn check_after(plan: &OperationPlan, actual: &str, target: &str) -> Result<(), Fault> {
    let PreparedAction::CopyAs {
        source,
        destination,
        name,
    } = &plan.action
    else {
        return Err(Fault::new(FaultCode::InvalidRequest));
    };
    recheck(destination)?;
    let copied = capture_source(target)?;
    let expected = Path::new(&destination.path).join(name);
    if capture_source(actual)? != *source
        || Path::new(target) != expected
        || Path::new(&copied.path) != expected
        || copied.kind != EntryKind::File
        || copied.volume != destination.volume
        || (copied.volume, copied.file_index) == (source.volume, source.file_index)
    {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    Ok(())
}

impl ProgressSink_Impl {
    pub(super) fn named_copy_before(
        &self,
        source: Ref<'_, IShellItem>,
        target: (Ref<'_, IShellItem>, &PCWSTR),
    ) -> windows::core::Result<()> {
        self.context.check_cancel()?;
        let source = shell_path(&source)?;
        let parent = shell_path(&target.0)?;
        if target.1.is_null() {
            return Err(E_ABORT.into());
        }
        // SAFETY: FFI category 8: Shell owns this non-null, NUL-terminated UTF-16
        // callback argument until return; conversion copies it without retaining a pointer.
        let name = unsafe { target.1.to_string()? };
        check_before(&self.plan, &source, (&parent, Some(&name))).map_err(|_| E_ABORT.into())
    }

    pub(super) fn named_copy_after(
        &self,
        items: (Ref<'_, IShellItem>, Ref<'_, IShellItem>),
        result: HRESULT,
    ) -> windows::core::Result<()> {
        let source = shell_path(&items.0)?;
        let destination = shell_path(&items.1).ok();
        let mut status = super::result::item_status(result, destination.is_some(), false);
        let unconfirmed = status == ItemStatus::Changed
            && destination
                .as_deref()
                .is_none_or(|target| check_after(&self.plan, &source, target).is_err());
        if unconfirmed {
            status = ItemStatus::Unknown;
        }
        let mut state = self
            .context
            .state
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        if unconfirmed {
            state.report.evidence.outcomes_complete = false;
        }
        state.push(ItemOutcome {
            source,
            destination,
            status,
            hresult: result.0,
        })?;
        drop(state);
        if unconfirmed {
            return Err(E_ABORT.into());
        }
        self.context.check_cancel()
    }
}
