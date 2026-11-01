//! Ordered handoff. Any uncertain result stops before the next destructive step.
use std::future::Future;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Verify,
    SaveWindow,
    StopWorkers,
    WriteReceipt,
    Install,
}
pub trait Handoff {
    fn perform(&mut self, step: Step) -> impl Future<Output = Result<(), &'static str>> + Send;
}
pub async fn run(mut handoff: impl Handoff) -> Result<(), &'static str> {
    for step in [
        Step::Verify,
        Step::SaveWindow,
        Step::StopWorkers,
        Step::WriteReceipt,
        Step::Install,
    ] {
        handoff.perform(step).await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    struct Fake {
        fail: Option<Step>,
        seen: Arc<Mutex<Vec<Step>>>,
    }
    impl Handoff for Fake {
        async fn perform(&mut self, step: Step) -> Result<(), &'static str> {
            self.seen.lock().unwrap().push(step);
            if self.fail == Some(step) {
                Err("injected uncertainty")
            } else {
                Ok(())
            }
        }
    }
    #[tokio::test]
    async fn failures_and_uncertain_shutdown_never_reach_the_installer() {
        let steps = [
            Step::Verify,
            Step::SaveWindow,
            Step::StopWorkers,
            Step::WriteReceipt,
            Step::Install,
        ];
        for (index, step) in steps.iter().enumerate() {
            let seen = Arc::new(Mutex::new(Vec::new()));
            assert!(
                run(Fake {
                    fail: Some(*step),
                    seen: seen.clone()
                })
                .await
                .is_err()
            );
            assert_eq!(*seen.lock().unwrap(), steps[..=index]);
            if *step != Step::Install {
                assert!(!seen.lock().unwrap().contains(&Step::Install));
            }
        }
    }
    #[tokio::test]
    async fn only_confirmed_handoff_installs_once_after_receipt() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        run(Fake {
            fail: None,
            seen: seen.clone(),
        })
        .await
        .unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [
                Step::Verify,
                Step::SaveWindow,
                Step::StopWorkers,
                Step::WriteReceipt,
                Step::Install
            ]
        );
    }
}
