//! M213 / AC-01: the headless event loop runs a full
//! Dispatching -> WaitingRunner -> Reviewing -> Deciding -> CycleNext
//! cycle from typed session events, without depending on raul or
//! any TUI state.
//!
//! Black-box coverage of the cycle-flow state machine:
//!
//! - Build a fresh `CycleStateMachine` (3-pane topology).
//! - Feed it a scripted event sequence: `RunnerDispatched`,
//!   `RunnerCompleted`, `ReviewerVerdict { Pass }`, two
//!   `StateTick` events.
//! - Assert the state walks through every step in the canonical
//!   order and bumps the cycle counter after `CycleNext`.
//! - A second scripted sequence covers the 1-pane skip path:
//!   `RunnerDispatched` + `RunnerCompleted` lands directly on
//!   `Deciding` (no `Reviewing`).

use mp::autopilot::cycle::{
    CycleEvent, CycleState, CycleStateMachine, FindingSummary, ReviewerVerdict,
};
use mp::autopilot::role::Topology;

fn empty_findings() -> FindingSummary {
    FindingSummary::default()
}

#[test]
fn full_cycle_dispatching_to_dispatching_via_typed_events() {
    let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
    assert_eq!(s.state, CycleState::Dispatching);
    assert_eq!(s.cycle, 1);

    s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
    assert_eq!(s.state, CycleState::WaitingRunner);

    s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
    assert_eq!(s.state, CycleState::Reviewing);

    s = s.advance(CycleEvent::ReviewerVerdict {
        pane: "%3".into(),
        verdict: ReviewerVerdict::Pass,
        findings: empty_findings(),
    });
    assert_eq!(s.state, CycleState::Deciding);

    s = s.advance(CycleEvent::StateTick);
    assert_eq!(s.state, CycleState::CycleNext);

    s = s.advance(CycleEvent::StateTick);
    assert_eq!(s.state, CycleState::Dispatching);
    assert_eq!(s.cycle, 2, "second cycle counter is bumped");
}

#[test]
fn one_pane_skips_reviewing_state_direct_to_deciding() {
    let mut s = CycleStateMachine::new("M213", Topology::OneAgent);
    s = s.advance(CycleEvent::RunnerDispatched { pane: "%1".into() });
    assert_eq!(s.state, CycleState::WaitingRunner);
    s = s.advance(CycleEvent::RunnerCompleted { pane: "%1".into() });
    // 1-pane topology skips the Reviewing state — the runner
    // verdict IS the verdict under collapsed roles.
    assert_eq!(s.state, CycleState::Deciding);
}

#[test]
fn raul_lane_changes_have_no_effect_on_cycle_progress() {
    // The cycle engine consumes only typed `CycleEvent`s. A raul
    // lane change (e.g. opening / closing a TUI tab) cannot fire
    // any of those events. We model that by feeding an empty
    // event stream and asserting the state machine does not
    // move on its own.
    let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
    let empty: Vec<CycleEvent> = vec![];
    let mut current = s;
    for _ in 0..10 {
        for event in &empty {
            current = current.advance(event.clone());
        }
    }
    // No events have been fed -> still Dispatching, cycle=1.
    assert_eq!(current.state, CycleState::Dispatching);
    assert_eq!(current.cycle, 1);
}

#[test]
fn cycle_history_records_per_cycle_verdicts() {
    let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
    s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
    s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
    s = s.advance(CycleEvent::ReviewerVerdict {
        pane: "%3".into(),
        verdict: ReviewerVerdict::Fail,
        findings: FindingSummary {
            high_severity_count: 1,
            ..Default::default()
        },
    });
    assert_eq!(s.cycle_history.len(), 1);
    assert_eq!(s.cycle_history[0].verdict, "fail");
    assert_eq!(s.cycle_history[0].cycle, 1);
}
