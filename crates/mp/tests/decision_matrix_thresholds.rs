//! M213 / AC-02: the decision matrix has the four documented
//! outcomes and returns the right one for representative inputs.
//!
//! Representative inputs (from the spec):
//! - clean reviewer pass at cycle 1 -> `Complete`
//! - low-count findings (≤2 low) at cycle 2 under 3-pane ->
//!   `ShipWithBacklog`
//! - high-severity or correctness findings -> `CycleNext`
//! - non-pass at the hard limit (cycle 4 in 3-pane) ->
//!   `Escalate(CycleCapExhausted)`

use mp::autopilot::cycle::{
    apply_decision_matrix, CycleDecision, CycleEscalateReason, DecisionInput, FindingSummary,
    ReviewerVerdict,
};
use mp::autopilot::role::{topology_policy, Topology};

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
fn clean_reviewer_pass_returns_complete() {
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::Complete);
}

#[test]
fn low_count_findings_return_ship_with_backlog_three_pane_cycle_2() {
    // The spec example: `low_count<=2 + cycle=2 -> ShipWithBacklog`
    // (3-pane FullMatrix allows the path).
    let out = apply_decision_matrix(&input(
        2,
        Topology::ThreeAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(2, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::ShipWithBacklog);
}

#[test]
fn high_severity_findings_return_cycle_next() {
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 1, 0, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn correctness_findings_return_cycle_next() {
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 0, 1, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn hard_limit_escalates_even_on_pass() {
    // The spec example: `cycle=4 -> Escalate`.
    let out = apply_decision_matrix(&input(
        4,
        Topology::ThreeAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    ));
    match out {
        CycleDecision::Escalate {
            reason: CycleEscalateReason::CycleCapExhausted { cycle, budget, .. },
        } => {
            assert_eq!(cycle, 4);
            assert_eq!(budget, 4);
        }
        other => panic!("expected CycleCapExhausted, got {other:?}"),
    }
}

#[test]
fn high_severity_returns_cycle_next_even_at_high_cycle() {
    // High severity at cycle 3 under 3-pane: still CycleNext,
    // not Escalate (the cap fires only on Pass-with-no-progress
    // or the cycle counter reaching the budget).
    let out = apply_decision_matrix(&input(
        3,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 2, 0, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn decision_kinds_are_distinct_wire_forms() {
    // Pin the wire form so a future rename is a deliberate test
    // edit (and the four outcomes stay distinguishable to
    // consumers — TUI / verifier).
    assert_eq!(CycleDecision::Complete.kind_str(), "complete");
    assert_eq!(CycleDecision::CycleNext.kind_str(), "cycle-next");
    assert_eq!(
        CycleDecision::ShipWithBacklog.kind_str(),
        "ship-with-backlog"
    );
    let esc = CycleDecision::Escalate {
        reason: CycleEscalateReason::CycleCapExhausted {
            cycle: 4,
            budget: 4,
            verdict_history: vec!["pass".into()],
        },
    };
    assert_eq!(esc.kind_str(), "escalate");
}

#[test]
fn terminal_decisions_are_terminal() {
    // Complete / ShipWithBacklog / Escalate are terminal —
    // the cycle engine stops dispatching for any of them.
    let d1 = input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    );
    assert!(apply_decision_matrix(&d1).is_terminal());

    let d2 = input(
        2,
        Topology::ThreeAgent,
        ReviewerVerdict::PassWithBacklog,
        findings(2, 0, 0, 0),
    );
    assert!(apply_decision_matrix(&d2).is_terminal());

    let d3 = input(
        4,
        Topology::ThreeAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    );
    assert!(apply_decision_matrix(&d3).is_terminal());

    // CycleNext is NOT terminal.
    let d4 = input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 1, 0, 0),
    );
    assert!(!apply_decision_matrix(&d4).is_terminal());
}
