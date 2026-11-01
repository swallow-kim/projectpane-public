use super::*;
use crate::domain::MAX_FRAME;

#[cfg(test)]
#[path = "copy_as_tests.rs"]
mod copy_as_tests;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Compress,
    Extract,
    NewFolder,
    Rename,
    Copy,
    CopyAs,
    CopySkipExisting,
    Move,
    Recycle,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPreview {
    pub id: String,
    pub project_id: Option<String>,
    pub kind: OperationKind,
    pub sources: Vec<String>,
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_sources: Option<Vec<String>>,
}

impl OperationPreview {
    pub(super) fn new(id: String, plan: &OperationPlan) -> Result<Self, Fault> {
        let (kind, sources, destination) = match &plan.action {
            PreparedAction::Archive {
                roots,
                destination,
                name,
                extract,
                ..
            } => (
                if *extract {
                    OperationKind::Extract
                } else {
                    OperationKind::Compress
                },
                roots.iter().map(|s| s.path.clone()).collect(),
                Some(Path::new(&destination.path).join(name)),
            ),
            PreparedAction::NewFolder { parent, name } => (
                OperationKind::NewFolder,
                vec![],
                Some(Path::new(&parent.path).join(name)),
            ),
            PreparedAction::Rename { source, name } => (
                OperationKind::Rename,
                vec![source.path.clone()],
                Some(Path::new(&source.path).with_file_name(name)),
            ),
            PreparedAction::CopySkipExisting {
                sources,
                destination,
                ..
            } => (
                OperationKind::CopySkipExisting,
                sources.iter().map(|source| source.path.clone()).collect(),
                Some(destination.path.clone().into()),
            ),
            PreparedAction::CopyAs {
                source,
                destination,
                name,
            } => (
                OperationKind::CopyAs,
                vec![source.path.clone()],
                Some(Path::new(&destination.path).join(name)),
            ),
            PreparedAction::Copy {
                sources,
                destination,
            }
            | PreparedAction::CopyTree {
                roots: sources,
                destination,
                ..
            } => (
                OperationKind::Copy,
                sources.iter().map(|item| item.path.clone()).collect(),
                Some(destination.path.clone().into()),
            ),
            PreparedAction::Move {
                sources,
                destination,
            }
            | PreparedAction::MoveTree {
                roots: sources,
                destination,
                ..
            } => (
                OperationKind::Move,
                sources.iter().map(|item| item.path.clone()).collect(),
                Some(destination.path.clone().into()),
            ),
            PreparedAction::Recycle { sources } => (
                OperationKind::Recycle,
                sources.iter().map(|item| item.path.clone()).collect(),
                None,
            ),
            PreparedAction::RecycleTree { root, .. } => {
                (OperationKind::Recycle, vec![root.path.clone()], None)
            }
        };
        let destination = destination
            .map(|path| {
                path.into_os_string()
                    .into_string()
                    .map_err(|_| Fault::new(FaultCode::InvalidPath))
            })
            .transpose()?;
        let preview = Self {
            id,
            project_id: plan.project_id.clone(),
            skipped_sources: match &plan.action {
                PreparedAction::CopySkipExisting { skipped, .. } => Some(
                    skipped
                        .iter()
                        .map(|decision| decision.source.clone())
                        .collect(),
                ),
                _ => None,
            },
            kind,
            sources,
            destination,
        };
        if serde_json::to_vec(&preview)
            .map_err(|_| Fault::new(FaultCode::ProtocolError))?
            .len()
            >= MAX_FRAME
        {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        Ok(preview)
    }
}
