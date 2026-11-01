use super::sink_state::SinkState;
use crate::domain::{
    MAX_FRAME,
    operations::{ItemOutcome, ItemStatus, MAX_OPERATION_RESULTS},
};
use windows::Win32::Foundation::E_ABORT;

impl SinkState {
    pub(super) fn count(&mut self, status: ItemStatus) {
        match status {
            ItemStatus::Changed => {
                self.report.counts.changed = self.report.counts.changed.saturating_add(1)
            }
            ItemStatus::Skipped => {
                self.report.counts.skipped = self.report.counts.skipped.saturating_add(1)
            }
            ItemStatus::Failed => {
                self.report.counts.failed = self.report.counts.failed.saturating_add(1)
            }
            ItemStatus::Unknown => {
                self.report.counts.unknown = self.report.counts.unknown.saturating_add(1)
            }
        }
    }

    pub(super) fn push(&mut self, item: ItemOutcome) -> windows::core::Result<()> {
        self.count(item.status);
        let size = match serde_json::to_vec(&item) {
            Ok(bytes) => bytes.len() + 1,
            Err(_) => {
                self.report.evidence.outcomes_complete = false;
                return Err(E_ABORT.into());
            }
        };
        if self.report.items.len() + self.reserved_rows >= MAX_OPERATION_RESULTS
            || self.bytes + size >= MAX_FRAME
            || !self.seen.insert(item.source.clone())
        {
            self.report.evidence.outcomes_complete = false;
            return Err(E_ABORT.into());
        }
        self.bytes += size;
        self.report.items.push(item);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::operations::OperationCompletion, platform::file_operations::sink_state::SinkContext,
    };
    use std::sync::{Arc, atomic::AtomicBool};
    use windows::core::HRESULT;

    #[test]
    fn missing_callbacks_are_counted_unknown_without_inventing_per_item_results() {
        let context = SinkContext::new(4, Arc::new(AtomicBool::new(false)), Arc::default());
        let mut state = context.state.borrow_mut();
        state
            .push(ItemOutcome {
                source: "C:\\fixture\\done".into(),
                destination: Some("C:\\target\\done".into()),
                status: ItemStatus::Changed,
                hresult: 0,
            })
            .unwrap();
        state
            .push(ItemOutcome {
                source: "C:\\fixture\\stopped".into(),
                destination: None,
                status: ItemStatus::Failed,
                hresult: windows::Win32::UI::Shell::COPYENGINE_E_USER_CANCELLED.0,
            })
            .unwrap();
        state.finished = Some(windows::Win32::Foundation::E_ABORT);
        state.report.evidence.cancellation_confirmed = true;
        drop(state);
        let report = context.finish(Err(windows::Win32::Foundation::E_ABORT.into()), Some(false));
        assert_eq!(report.counts.unknown, 2);
        assert_eq!(report.items.len(), 2);
        assert!(!report.evidence.outcomes_complete);
        assert_eq!(report.evidence.aborted, Some(false));
        assert_eq!(
            report.status(),
            crate::domain::operations::OperationStatus::InterruptedUnknown
        );
        report.validate().unwrap();
    }

    #[test]
    fn escaped_native_paths_cannot_make_the_completion_exceed_its_frame() {
        let context = SinkContext::new(100, Arc::new(AtomicBool::new(false)), Arc::default());
        let mut state = context.state.borrow_mut();
        for index in 0..100 {
            let source = format!("C:\\{}source-{index}", "a\\".repeat(3000));
            let destination = format!("D:\\{}target-{index}", "b\\".repeat(3000));
            if state
                .push(ItemOutcome {
                    source,
                    destination: Some(destination),
                    status: ItemStatus::Changed,
                    hresult: 0,
                })
                .is_err()
            {
                break;
            }
        }
        state.finished = Some(HRESULT(0));
        drop(state);
        let completion = OperationCompletion {
            schema_version: 1,
            id: "fixture-1".into(),
            report: context.finish(Ok(()), Some(false)),
        };
        assert!(!completion.report.evidence.outcomes_complete);
        assert!(serde_json::to_vec(&completion).unwrap().len() < MAX_FRAME);
        assert!(completion.validate().is_ok());
    }
}
