use super::{NativeProgress, execute::Apartment, message_pump::pump};
use crate::domain::{
    ActivationTarget, Fault, FaultCode,
    operations::{
        OperationPlan, OperationReport, OperationSource, PrepareOperation, PrepareTransfer,
        TransferMode,
    },
};
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::{
        Arc, Mutex,
        atomic::AtomicBool,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::oneshot;
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

enum Command {
    Capture {
        targets: Vec<ActivationTarget>,
        reply: oneshot::Sender<Result<Vec<OperationSource>, Fault>>,
    },
    Transfer {
        request: PrepareTransfer,
        mode: TransferMode,
        sources: Vec<OperationSource>,
        reply: oneshot::Sender<Result<OperationPlan, Fault>>,
    },
    Prepare {
        request: PrepareOperation,
        targets: Vec<ActivationTarget>,
        reply: oneshot::Sender<Result<OperationPlan, Fault>>,
    },
    Execute {
        plan: OperationPlan,
        owner: usize,
        cancel: Arc<AtomicBool>,
        progress: Arc<NativeProgress>,
        reply: oneshot::Sender<Result<OperationReport, Fault>>,
    },
    Shutdown(oneshot::Sender<()>),
}

pub struct NativeFileOperationLane {
    sender: SyncSender<Command>,
    wake: Arc<OwnedHandle>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl NativeFileOperationLane {
    pub fn start() -> Result<Self, Fault> {
        // SAFETY: FFI category 8: null optional attributes/name and initialized
        // Boolean values create an unnamed auto-reset event owned by this instance.
        let raw = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if raw.is_null() {
            return Err(Fault::from(std::io::Error::last_os_error()));
        }
        // SAFETY: handle category 7: CreateEventW returned a new non-null handle;
        // ownership transfers exactly once and OwnedHandle closes it after both users.
        let wake = Arc::new(unsafe { OwnedHandle::from_raw_handle(raw) });
        let (sender, receiver) = mpsc::sync_channel(32);
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        let thread_wake = Arc::clone(&wake);
        let thread = thread::Builder::new()
            .name("projectpane-file-operations".into())
            .spawn(move || {
                let apartment = match Apartment::initialize() {
                    Ok(apartment) => apartment,
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                        return;
                    }
                };
                if ready_send.send(Ok(())).is_err() {
                    return;
                }
                let shutdown = run(&apartment, receiver, &thread_wake);
                drop(apartment);
                if let Some(reply) = shutdown {
                    let _ = reply.send(());
                }
            })
            .map_err(Fault::from)?;
        ready_receive
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))??;
        Ok(Self {
            sender,
            wake,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub async fn prepare(
        &self,
        request: PrepareOperation,
        targets: Vec<ActivationTarget>,
    ) -> Result<OperationPlan, Fault> {
        request.validate()?;
        let (reply, receive) = oneshot::channel();
        self.send(Command::Prepare {
            request,
            targets,
            reply,
        })?;
        receive
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?
    }

    pub async fn capture(
        &self,
        targets: Vec<ActivationTarget>,
    ) -> Result<Vec<OperationSource>, Fault> {
        let (reply, receive) = oneshot::channel();
        self.send(Command::Capture { targets, reply })?;
        receive
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?
    }

    pub async fn prepare_transfer(
        &self,
        request: PrepareTransfer,
        mode: TransferMode,
        sources: Vec<OperationSource>,
    ) -> Result<OperationPlan, Fault> {
        let (reply, receive) = oneshot::channel();
        self.send(Command::Transfer {
            request,
            mode,
            sources,
            reply,
        })?;
        receive
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?
    }

    pub async fn execute(
        &self,
        plan: OperationPlan,
        owner: usize,
        cancel: Arc<AtomicBool>,
    ) -> Result<OperationReport, Fault> {
        self.execute_observed(plan, owner, cancel, Arc::default())
            .await
    }

    pub async fn execute_observed(
        &self,
        plan: OperationPlan,
        owner: usize,
        cancel: Arc<AtomicBool>,
        progress: Arc<NativeProgress>,
    ) -> Result<OperationReport, Fault> {
        plan.validate()?;
        let (reply, receive) = oneshot::channel();
        self.send(Command::Execute {
            plan,
            owner,
            cancel,
            progress,
            reply,
        })?;
        receive
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?
    }

    pub async fn shutdown(&self) -> Result<(), Fault> {
        let (reply, receive) = oneshot::channel();
        self.send(Command::Shutdown(reply))?;
        receive
            .await
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let thread = self
            .thread
            .lock()
            .map_err(|_| Fault::new(FaultCode::InvalidState))?
            .take();
        if let Some(thread) = thread {
            thread
                .join()
                .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        }
        Ok(())
    }

    fn send(&self, command: Command) -> Result<(), Fault> {
        self.sender.try_send(command).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => Fault::new(FaultCode::Busy),
            mpsc::TrySendError::Disconnected(_) => Fault::new(FaultCode::WorkerUnavailable),
        })?;
        // SAFETY: handle category 7: this instance retains an Arc owning the event
        // for the call. A failed wake cannot revoke queued work; the pump also polls.
        unsafe {
            SetEvent(self.wake.as_raw_handle());
        }
        Ok(())
    }
}

fn run(
    apartment: &Apartment,
    receiver: Receiver<Command>,
    wake: &OwnedHandle,
) -> Option<oneshot::Sender<()>> {
    loop {
        match receiver.try_recv() {
            Ok(Command::Capture { targets, reply }) => {
                let _ = reply.send(super::prepare::capture(targets));
            }
            Ok(Command::Transfer {
                request,
                mode,
                sources,
                reply,
            }) => {
                let _ = reply.send(super::prepare::from_transfer(request, mode, sources));
            }
            Ok(Command::Prepare {
                request,
                targets,
                reply,
            }) => {
                let _ = reply.send(super::prepare::prepare(request, targets));
            }
            Ok(Command::Execute {
                plan,
                owner,
                cancel,
                progress,
                reply,
            }) => {
                let result = apartment.execute(plan, owner, cancel, progress);
                let _ = reply.send(result);
            }
            Ok(Command::Shutdown(reply)) => return Some(reply),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {
                if !pump(wake) {
                    return None;
                }
            }
        }
    }
}
