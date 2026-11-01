use crate::domain::{Fault, FaultCode};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Semaphore, oneshot};

pub(super) struct NativeLauncher {
    permit: Arc<Semaphore>,
}

impl Default for NativeLauncher {
    fn default() -> Self {
        Self {
            permit: Arc::new(Semaphore::new(1)),
        }
    }
}

impl NativeLauncher {
    pub(super) async fn run<T, F>(&self, deadline: Duration, job: F) -> Result<T, Fault>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, Fault> + Send + 'static,
    {
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        let (sender, receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name("native-open".into())
            .spawn(move || {
                // An IPC timeout does not cancel native execution. The dedicated
                // thread retains this permit until completion, bounding stuck calls to one.
                let result = job();
                drop(permit);
                let _ = sender.send(result);
            })
            .map_err(|_| Fault::new(FaultCode::ResourceLimit))?;
        tokio::time::timeout(deadline, receiver)
            .await
            .map_err(|_| Fault::new(FaultCode::Timeout))?
            .map_err(|_| Fault::new(FaultCode::IoError))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_keeps_native_capacity_busy_until_dispatch_really_finishes() {
        let launcher = Arc::new(NativeLauncher::default());
        let pending_launcher = launcher.clone();
        let (entered, started) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let pending = tokio::spawn(async move {
            pending_launcher
                .run(Duration::from_millis(80), move || {
                    let _ = entered.send(());
                    blocked.recv().map_err(|_| Fault::new(FaultCode::IoError))?;
                    Ok(())
                })
                .await
        });
        started.await.unwrap();
        let result = pending.await.unwrap();
        let retry = launcher.run(Duration::from_millis(10), || Ok(())).await;
        release.send(()).unwrap();
        assert_eq!(result.unwrap_err().code, FaultCode::Timeout);
        assert_eq!(retry.unwrap_err().code, FaultCode::Busy);
        tokio::time::timeout(Duration::from_secs(1), async {
            while launcher.permit.available_permits() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            launcher
                .run(Duration::from_secs(1), || Ok(()))
                .await
                .is_ok()
        );
    }
}
