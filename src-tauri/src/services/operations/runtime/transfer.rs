use super::*;

pub(super) struct CapturedTransfer {
    preview: TransferPreview,
    sources: Vec<OperationSource>,
    pub prepared: Option<String>,
}

impl OperationService {
    pub async fn capture_transfer(
        self: &Arc<Self>,
        request: CaptureTransfer,
        targets: Vec<ActivationTarget>,
    ) -> Result<TransferPreview, Fault> {
        request.validate()?;
        if request.selection.entry_ids.len() != targets.len() {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        self.capture_targets(request.mode, targets).await
    }

    pub async fn capture_external(
        self: &Arc<Self>,
        paths: Vec<String>,
    ) -> Result<TransferPreview, Fault> {
        let targets =
            tokio::task::spawn_blocking(move || crate::platform::explorer::targets(paths))
                .await
                .map_err(|_| Fault::new(FaultCode::IoError))??;
        self.capture_targets(TransferMode::Copy, targets).await
    }

    async fn capture_targets(
        self: &Arc<Self>,
        mode: TransferMode,
        targets: Vec<ActivationTarget>,
    ) -> Result<TransferPreview, Fault> {
        let sequence = self
            .access(|state| {
                state.queue.admission_ready()?;
                state.transfer_sequence = state
                    .transfer_sequence
                    .checked_add(1)
                    .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
                if let Some(old) = state.transfer.take()
                    && let Some(id) = old.prepared
                {
                    state.queue.discard_prepared(&id);
                }
                Ok(state.transfer_sequence)
            })
            .await?;
        let sources = self.lane.capture(targets).await?;
        self.access(move |state| {
            state.queue.admission_ready()?;
            if sequence != state.transfer_sequence {
                return Err(Fault::new(FaultCode::StaleRequest));
            }
            let preview = TransferPreview {
                token: crate::services::id::new_id().map_err(Fault::from)?,
                mode,
                count: sources.len(),
            };
            state.transfer = Some(CapturedTransfer {
                preview: preview.clone(),
                sources,
                prepared: None,
            });
            Ok(preview)
        })
        .await
    }

    pub async fn prepare_transfer(
        self: &Arc<Self>,
        request: PrepareTransfer,
    ) -> Result<OperationPreview, Fault> {
        request.validate()?;
        let token = request.token.clone();
        let (mode, sources) = self
            .access(move |state| {
                state.queue.admission_ready()?;
                let transfer = state
                    .transfer
                    .as_ref()
                    .filter(|transfer| transfer.preview.token == token)
                    .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
                if transfer.prepared.is_some() {
                    return Err(Fault::new(FaultCode::Busy));
                }
                Ok((transfer.preview.mode, transfer.sources.clone()))
            })
            .await?;
        let token = request.token.clone();
        let plan = self.lane.prepare_transfer(request, mode, sources).await?;
        self.access(move |state| {
            let transfer = state
                .transfer
                .as_mut()
                .filter(|transfer| transfer.preview.token == token)
                .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
            if transfer.prepared.is_some() {
                return Err(Fault::new(FaultCode::Busy));
            }
            let id = state.queue.prepare(plan.clone())?;
            let preview = match OperationPreview::new(id.clone(), &plan) {
                Ok(preview) => preview,
                Err(error) => {
                    state.queue.discard_prepared(&id);
                    return Err(error);
                }
            };
            if mode == TransferMode::Move {
                transfer.prepared = Some(id);
            }
            Ok(preview)
        })
        .await
    }
}
