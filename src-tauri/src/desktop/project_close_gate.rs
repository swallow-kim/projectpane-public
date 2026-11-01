#[derive(Default)]
pub(super) struct CloseGate {
    sequence: u32,
    pending: Option<u32>,
    exit_confirmed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Completion {
    Ignore,
    Keep,
    Exit,
}

impl CloseGate {
    pub fn begin(&mut self) -> Option<u32> {
        if self.pending.is_some() || self.exit_confirmed {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        self.pending = Some(self.sequence);
        self.pending
    }

    pub fn complete(&mut self, attempt: u32, saved: bool) -> Completion {
        if self.pending != Some(attempt) {
            return Completion::Ignore;
        }
        self.pending = None;
        if saved {
            self.exit_confirmed = true;
            Completion::Exit
        } else {
            Completion::Keep
        }
    }

    pub fn exit_allowed(&self) -> bool {
        self.exit_confirmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_save_or_timeout_keeps_window_and_allows_a_new_attempt() {
        let mut gate = CloseGate::default();
        let first = gate.begin().unwrap();
        assert_eq!(gate.begin(), None);
        assert_eq!(gate.complete(first, false), Completion::Keep);
        assert!(!gate.exit_allowed());
        let second = gate.begin().unwrap();
        assert_ne!(first, second);
        assert_eq!(gate.complete(first, true), Completion::Ignore);
        assert_eq!(gate.complete(first, false), Completion::Ignore);
        assert!(!gate.exit_allowed());
        assert_eq!(gate.complete(second, true), Completion::Exit);
        assert!(gate.exit_allowed());
        assert_eq!(gate.begin(), None);
    }

    #[test]
    fn unrequested_and_late_acknowledgements_never_allow_exit() {
        let mut gate = CloseGate::default();
        assert_eq!(gate.complete(0, true), Completion::Ignore);
        let attempt = gate.begin().unwrap();
        assert_eq!(gate.complete(attempt, false), Completion::Keep);
        assert_eq!(gate.complete(attempt, true), Completion::Ignore);
        assert!(!gate.exit_allowed());
    }

    #[test]
    fn exhausted_attempt_ids_do_not_wrap() {
        let mut gate = CloseGate {
            sequence: u32::MAX,
            ..CloseGate::default()
        };
        assert_eq!(gate.begin(), None);
        assert!(!gate.exit_allowed());
    }
}
