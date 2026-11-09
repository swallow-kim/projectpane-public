use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum WorkerResponse {
    Batch(Batch),
    Located(LocatedEntry),
    Target(ActivationTarget),
    Targets(Vec<ActivationTarget>),
}

impl WorkerResponse {
    pub(crate) fn validate(&self, request: &Request) -> Result<(), Fault> {
        match (self, request) {
            (Self::Batch(batch), _) => batch.validate(request),
            (Self::Located(location), Request::Locate { .. })
                if location.revision > 0
                    && location.revision <= 9_007_199_254_740_991
                    && location.index.is_none_or(|i| i < MAX_ENTRIES) =>
            {
                Ok(())
            }
            (Self::Target(target), Request::Activate { .. }) => {
                AbsolutePath::new(target.path.clone()).map(|_| ())
            }
            (Self::Targets(targets), Request::ActivateMany { entry_ids })
                if !entry_ids.is_empty()
                    && entry_ids.len() <= MAX_ACTIVATION_BATCH
                    && targets.len() == entry_ids.len() =>
            {
                for target in targets {
                    AbsolutePath::new(target.path.clone())?;
                }
                Ok(())
            }
            _ => Err(Fault::new(FaultCode::ProtocolError)),
        }
    }
}
