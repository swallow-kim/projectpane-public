use super::{check_plan, sink_state::SinkContext};
use crate::domain::operations::{OperationPlan, PreparedAction};
use windows::{
    Win32::{
        Foundation::{E_ABORT, E_FAIL},
        UI::Shell::{IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem},
    },
    core::{HRESULT, PCWSTR, Ref, implement},
};

#[implement(IFileOperationProgressSink)]
pub(super) struct ProgressSink {
    pub context: SinkContext,
    pub plan: OperationPlan,
    pub tree: super::copy_tree_sink::TreeCopyProgress,
    pub recycle: super::recycle::RecycleProgress,
    pub moving_tree: super::move_tree_sink::TreeMoveProgress,
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for ProgressSink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        self.context.check_cancel()
    }
    fn FinishOperations(&self, result: HRESULT) -> windows::core::Result<()> {
        self.context
            .state
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .finished = Some(result);
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.before(source)
    }
    fn PostRenameItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        _: &PCWSTR,
        result: HRESULT,
        destination: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        let confirmed = self.relocation_confirmed(&source, &destination);
        self.context.record(source, destination, result, confirmed)
    }
    fn PreMoveItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        parent: Ref<'_, IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        if matches!(self.plan.action, PreparedAction::MoveTree { .. }) {
            self.context.check_cancel()?;
            return self.moving_tree.before(&self.plan, &source, &parent, name);
        }
        self.before(source)
    }
    fn PostMoveItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        result: HRESULT,
        destination: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        if matches!(self.plan.action, PreparedAction::MoveTree { .. }) {
            return self
                .moving_tree
                .after(&self.plan, &self.context, source, destination, result);
        }
        let confirmed = self.relocation_confirmed(&source, &destination);
        self.context.record(source, destination, result, confirmed)
    }
    fn PreCopyItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        parent: Ref<'_, IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        if matches!(self.plan.action, PreparedAction::CopyTree { .. }) {
            self.context.check_cancel()?;
            return self.tree.before(&self.plan, &source, &parent, name);
        }
        if matches!(self.plan.action, PreparedAction::CopyAs { .. }) {
            return self.named_copy_before(source, (parent, name));
        }
        self.before(source)
    }
    fn PostCopyItem(
        &self,
        _: u32,
        source: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        result: HRESULT,
        destination: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        if matches!(self.plan.action, PreparedAction::CopyAs { .. }) {
            return self.named_copy_after((source, destination), result);
        }
        let tree_result =
            if result.0 == 0 && matches!(self.plan.action, PreparedAction::CopyTree { .. }) {
                self.tree.after(&self.plan, &source, &destination)
            } else if result.0 > 0 && matches!(self.plan.action, PreparedAction::CopyTree { .. }) {
                Err(E_ABORT.into())
            } else {
                Ok(())
            };
        if tree_result.is_err() {
            self.context
                .state
                .try_borrow_mut()
                .map_err(|_| windows::core::Error::from(E_FAIL))?
                .report
                .evidence
                .outcomes_complete = false;
        }
        self.context.record(source, destination, result, false)?;
        tree_result
    }
    fn PreDeleteItem(&self, flags: u32, source: Ref<'_, IShellItem>) -> windows::core::Result<()> {
        self.context.check_cancel()?;
        self.recycle.before(&self.plan, flags, &source)
    }
    fn PostDeleteItem(
        &self,
        flags: u32,
        source: Ref<'_, IShellItem>,
        result: HRESULT,
        destination: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.recycle
            .after(&self.context, flags, &source, result, &destination)
    }
    fn PreNewItem(&self, _: u32, _: Ref<'_, IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        self.context.check_cancel()?;
        check_plan(&self.plan).map_err(|_| windows::core::Error::from(E_ABORT))
    }
    fn PostNewItem(
        &self,
        _: u32,
        parent: Ref<'_, IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        result: HRESULT,
        destination: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.context.record(parent, destination, result, false)
    }
    fn UpdateProgress(&self, total: u32, completed: u32) -> windows::core::Result<()> {
        self.context.progress.update(total, completed);
        self.context.check_cancel()
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}
