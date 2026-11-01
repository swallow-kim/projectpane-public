use super::{check_plan, sink::ProgressSink, sink_state::SinkContext};
use crate::domain::{Fault, FaultCode, operations::*};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoUninitialize,
        },
        UI::Shell::*,
    },
    core::{HSTRING, PCWSTR},
};

pub(super) struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    pub fn initialize() -> Result<Self, Fault> {
        // SAFETY: COM category 8: invoked once on this dedicated OS thread. The
        // non-Send guard balances both S_OK and S_FALSE on the same thread.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
            .ok()
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        Ok(Self(PhantomData))
    }

    pub fn execute(
        &self,
        plan: OperationPlan,
        owner: usize,
        cancel: Arc<AtomicBool>,
        progress: Arc<super::NativeProgress>,
    ) -> Result<OperationReport, Fault> {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(OperationReport {
                failure: None,
                user_skipped: vec![],
                user_decisions_only: None,
                counts: OutcomeCounts::default(),
                items: vec![],
                evidence: FinishEvidence {
                    perform_succeeded: false,
                    aborted: Some(true),
                    outcomes_complete: true,
                    cancellation_confirmed: true,
                },
            });
        }
        check_plan(&plan)?;
        if let PreparedAction::Move {
            sources,
            destination,
        } = &plan.action
            && super::wsl_move::compatible(sources, destination)
        {
            return super::wsl_move::execute(sources, destination, &cancel, &progress);
        }
        if let PreparedAction::Archive {
            roots,
            descendants,
            destination,
            name,
            extract,
        } = &plan.action
        {
            return super::archive::execute(
                roots,
                descendants,
                destination,
                name,
                *extract,
                &cancel,
                &progress,
            );
        }
        let (plan, decisions) = super::skip_existing::dispatch(plan);
        if matches!(&plan.action, PreparedAction::Copy { sources, .. } if sources.is_empty()) {
            return super::skip_existing::decision_only(decisions);
        }
        let expected = match &plan.action {
            PreparedAction::Archive { .. } | PreparedAction::CopySkipExisting { .. } => {
                return Err(Fault::new(FaultCode::InvalidState));
            }
            PreparedAction::Copy { sources, .. }
            | PreparedAction::Move { sources, .. }
            | PreparedAction::Recycle { sources } => sources.len(),
            PreparedAction::NewFolder { .. }
            | PreparedAction::RecycleTree { .. }
            | PreparedAction::Rename { .. }
            | PreparedAction::CopyAs { .. } => 1,
            PreparedAction::CopyTree {
                roots, descendants, ..
            }
            | PreparedAction::MoveTree {
                roots, descendants, ..
            } => roots.len() + descendants.len(),
        };
        // Pin source parents and the destination namespace. Do not hold source
        // objects against deletion; the native Shell owns their move transition.
        let _move_parents = if let PreparedAction::MoveTree {
            roots, destination, ..
        } = &plan.action
        {
            let paths: Vec<_> = roots
                .iter()
                .filter_map(|s| std::path::Path::new(&s.path).parent())
                .chain([std::path::Path::new(&destination.path)])
                .collect();
            super::guarded_io::ancestors(&paths, &Default::default())?
        } else {
            vec![]
        };
        let context = SinkContext::new(expected, Arc::clone(&cancel), Arc::clone(&progress));
        let skipped_count =
            u32::try_from(decisions.len()).map_err(|_| Fault::new(FaultCode::ResourceLimit))?;
        if !decisions.is_empty() {
            let mut state = context.state.borrow_mut();
            state.reserved_rows = decisions.len();
            state.bytes += serde_json::to_vec(&decisions)
                .map_err(|_| Fault::new(FaultCode::InvalidRequest))?
                .len()
                + 32;
        }
        let observer = SinkContext {
            state: Rc::clone(&context.state),
            cancel,
            progress,
        };
        let moving_tree = super::move_tree_sink::TreeMoveProgress::default();
        let sink: IFileOperationProgressSink = ProgressSink {
            context,
            plan: plan.clone(),
            tree: super::copy_tree_sink::TreeCopyProgress::default(),
            recycle: super::recycle::RecycleProgress::default(),
            moving_tree: moving_tree.clone(),
        }
        .into();
        // SAFETY: COM category 8: all interfaces are created and released inside
        // this initialized STA; strings/interfaces outlive each synchronous call.
        // No service lock is held. Advise owns its reference until Unadvise/release.
        unsafe {
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
                    .map_err(native_fault)?;
            operation
                .SetOwnerWindow(HWND(owner as *mut _))
                .map_err(native_fault)?;
            let mut flags = FOF_NOERRORUI
                | FOFX_EARLYFAILURE
                | FOF_NO_CONNECTED_ELEMENTS
                | FOFX_NOCOPYHOOKS
                | FOF_NOCONFIRMMKDIR
                | FOF_SILENT;
            if matches!(
                plan.action,
                PreparedAction::Recycle { .. } | PreparedAction::RecycleTree { .. }
            ) {
                flags |= FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING;
            }
            if matches!(plan.action, PreparedAction::MoveTree { .. }) {
                flags |= FOFX_MOVEACLSACROSSVOLUMES;
            }
            operation.SetOperationFlags(flags).map_err(native_fault)?;
            let cookie = operation.Advise(&sink).map_err(native_fault)?;
            let queued = enqueue(&operation, &plan.action);
            let report = match queued {
                Ok(()) => {
                    let perform = operation.PerformOperations();
                    #[cfg(test)]
                    if matches!(plan.action, PreparedAction::MoveTree { .. }) {
                        eprintln!("move perform {perform:?}");
                    }
                    let aborted = operation
                        .GetAnyOperationsAborted()
                        .ok()
                        .map(|value| value.as_bool());
                    moving_tree.finish(&observer);
                    let mut report = observer.finish(perform, aborted);
                    report.counts.skipped += skipped_count;
                    report.user_skipped = decisions;
                    Ok(report)
                }
                Err(error) => Err(native_fault(error)),
            };
            let _ = operation.Unadvise(cookie);
            report
        }
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: COM category 8: this non-Send guard owns one successful apartment
        // initialization and drops only after all per-operation interfaces release.
        unsafe {
            CoUninitialize();
        }
    }
}

fn native_fault(_: windows::core::Error) -> Fault {
    Fault::new(FaultCode::IoError)
}

unsafe fn enqueue(
    operation: &IFileOperation,
    action: &PreparedAction,
) -> windows::core::Result<()> {
    // SAFETY: COM category 8: caller supplies a live interface on the current STA;
    // every generated HSTRING and IShellItem remains valid for its synchronous call.
    unsafe {
        match action {
            PreparedAction::Archive { .. } => Err(windows::Win32::Foundation::E_INVALIDARG.into()),
            PreparedAction::CopySkipExisting { .. } => {
                Err(windows::Win32::Foundation::E_INVALIDARG.into())
            }
            PreparedAction::NewFolder { parent, name } => operation.NewItem(
                &item(&parent.path)?,
                0x10,
                &HSTRING::from(name),
                PCWSTR::null(),
                None,
            ),
            PreparedAction::Rename { source, name } => {
                operation.RenameItem(&item(&source.path)?, &HSTRING::from(name), None)
            }
            PreparedAction::CopyAs {
                source,
                destination,
                name,
            } => operation.CopyItem(
                &item(&source.path)?,
                &item(&destination.path)?,
                &HSTRING::from(name),
                None,
            ),
            PreparedAction::Copy {
                sources,
                destination,
            }
            | PreparedAction::Move {
                sources,
                destination,
            }
            | PreparedAction::CopyTree {
                roots: sources,
                destination,
                ..
            }
            | PreparedAction::MoveTree {
                roots: sources,
                destination,
                ..
            } => {
                let destination = item(&destination.path)?;
                for source in sources {
                    let source = item(&source.path)?;
                    if matches!(
                        action,
                        PreparedAction::Copy { .. } | PreparedAction::CopyTree { .. }
                    ) {
                        operation.CopyItem(&source, &destination, PCWSTR::null(), None)?;
                    } else {
                        operation.MoveItem(&source, &destination, PCWSTR::null(), None)?;
                    }
                }
                Ok(())
            }
            PreparedAction::Recycle { sources } => {
                for source in sources {
                    operation.DeleteItem(&item(&source.path)?, None)?;
                }
                Ok(())
            }
            PreparedAction::RecycleTree { root, .. } => {
                operation.DeleteItem(&item(&root.path)?, None)
            }
        }
    }
}

unsafe fn item(path: &str) -> windows::core::Result<IShellItem> {
    // SAFETY: COM category 8: the validated absolute filesystem path has no NUL;
    // the HSTRING remains live until the synchronous call acquires its interface.
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path), None) }
}
