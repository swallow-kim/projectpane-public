use super::{
    FolderRecycleSnapshot, capture_source,
    identity::recheck,
    sink_state::{SinkContext, shell_path},
};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, operations::*};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs, io,
    path::{Component, Path},
};
use windows::{
    Win32::{
        Foundation::{E_ABORT, E_FAIL},
        UI::Shell::{IShellItem, TSF_DELETE_RECYCLE_IF_POSSIBLE},
    },
    core::{HRESULT, Ref},
};

pub(super) fn in_recycle_bin(path: &str) -> bool {
    matches!(Path::new(path).components().nth(2), Some(Component::Normal(name)) if name.eq_ignore_ascii_case("$Recycle.Bin"))
}

pub(super) fn check(sources: &[OperationSource]) -> Result<(), Fault> {
    let mut bytes = 4096;
    for source in sources {
        if source.kind != EntryKind::File
            || !super::identity::fixed_local(&source.path)
            || in_recycle_bin(&source.path)
        {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        recheck(source)?;
        bytes += bounded_json(source)?.len() + 4096;
        if bytes >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct RecycleProgress {
    authorized: RefCell<BTreeMap<String, AuthorizedRecycle>>,
}

enum AuthorizedRecycle {
    File(OperationSource),
    Folder(FolderRecycleSnapshot),
}

impl AuthorizedRecycle {
    fn source(&self) -> &OperationSource {
        match self {
            Self::File(source) => source,
            Self::Folder(snapshot) => snapshot.root(),
        }
    }

    fn confirms(&self, destination: &str) -> bool {
        match self {
            Self::Folder(snapshot) => snapshot.confirm_recycled(destination).is_ok(),
            Self::File(expected) => {
                in_recycle_bin(destination)
                    && capture_source(destination).is_ok_and(|actual| {
                        actual.kind == EntryKind::File
                            && actual.volume == expected.volume
                            && actual.file_index == expected.file_index
                            && fs::symlink_metadata(&expected.path)
                                .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
                    })
            }
        }
    }
}

impl RecycleProgress {
    pub fn before(
        &self,
        plan: &OperationPlan,
        flags: u32,
        source: &Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            return Err(E_ABORT.into());
        }
        let path = shell_path(source)?;
        let actual = capture_source(&path).map_err(|_| windows::core::Error::from(E_ABORT))?;
        let approval = match &plan.action {
            PreparedAction::Recycle { sources } if sources.contains(&actual) => {
                check(std::slice::from_ref(&actual))
                    .map_err(|_| windows::core::Error::from(E_ABORT))?;
                AuthorizedRecycle::File(actual)
            }
            PreparedAction::RecycleTree { root, descendants } if root == &actual => {
                let snapshot = FolderRecycleSnapshot::from_prepared(root, descendants)
                    .map_err(|_| windows::core::Error::from(E_ABORT))?;
                AuthorizedRecycle::Folder(snapshot)
            }
            _ => return Err(E_ABORT.into()),
        };
        let mut authorized = self
            .authorized
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        if authorized.contains_key(&path) {
            return Err(E_ABORT.into());
        }
        authorized.insert(path, approval);
        Ok(())
    }

    pub fn after(
        &self,
        context: &SinkContext,
        flags: u32,
        source: &Ref<'_, IShellItem>,
        result: HRESULT,
        destination: &Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        let path = shell_path(source)?;
        let expected = self
            .authorized
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .remove(&path);
        let destination = shell_path(destination).ok();
        let confirmed = flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 != 0
            && expected.as_ref().is_some_and(|expected| {
                destination
                    .as_deref()
                    .is_some_and(|path| expected.confirms(path))
            });
        let status = super::result::item_status(result, confirmed, confirmed);
        context
            .state
            .try_borrow_mut()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .push(ItemOutcome {
                source: expected.map_or(path, |value| value.source().path.clone()),
                destination,
                status,
                hresult: result.0,
            })?;
        if status == ItemStatus::Unknown {
            return Err(E_ABORT.into());
        }
        context.check_cancel()
    }
}
