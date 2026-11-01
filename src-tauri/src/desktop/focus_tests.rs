use super::*;

#[test]
fn delayed_focus_succeeds_before_deadline() {
    assert_eq!(
        poll_decision(Duration::ZERO, Ok(false)),
        PollDecision::Wait(POLL_INTERVAL)
    );
    assert_eq!(
        poll_decision(Duration::from_millis(50), Ok(false)),
        PollDecision::Wait(POLL_INTERVAL)
    );
    assert_eq!(
        poll_decision(Duration::from_millis(100), Ok(true)),
        PollDecision::Focused
    );
}

#[test]
fn permanent_unfocused_state_stops_at_deadline() {
    for millis in (0..500).step_by(50) {
        assert_eq!(
            poll_decision(Duration::from_millis(millis), Ok(false)),
            PollDecision::Wait(POLL_INTERVAL)
        );
    }
    assert_eq!(
        poll_decision(CONFIRM_LIMIT, Ok(false)),
        PollDecision::Failed
    );
}

#[test]
fn query_failure_is_terminal_without_waiting() {
    assert_eq!(poll_decision(Duration::ZERO, Err(())), PollDecision::Failed);
}

#[test]
fn exact_deadline_rejects_even_a_positive_late_observation() {
    assert_eq!(poll_decision(CONFIRM_LIMIT, Ok(true)), PollDecision::Failed);
    assert_eq!(
        poll_decision(CONFIRM_LIMIT + POLL_INTERVAL, Ok(true)),
        PollDecision::Failed
    );
}

#[test]
fn last_wait_never_exceeds_remaining_budget() {
    assert_eq!(
        poll_decision(Duration::from_millis(499), Ok(false)),
        PollDecision::Wait(Duration::from_millis(1))
    );
    assert_eq!(
        poll_decision(Duration::from_millis(499), Ok(true)),
        PollDecision::Focused
    );
}

#[test]
fn queued_callback_is_skipped_at_deadline_or_after_receiver_closes() {
    assert!(!dispatch_allowed(Duration::ZERO, false));
    assert!(!dispatch_allowed(Duration::ZERO, true));
    assert!(!dispatch_allowed(CONFIRM_LIMIT, true));
    assert!(dispatch_allowed(Duration::from_nanos(1), false));
}
