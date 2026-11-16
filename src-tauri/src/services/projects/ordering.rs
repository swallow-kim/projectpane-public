use super::{documents::*, *};
use std::collections::{HashMap, HashSet};

impl ProjectService {
    /// Compare and swap only the catalog order. Project/session/selection documents are untouched.
    pub fn reorder(&self, request: ReorderRequest) -> Result<Reordered, ProjectError> {
        let valid_ids = |ids: &[String]| -> std::io::Result<()> {
            if ids.len() > MAX_PROJECTS {
                return Err(api::fail("invalid_document"));
            }
            let mut seen = HashSet::new();
            for id in ids {
                uuid(id)?;
                if !seen.insert(id) {
                    return Err(api::fail("invalid_document"));
                }
            }
            Ok(())
        };
        valid_ids(&request.expected_order)?;
        valid_ids(&request.project_ids)?;
        if request.expected_order.len() != request.project_ids.len()
            || request
                .project_ids
                .iter()
                .any(|id| !request.expected_order.contains(id))
        {
            return Err(api::fail("invalid_document").into());
        }
        self.transaction(|tx| {
            let mut catalog = manifest(tx)?;
            if !catalog
                .projects
                .iter()
                .map(|project| &project.id)
                .eq(request.expected_order.iter())
            {
                return Err(api::fail("revision_conflict"));
            }
            if request.expected_order != request.project_ids {
                let mut entries: HashMap<_, _> = catalog
                    .projects
                    .into_iter()
                    .map(|project| (project.id.clone(), project))
                    .collect();
                catalog.projects = request
                    .project_ids
                    .iter()
                    .map(|id| {
                        entries
                            .remove(id)
                            .ok_or_else(|| api::fail("invalid_document"))
                    })
                    .collect::<Result<_, _>>()?;
                write_manifest(tx, &catalog)?;
            }
            Ok(Reordered {
                project_ids: request.project_ids,
            })
        })
        .map_err(Into::into)
    }
}
