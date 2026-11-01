use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProgressSnapshot {
    pub total: u32,
    pub completed: u32,
}

#[derive(Default)]
pub struct NativeProgress(AtomicU64);

impl NativeProgress {
    pub(super) fn update(&self, total: u32, completed: u32) {
        let packed = if total == 0 || completed > total {
            0
        } else {
            (u64::from(total) << 32) | u64::from(completed)
        };
        self.0.store(packed, Ordering::Release);
    }

    pub fn snapshot(&self) -> Option<NativeProgressSnapshot> {
        let packed = self.0.load(Ordering::Acquire);
        (packed != 0).then_some(NativeProgressSnapshot {
            total: (packed >> 32) as u32,
            completed: packed as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_estimates_preserve_revisions_and_invalid_pairs_become_indeterminate() {
        let progress = NativeProgress::default();
        assert_eq!(progress.snapshot(), None);
        for (total, completed) in [(10, 2), (40, 3), (40, 40), (u32::MAX, u32::MAX)] {
            progress.update(total, completed);
            assert_eq!(
                progress.snapshot(),
                Some(NativeProgressSnapshot { total, completed })
            );
        }
        for (total, completed) in [(0, 0), (0, 1), (5, 6)] {
            progress.update(total, completed);
            assert_eq!(progress.snapshot(), None);
        }
    }
}
