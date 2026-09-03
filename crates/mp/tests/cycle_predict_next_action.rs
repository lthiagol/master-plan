//! M213 / AC-07: `predict_next_action(cycle_state, recent_events)`
//! is a pure function with at least 6 named outcomes.
//!
//! The TUI consumes this directly so users can see "what's next"
//! for each queued milestone. The closed set is exactly the
//! 6 outcomes the spec mandates:
//!
//! - `DispatchRunner`  — cycle is Dispatching; compose + send
//!   the runner assignment.
//! - `DispatchReviewer` — runner done (3-pane / 2-pane); compose
//!   + send the reviewer assignment.
//! - `AwaitRunner`     — runner dispatched; awaiting the
//!   "completed-execute" notification.
//! - `AwaitReviewer`   — reviewer dispatched; awaiting the
//!   verdict.
//! - `ApplyMatrix`     — verdict in hand; apply the decision
//!   matrix.
//! - `EscalateUser`    — cycle cap or topology-block reached.
//!
//! Plus a `NoOp` sentinel for terminal states (Complete → NoOp).

use mp::autopilot::cycle::{predict_next_action, CycleState, CycleStateMachine, NextAction};
use mp::autopilot::events::{EventKind, OrchestrationEvent};
use mp::autopilot::role::Topology;
use mp::autopilot::session::RoleName;

fn cycle_state(s: CycleState, topology: Topology) -> CycleStateMachine {
    let mut m = CycleStateMachine::new("M213", topology);
    m.state = s;
    m
}

fn event(seq: u64, kind: EventKind) -> OrchestrationEvent {
    let mut e = OrchestrationEvent::new(seq, kind, "test", serde_json::json!({}));
    e.role = Some(RoleName::Runner);
    e
}

#[test]
fn dispatching_state_returns_dispatch_runner() {
    let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::DispatchRunner);
    assert_eq!(out.as_str(), "dispatch-runner");
}

#[test]
fn waiting_runner_state_returns_await_runner() {
    let s = cycle_state(CycleState::WaitingRunner, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::AwaitRunner);
}

#[test]
fn reviewing_state_returns_dispatch_reviewer_when_no_assignment_dispatched() {
    // Fresh Reviewing entry — no AssignmentDispatched event yet.
    let s = cycle_state(CycleState::Reviewing, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::DispatchReviewer);
}

#[test]
fn reviewing_state_returns_await_reviewer_after_assignment_dispatched() {
    // A recent AssignmentDispatched event means the reviewer
    // prompt was sent — we're now waiting for the verdict.
    let s = cycle_state(CycleState::Reviewing, Topology::ThreeAgent);
    let events = vec![event(1, EventKind::AssignmentDispatched)];
    let out = predict_next_action(&s, &events);
    assert_eq!(out, NextAction::AwaitReviewer);
}

#[test]
fn deciding_state_returns_apply_matrix() {
    let s = cycle_state(CycleState::Deciding, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::ApplyMatrix);
}

#[test]
fn cycle_next_state_returns_dispatch_runner() {
    // After CycleNext, the next action is to start the next
    // cycle (which means dispatching the runner).
    let s = cycle_state(CycleState::CycleNext, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::DispatchRunner);
}

#[test]
fn escalate_state_returns_escalate_user() {
    let s = cycle_state(CycleState::Escalate, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::EscalateUser);
}

#[test]
fn complete_state_returns_no_op() {
    let s = cycle_state(CycleState::Complete, Topology::ThreeAgent);
    let out = predict_next_action(&s, &[]);
    assert_eq!(out, NextAction::NoOp);
}

#[test]
fn predict_next_action_is_pure_under_repeated_calls() {
    // The function must be deterministic: calling it twice with
    // the same inputs yields the same result. This pins the
    // "pure function" contract.
    let s = cycle_state(CycleState::Reviewing, Topology::ThreeAgent);
    let events = vec![event(1, EventKind::AssignmentDispatched)];
    let out1 = predict_next_action(&s, &events);
    let out2 = predict_next_action(&s, &events);
    assert_eq!(out1, out2);
}

#[test]
fn predict_next_action_has_at_least_six_named_outcomes() {
    // The spec mandates at least 6 named outcomes. We have 7
    // (the 6 spec'd plus NoOp). Pin the count so a future
    // rename / removal is a deliberate test edit.
    let named = [
        NextAction::DispatchRunner,
        NextAction::DispatchReviewer,
        NextAction::AwaitRunner,
        NextAction::AwaitReviewer,
        NextAction::ApplyMatrix,
        NextAction::EscalateUser,
        NextAction::NoOp,
    ];
    assert!(named.len() >= 6);
    // Each variant has a distinct wire form (used by the TUI).
    let wire: std::collections::BTreeSet<&'static str> = named.iter().map(|n| n.as_str()).collect();
    assert_eq!(wire.len(), named.len());
}

#[test]
fn predict_next_action_handles_empty_event_journal() {
    // Empty event journal → no AssignmentDispatched tail →
    // DispatchRunner at Dispatching, DispatchReviewer at
    // Reviewing.
    let s = cycle_state(CycleState::Dispatching, Topology::ThreeAgent);
    assert_eq!(predict_next_action(&s, &[]), NextAction::DispatchRunner);

    let s = cycle_state(CycleState::Reviewing, Topology::ThreeAgent);
    assert_eq!(predict_next_action(&s, &[]), NextAction::DispatchReviewer);
}

#[test]
fn predict_next_action_uses_most_recent_event_for_dispatching_state() {
    // If the journal shows a fresh Dispatch event (not an
    // AssignmentDispatched), the runner hasn't been notified
    // yet → DispatchRunner.
    let s = cycle_state(CycleState::Dispatching, Topology::ThreeAgent);
    let events = vec![event(1, EventKind::Dispatch)];
    let out = predict_next_action(&s, &events);
    assert_eq!(out, NextAction::DispatchRunner);
}
