use crate::domain::operations::*;
use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{E_ABORT, E_FAIL},
        System::Com::CoTaskMemFree,
        UI::Shell::{IShellItem, SIGDN_FILESYSPATH},
    },
    core::{HRESULT, Ref},
};

pub(super) struct SinkState {
    pub report: OperationReport,
    pub finished: Option<HRESULT>,
    pub expected: usize,
    pub seen: HashSet<String>,
    pub bytes: usize,
    pub reserved_rows: usize,
}

pub(super) struct SinkContext {
    pub state: Rc<RefCell<SinkState>>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<super::NativeProgress>,
}

impl SinkContext {
    pub fn new(
        expected: usize,
        cancel: Arc<AtomicBool>,
        progress: Arc<super::NativeProgress>,
    ) -> Self {
        Self {
            cancel,
            progress,
            state: Rc::new(RefCell::new(SinkState {
                report: OperationReport {
                    failure: None,
                    user_skipped: vec![],
                    user_decisions_only: None,
                    counts: OutcomeCounts::default(),
                    items: vec![],
                    evidence: FinishEvidence {
                        perform_succeeded: false,
                        aborted: None,
                        outcomes_complete: true,
                        cancellation_confirmed: false,
                    },
                },
                finished: None,
                expected,
                seen: HashSet::new(),
                bytes: 4096,
                reserved_rows: 0,
            })),
        }
    }

    pub fn check_cancel(&self) -> windows::core::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            if let Ok(mut state) = self.state.try_borrow_mut() {
                state.report.evidence.cancellation_confirmed = true;
            }
            return Err(E_ABORT.into());
        }
        Ok(())
    }

    pub fn record(
        &self,
        source: Ref<'_, IShellItem>,
        destination: Ref<'_, IShellItem>,
        result: HRESULT,
        relocation_identity_confirmed: bool,
    ) -> windows::core::Result<()> {
        let source = shell_path(&source);
        let destination = shell_path(&destination).ok();
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        let status = super::result::item_status(
            result,
            destination.is_some(),
            relocation_identity_confirmed,
        );
        let source = match source {
            Ok(source) => source,
            Err(_) => {
                state.count(status);
                state.report.evidence.outcomes_complete = false;
                return Err(E_ABORT.into());
            }
        };
        state.push(ItemOutcome {
            source,
            destination,
            status,
            hresult: result.0,
        })?;
        drop(state);
        self.check_cancel()
    }

    pub fn finish(
        &self,
        perform: windows::core::Result<()>,
        aborted: Option<bool>,
    ) -> OperationReport {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return OperationReport {
                failure: None,
                user_skipped: vec![],
                user_decisions_only: None,
                counts: OutcomeCounts::default(),
                items: vec![],
                evidence: FinishEvidence {
                    perform_succeeded: false,
                    aborted: None,
                    outcomes_complete: false,
                    cancellation_confirmed: false,
                },
            };
        };
        state.report.evidence.perform_succeeded =
            perform.is_ok() && state.finished.is_some_and(|hr| hr.is_ok());
        state.report.evidence.aborted = aborted;
        let counts = state.report.counts;
        let observed = [
            counts.changed,
            counts.skipped,
            counts.failed,
            counts.unknown,
        ]
        .into_iter()
        .fold(0, u32::saturating_add);
        let expected = u32::try_from(state.expected).unwrap_or(u32::MAX);
        state.report.counts.unknown = counts
            .unknown
            .saturating_add(expected.saturating_sub(observed));
        state.report.evidence.outcomes_complete &=
            state.finished.is_some() && state.report.items.len() == state.expected;
        state.report.clone()
    }
}

pub(super) fn shell_path(item: &Ref<'_, IShellItem>) -> windows::core::Result<String> {
    let item = item
        .as_ref()
        .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
    // SAFETY: COM category 8: item is a live borrowed interface on its STA. The
    // returned NUL-terminated UTF-16 allocation belongs to this caller until freed.
    let value = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
    // SAFETY: category 8: the Shell contract supplies a live NUL-terminated buffer.
    // Convert before the matching CoTaskMemFree; no pointer or borrow escapes.
    let result = unsafe {
        let result = value.to_string();
        CoTaskMemFree(Some(value.0.cast()));
        result
    }
    .map_err(|_| windows::core::Error::from(E_FAIL))?;
    if result.is_empty() || result.len() > 32768 {
        return Err(E_FAIL.into());
    }
    Ok(result)
}
