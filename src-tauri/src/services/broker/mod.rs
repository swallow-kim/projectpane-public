use crate::{domain::*, platform::session::Session};
mod display;
mod native_launch;
mod progress;
mod view;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
#[derive(Default)]
struct Slot {
    generation: u64,
    session: Option<Arc<Session>>,
}

pub struct Broker {
    executable: PathBuf,
    deadline: Duration,
    slots: Arc<Mutex<[Slot; 4]>>,
    starts: [AsyncMutex<()>; 4],
    native_launch: native_launch::NativeLauncher,
}

impl Broker {
    pub fn new(executable: PathBuf, deadline: Duration) -> Self {
        Self {
            executable,
            deadline,
            slots: Arc::new(Mutex::new(std::array::from_fn(|_| Slot::default()))),
            starts: std::array::from_fn(|_| AsyncMutex::new(())),
            native_launch: native_launch::NativeLauncher::default(),
        }
    }

    pub async fn start(&self, identity: Identity, path: String) -> Result<Reply, Fault> {
        self.start_view(identity, path, ListingView::default())
            .await
    }

    pub async fn start_view(
        &self,
        identity: Identity,
        path: String,
        view: ListingView,
    ) -> Result<Reply, Fault> {
        identity.validate()?;
        view.validate()?;
        let path = AbsolutePath::new(path)?.into_string();
        // At most four starts; replacements must reap the previous process first.
        let _start = self.starts[identity.pane]
            .try_lock()
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        let old = {
            let mut slots = self
                .slots
                .lock()
                .map_err(|_| Fault::new(FaultCode::InvalidState))?;
            let slot = &mut slots[identity.pane];
            if identity.generation <= slot.generation {
                return Err(Fault::new(FaultCode::StaleRequest));
            }
            slot.generation = identity.generation;
            slot.session.take()
        };
        if let Some(old) = old {
            old.stop().await;
        }
        let session = Arc::new(Session::spawn(&self.executable, identity.clone()).await?);
        self.slots
            .lock()
            .map_err(|_| Fault::new(FaultCode::InvalidState))?[identity.pane]
            .session = Some(session.clone());
        let result = session
            .exchange(Request::Open { path, view }, self.deadline)
            .await;
        if result.is_err() {
            self.cancel(&identity).await?;
        }
        // Cancellation or replacement must not resurrect a completed old reply.
        if result.is_ok() {
            self.get(&identity)?;
        }
        result
    }

    fn get(&self, identity: &Identity) -> Result<Arc<Session>, Fault> {
        identity.validate()?;
        self.slots
            .lock()
            .map_err(|_| Fault::new(FaultCode::InvalidState))?[identity.pane]
            .session
            .as_ref()
            .filter(|session| session.identity == *identity)
            .cloned()
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))
    }

    pub async fn read(&self, identity: Identity, offset: Option<usize>) -> Result<Reply, Fault> {
        let request = offset.map_or(Request::Advance, |offset| Request::Window {
            offset,
            limit: BATCH_SIZE,
        });
        self.read_request(identity, request, self.deadline).await
    }

    async fn read_request(
        &self,
        identity: Identity,
        request: Request,
        deadline: Duration,
    ) -> Result<Reply, Fault> {
        let session = self.get(&identity)?;
        let result = session.exchange(request, deadline).await;
        if result.as_ref().is_err_and(|e| e.code != FaultCode::Busy) {
            self.cancel(&identity).await?;
        }
        if result.is_ok() {
            self.get(&identity)?;
        }
        result
    }

    pub async fn cancel(&self, identity: &Identity) -> Result<(), Fault> {
        identity.validate()?;
        match self.get(identity) {
            Err(fault) if fault.code == FaultCode::StaleRequest => return Ok(()),
            Err(fault) => return Err(fault),
            Ok(session) => {
                {
                    let slots = self
                        .slots
                        .lock()
                        .map_err(|_| Fault::new(FaultCode::InvalidState))?;
                    if slots[identity.pane]
                        .session
                        .as_ref()
                        .is_some_and(|s| s.identity == *identity)
                    {
                        session.cancelled.send_replace(true);
                    }
                }
                // Keep the slot occupied until the child is reaped. Otherwise a
                // concurrent start could spawn a fifth child while cancellation waits.
                session.stop().await;
                let mut slots = self
                    .slots
                    .lock()
                    .map_err(|_| Fault::new(FaultCode::InvalidState))?;
                let slot = &mut slots[identity.pane];
                if slot
                    .session
                    .as_ref()
                    .is_some_and(|s| s.identity == *identity)
                {
                    slot.session.take();
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests;
