use super::{Broker, display::DISPLAY_PAGE_SIZE};
use crate::{domain::*, platform::activation};
use std::time::Instant;

impl Broker {
    pub async fn set_view(&self, identity: Identity, view: ListingView) -> Result<Reply, Fault> {
        view.validate()?;
        let mut reply = self
            .read_request(identity, Request::SetView { view }, self.deadline)
            .await?;
        reply.batch.entries.truncate(DISPLAY_PAGE_SIZE);
        Ok(reply)
    }

    async fn query(&self, identity: &Identity, request: Request) -> Result<WorkerResponse, Fault> {
        let session = self.get(identity)?;
        let result = session.exchange_response(request, self.deadline).await;
        if *session.cancelled.borrow() {
            self.cancel(identity).await?;
        }
        let response = result?;
        self.get(identity)?;
        Ok(response)
    }

    pub async fn locate(&self, identity: Identity, entry_id: String) -> Result<LocateReply, Fault> {
        match self.query(&identity, Request::Locate { entry_id }).await? {
            WorkerResponse::Located(location) => Ok(LocateReply {
                identity,
                revision: location.revision,
                index: location.index,
            }),
            WorkerResponse::Batch(_) | WorkerResponse::Target(_) | WorkerResponse::Targets(_) => {
                Err(Fault::new(FaultCode::ProtocolError))
            }
        }
    }

    pub async fn resolve_activation(
        &self,
        identity: &Identity,
        entry_id: String,
    ) -> Result<ActivationTarget, Fault> {
        match self.query(identity, Request::Activate { entry_id }).await? {
            WorkerResponse::Target(target) => Ok(target),
            WorkerResponse::Batch(_) | WorkerResponse::Located(_) | WorkerResponse::Targets(_) => {
                Err(Fault::new(FaultCode::ProtocolError))
            }
        }
    }

    pub async fn resolve_activations(
        &self,
        identity: &Identity,
        entry_ids: Vec<String>,
    ) -> Result<Vec<ActivationTarget>, Fault> {
        if entry_ids.is_empty() || entry_ids.len() > MAX_ACTIVATION_BATCH {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        match self
            .query(identity, Request::ActivateMany { entry_ids })
            .await?
        {
            WorkerResponse::Targets(targets) => Ok(targets),
            _ => Err(Fault::new(FaultCode::ProtocolError)),
        }
    }

    pub async fn activate(
        &self,
        identity: Identity,
        entry_id: String,
    ) -> Result<ActivationReply, Fault> {
        let started = Instant::now();
        let target = self.resolve_activation(&identity, entry_id).await?;
        match target.kind {
            ActivationKind::Folder => {
                return Ok(ActivationReply {
                    identity,
                    kind: target.kind,
                    path: Some(target.path),
                });
            }
            ActivationKind::File => {}
        }
        let slots = self.slots.clone();
        let deadline = self.deadline;
        self.native_launch
            .run(deadline.saturating_sub(started.elapsed()), move || {
                let slots = slots
                    .lock()
                    .map_err(|_| Fault::new(FaultCode::InvalidState))?;
                let current = slots[identity.pane]
                    .session
                    .as_ref()
                    .filter(|session| session.identity == identity)
                    .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
                if *current.cancelled.borrow() {
                    return Err(Fault::new(FaultCode::StaleRequest));
                }
                if started.elapsed() >= deadline {
                    return Err(Fault::new(FaultCode::Timeout));
                }
                // Activation is accepted against the current snapshot here. Release
                // the slot lock before native dispatch so a slow shell cannot block listings.
                drop(slots);
                activation::open_file(&target.path)?;
                Ok(ActivationReply {
                    identity,
                    kind: target.kind,
                    path: None,
                })
            })
            .await
    }
}
