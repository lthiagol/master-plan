//! M213 / AC-03: topology-mode tightening of the decision matrix
//! and the state machine.
//!
//! - 3-pane: full matrix is available (Complete / CycleNext /
//!   ShipWithBacklog / Escalate); four-cycle budget.
//! - 2-pane: forbids ShipWithBacklog — low-severity findings
//!   force CycleNext; three-cycle budget.
//! - 1-pane: NoExternalReview — the Reviewing state is skipped
//!   in the cycle state machine; two-cycle budget; only tracks
//!   are accepted.

use mp::autopilot::cycle::{
    apply_decision_matrix, CycleDecision, CycleEvent, CycleState, CycleStateMachine, DecisionInput,
    FindingSummary, ReviewerVerdict,
};
use mp::autopilot::role::{topology_policy, Topology, TopologyMode};

fn findings(low: u32, high: u32, correctness: u32, blocker: u32) -> FindingSummary {
    FindingSummary {
        high_severity_count: high,
        correctness_count: correctness,
        low_count: low,
        blocker_count: blocker,
    }
}

fn input(
    cycle: u32,
    topology: Topology,
    verdict: ReviewerVerdict,
    f: FindingSummary,
) -> DecisionInput {
    let policy = topology_policy(topology);
    DecisionInput {
        cycle,
        topology,
        policy,
        verdict,
        findings: f,
        cycle_history: Vec::new(),
    }
}

#[test]
fn three_pane_uses_full_matrix_and_four_cycle_budget() {
    let policy = topology_policy(Topology::ThreeAgent);
    assert_eq!(policy.mode, TopologyMode::FullMatrix);
    assert_eq!(policy.cycle_budget, 4);
    assert!(policy.allows_ship_with_backlog());
    // Full matrix: low-severity PassWithBacklog at cycle 1 →
    // ShipWithBacklog.
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(2, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::ShipWithBacklog);
}

#[test]
fn two_pane_forbids_ship_with_backlog_and_uses_three_cycles() {
    let policy = topology_policy(Topology::TwoAgent);
    assert_eq!(policy.mode, TopologyMode::NoShipWithBacklog);
    assert_eq!(policy.cycle_budget, 3);
    assert!(!policy.allows_ship_with_backlog());
    // Low-severity PassWithBacklog at cycle 1 → CycleNext
    // (NOT ShipWithBacklog — the topology forbids the path).
    let out = apply_decision_matrix(&input(
        1,
        Topology::TwoAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(2, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn one_pane_is_accepted_only_for_trivial_with_two_cycles() {
    let policy = topology_policy(Topology::OneAgent);
    assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
    assert_eq!(policy.cycle_budget, 2);
    assert!(!policy.allows_ship_with_backlog());
}

#[test]
fn one_pane_state_machine_skips_reviewing_state() {
    // NoExternalReview: 1-pane path is RunnerCompleted →
    // Deciding (skipping Reviewing).
    let mut s = CycleStateMachine::new("M213", Topology::OneAgent);
    s = s.advance(CycleEvent::RunnerDispatched { pane: "%1".into() });
    assert_eq!(s.state, CycleState::WaitingRunner);
    s = s.advance(CycleEvent::RunnerCompleted { pane: "%1".into() });
    assert_eq!(
        s.state,
        CycleState::Deciding,
        "1-pane must skip Reviewing (no external reviewer)"
    );
}

#[test]
fn two_pane_state_machine_keeps_reviewing_state() {
    // Reviewing still exists under 2-pane — orchestrator +
    // reviewer share a supervisor pane but the channel is
    // usable (just not independent).
    let mut s = CycleStateMachine::new("M213", Topology::TwoAgent);
    s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
    s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
    assert_eq!(s.state, CycleState::Reviewing);
}

#[test]
fn three_pane_state_machine_keeps_reviewing_state() {
    let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
    s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
    s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
    assert_eq!(s.state, CycleState::Reviewing);
}

#[test]
fn cycle_budget_decreases_with_topology() {
    // Tightening is monotonic: 3-pane (4) > 2-pane (3) > 1-pane (2).
    let three = topology_policy(Topology::ThreeAgent).cycle_budget;
    let two = topology_policy(Topology::TwoAgent).cycle_budget;
    let one = topology_policy(Topology::OneAgent).cycle_budget;
    assert!(three > two);
    assert!(two > one);
}

#[test]
fn two_pane_ship_with_backlog_downgrades_to_cycle_next_under_cap() {
    // 2-pane + cycle 2 (under the 3-cycle budget) +
    // PassWithBacklog → CycleNext (NOT Escalate, NOT
    // ShipWithBacklog).
    let out = apply_decision_matrix(&input(
        2,
        Topology::TwoAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(1, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn two_pane_ship_with_backlog_at_cap_escalates_instead() {
    // 2-pane + cycle 3 (cap reached) + PassWithBacklog →
    // Escalate (cap wins over the topology-downgrade rule).
    let out = apply_decision_matrix(&input(
        3,
        Topology::TwoAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(1, 0, 0, 0),
    ));
    assert!(matches!(out, CycleDecision::Escalate { .. }));
}

#[test]
fn one_pane_ship_with_backlog_is_always_cycle_next_or_escalate() {
    // 1-pane has no ShipWithBacklog path. At cycle 1 (under
    // the 2-cycle budget) → CycleNext. At cycle 2 (cap) →
    // Escalate.
    let under = apply_decision_matrix(&input(
        1,
        Topology::OneAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(0, 0, 0, 0),
    ));
    assert_eq!(under, CycleDecision::CycleNext);

    let at_cap = apply_decision_matrix(&input(
        2,
        Topology::OneAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(0, 0, 0, 0),
    ));
    assert!(matches!(at_cap, CycleDecision::Escalate { .. }));
}
