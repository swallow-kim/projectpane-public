use projectpane_core::domain::{Fault, FaultCode, MAX_FRAME, StartupConfig};
use serde::Serialize;
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    StateUnavailable,
    StateInvalid,
    LogUnavailable,
    WindowRestoreFailed,
}

#[derive(Clone, Serialize)]
pub struct StartupResponse {
    pub folders: Vec<String>,
    pub error: Option<String>,
    pub warnings: Vec<Warning>,
}

impl StartupResponse {
    pub fn new(config: StartupConfig, warnings: Vec<Warning>) -> Self {
        let mut unique_warnings = Vec::with_capacity(4);
        for warning in warnings {
            if !unique_warnings.contains(&warning) {
                unique_warnings.push(warning);
            }
        }
        let mut response = Self {
            folders: config.folders,
            error: config.error,
            warnings: unique_warnings,
        };
        if !serde_json::to_vec(&response).is_ok_and(|bytes| bytes.len() < MAX_FRAME) {
            response.folders.clear();
            response.error = Some("invalid_startup_args".into());
        }
        response
    }
}

pub struct StartupState(pub watch::Receiver<Option<StartupResponse>>);

impl StartupState {
    pub async fn response(&self) -> Result<StartupResponse, Fault> {
        let mut receiver = self.0.clone();
        receiver
            .wait_for(|response| response.is_some())
            .await
            .map_err(|_| Fault::new(FaultCode::InvalidState))?
            .clone()
            .ok_or_else(|| Fault::new(FaultCode::InvalidState))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_waits_until_setup_publishes_final_response() {
        use std::{future::Future, task::Poll};
        let (sender, receiver) = watch::channel(None);
        let state = StartupState(receiver);
        let mut response = Box::pin(state.response());
        std::future::poll_fn(|context| {
            assert!(response.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        sender.send_replace(Some(StartupResponse::new(
            StartupConfig {
                folders: vec![],
                error: None,
            },
            warnings(),
        )));
        drop(sender);
        assert_eq!(response.await.unwrap().warnings, warnings());
    }

    #[tokio::test]
    async fn failed_setup_returns_fixed_fault_instead_of_hanging() {
        let (sender, receiver) = watch::channel(None);
        drop(sender);
        let result = StartupState(receiver).response().await;
        assert!(matches!(
            result,
            Err(Fault {
                code: FaultCode::InvalidState
            })
        ));
    }
    fn warnings() -> Vec<Warning> {
        vec![
            Warning::StateUnavailable,
            Warning::StateInvalid,
            Warning::LogUnavailable,
            Warning::WindowRestoreFailed,
        ]
    }
    #[test]
    fn empty_warning_array_is_required() {
        let response = StartupResponse::new(
            StartupConfig {
                folders: vec![],
                error: None,
            },
            vec![],
        );
        assert_eq!(
            serde_json::to_value(response).unwrap()["warnings"],
            serde_json::json!([])
        );
    }

    #[test]
    fn repeated_warnings_cannot_escape_the_frame_limit() {
        let response = StartupResponse::new(
            StartupConfig {
                folders: vec![],
                error: None,
            },
            vec![Warning::StateUnavailable; MAX_FRAME],
        );
        assert_eq!(response.warnings, vec![Warning::StateUnavailable]);
        assert!(serde_json::to_vec(&response).unwrap().len() < MAX_FRAME);
    }
    #[test]
    fn warnings_cannot_push_startup_over_frame_boundary() {
        let base = StartupResponse::new(
            StartupConfig {
                folders: vec![String::new()],
                error: None,
            },
            warnings(),
        );
        let overhead = serde_json::to_vec(&base).unwrap().len();
        for (size, accepted) in [
            (MAX_FRAME - overhead - 1, true),
            (MAX_FRAME - overhead, false),
        ] {
            let response = StartupResponse::new(
                StartupConfig {
                    folders: vec!["x".repeat(size)],
                    error: None,
                },
                warnings(),
            );
            assert_eq!(response.error.is_none(), accepted);
            assert!(serde_json::to_vec(&response).unwrap().len() < MAX_FRAME);
            assert_eq!(response.warnings, warnings());
        }
    }
    #[test]
    fn escaped_paths_are_measured_as_serialized_bytes() {
        let response = StartupResponse::new(
            StartupConfig {
                folders: vec!["\\".repeat(MAX_FRAME / 2)],
                error: None,
            },
            warnings(),
        );
        assert!(response.folders.is_empty());
        assert_eq!(response.error.as_deref(), Some("invalid_startup_args"));
    }
}
