use super::Listing;
use crate::domain::*;

impl Listing {
    pub(super) fn respond(&mut self, request: Request) -> Result<WorkerResponse, Fault> {
        match request {
            Request::Locate { entry_id } => self.locate(&entry_id).map(WorkerResponse::Located),
            Request::Activate { entry_id } => self
                .activation_target(&entry_id)
                .map(WorkerResponse::Target),
            Request::ActivateMany { entry_ids } => self
                .activation_targets(&entry_ids)
                .map(WorkerResponse::Targets),
            other => self.handle(other).map(WorkerResponse::Batch),
        }
    }
}
