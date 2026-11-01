use super::{check_plan, sink::ProgressSink_Impl};
use crate::domain::operations::{OperationPlan, PreparedAction};
use windows::{
    Win32::{Foundation::E_ABORT, UI::Shell::IShellItem},
    core::Ref,
};
impl ProgressSink_Impl {
    pub(super) fn relocation_confirmed(
        &self,
        old: &Ref<'_, IShellItem>,
        destination: &Ref<'_, IShellItem>,
    ) -> bool {
        let (source, expected) = match &self.plan.action {
            PreparedAction::Rename { source, name } => (
                source,
                std::path::Path::new(&source.path)
                    .parent()
                    .map(|parent| parent.join(name)),
            ),
            PreparedAction::Move {
                sources,
                destination,
            } => {
                let Ok(old) = super::sink_state::shell_path(old) else {
                    return false;
                };
                let Some(source) = sources
                    .iter()
                    .find(|source| source.path.eq_ignore_ascii_case(&old))
                else {
                    return false;
                };
                if !std::fs::symlink_metadata(&source.path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                {
                    return false;
                }
                let target = std::path::Path::new(&source.path)
                    .file_name()
                    .map(|name| std::path::Path::new(&destination.path).join(name));
                (source, target)
            }
            _ => return false,
        };
        let Ok(path) = super::sink_state::shell_path(destination) else {
            return false;
        };
        let Ok(actual) = super::capture_source(&path) else {
            return false;
        };
        let identity_matches =
            (source.volume, source.file_index) == (actual.volume, actual.file_index);
        let cross_volume_move = matches!(&self.plan.action, PreparedAction::Move { destination, .. }
            if source.volume != destination.volume && actual.volume == destination.volume
                && source.kind == crate::domain::EntryKind::File);
        expected.as_deref() == Some(std::path::Path::new(&actual.path))
            && source.kind == actual.kind
            && (identity_matches || cross_volume_move)
    }

    pub(super) fn before(&self, source: Ref<'_, IShellItem>) -> windows::core::Result<()> {
        self.context.check_cancel()?;
        let actual = super::sink_state::shell_path(&source)?;
        let action = match &self.plan.action {
            PreparedAction::Copy {
                sources,
                destination,
            }
            | PreparedAction::Move {
                sources,
                destination,
            } => {
                let picked = sources
                    .iter()
                    .find(|item| item.path.eq_ignore_ascii_case(&actual))
                    .ok_or_else(|| windows::core::Error::from(E_ABORT))?
                    .clone();
                if matches!(self.plan.action, PreparedAction::Copy { .. }) {
                    PreparedAction::Copy {
                        sources: vec![picked],
                        destination: destination.clone(),
                    }
                } else {
                    PreparedAction::Move {
                        sources: vec![picked],
                        destination: destination.clone(),
                    }
                }
            }
            action => action.clone(),
        };
        check_plan(&OperationPlan {
            project_id: self.plan.project_id.clone(),
            action,
        })
        .map_err(|_| windows::core::Error::from(E_ABORT))
    }
}
