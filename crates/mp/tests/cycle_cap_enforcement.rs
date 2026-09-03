//! M213 / AC-04: cycle-cap enforcement.
//!
//! - A clean pass on cycle 4 (the 3-pane hard limit) is
//!   `Complete`-class only if the cap has not been reached.
//!   The cap turns it into `Escalate(CycleCapExhausted)`. The
//!   same rule applies to 2-pane (cap=3) and 1-pane (cap=2).
//! - Non-pass verdicts at the cap also escalate (the cap
//!   wins over CycleNext).
//! - The soft cap (`REVIEWER_SOFT_CAP_CYCLE = 2`) flips the
//!   reviewer mode to `BlockersOnly` from cycle 2 onward.

use mp::autopilot::cycle::{
    apply_decision_matrix, reviewer_mode_for_cycle, CycleDecision, CycleEscalateReason,
    DecisionInput, FindingSummary, ReviewerMode, ReviewerVerdict, REVIEWER_SOFT_CAP_CYCLE,
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
fn clean_pass_three_pane_before_cap_returns_complete() {
    // cycle 1 + Pass → Complete (under cap).
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    ));
    assert_eq!(out, CycleDecision::Complete);
}

#[test]
fn clean_pass_three_pane_at_cap_escalates() {
    // cycle=4 (cap) + Pass → Escalate(CycleCapExhausted).
    // Even a clean pass escalates at the cap — anti-thrash
    // guarantee (AC-04).
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
        other => panic!("expected CycleCapExhausted at cap, got {other:?}"),
    }
}

#[test]
fn synthetic_four_cycle_milestone_with_all_passes_reaches_escalate() {
    // Walk the matrix through cycles 1..=4 with clean Pass
    // verdicts. Cycles 1-3 are Complete, cycle 4 escalates.
    let mut decisions = Vec::new();
    for cycle in 1..=4 {
        let history: Vec<_> = (1..cycle)
            .map(|c| {
                serde_json::to_string(&serde_json::json!({"cycle": c, "verdict": "pass"}))
                    .unwrap_or_default()
            })
            .collect();
        let input = DecisionInput {
            cycle,
            topology: Topology::ThreeAgent,
            policy: topology_policy(Topology::ThreeAgent),
            verdict: ReviewerVerdict::Pass,
            findings: findings(0, 0, 0, 0),
            cycle_history: (1..cycle)
                .map(|c| mp::autopilot::cycle::CycleVerdictRecord {
                    cycle: c,
                    verdict: "pass".into(),
                    findings: findings(0, 0, 0, 0),
                    topology_mode: "full_matrix".into(),
                })
                .collect(),
        };
        decisions.push(apply_decision_matrix(&input));
        let _ = history; // suppress unused warning
    }
    assert_eq!(
        decisions[0],
        CycleDecision::Complete,
        "cycle 1 Pass → Complete"
    );
    assert_eq!(
        decisions[1],
        CycleDecision::Complete,
        "cycle 2 Pass → Complete"
    );
    assert_eq!(
        decisions[2],
        CycleDecision::Complete,
        "cycle 3 Pass → Complete"
    );
    assert!(
        matches!(decisions[3], CycleDecision::Escalate { .. }),
        "cycle 4 (cap) Pass → Escalate (anti-thrash guarantee)"
    );
}

#[test]
fn two_pane_cap_at_three_escalates() {
    let out = apply_decision_matrix(&input(
        3,
        Topology::TwoAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    ));
    assert!(matches!(out, CycleDecision::Escalate { .. }));
}

#[test]
fn one_pane_cap_at_two_escalates() {
    let out = apply_decision_matrix(&input(
        2,
        Topology::OneAgent,
        ReviewerVerdict::Pass,
        findings(0, 0, 0, 0),
    ));
    assert!(matches!(out, CycleDecision::Escalate { .. }));
}

#[test]
fn cap_wins_over_cycle_next_for_non_pass_at_limit() {
    // High-severity findings at cycle 4 (3-pane cap): cap
    // wins, the matrix returns Escalate instead of CycleNext.
    let out = apply_decision_matrix(&input(
        4,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 1, 0, 0),
    ));
    assert!(matches!(out, CycleDecision::Escalate { .. }));
}

#[test]
fn cap_under_limit_returns_cycle_next_for_fail() {
    // High-severity at cycle 1 (under cap): CycleNext, not
    // Escalate.
    let out = apply_decision_matrix(&input(
        1,
        Topology::ThreeAgent,
        ReviewerVerdict::Fail,
        findings(0, 1, 0, 0),
    ));
    assert_eq!(out, CycleDecision::CycleNext);
}

#[test]
fn soft_cap_flips_reviewer_mode_to_blockers_only_at_cycle_2() {
    // The soft cap engages at cycle 2 (per spec — catches
    // drift faster than cycle 3 would).
    assert_eq!(REVIEWER_SOFT_CAP_CYCLE, 2);
    assert_eq!(reviewer_mode_for_cycle(1), ReviewerMode::Full);
    assert_eq!(reviewer_mode_for_cycle(2), ReviewerMode::BlockersOnly);
    assert_eq!(reviewer_mode_for_cycle(3), ReviewerMode::BlockersOnly);
    assert_eq!(reviewer_mode_for_cycle(4), ReviewerMode::BlockersOnly);
}

#[test]
fn verdict_history_is_carried_into_cycle_cap_escalate() {
    // The CycleCapExhausted reason carries the prior verdict
    // history so the operator can audit what happened before
    // the cap fired.
    let history = vec![
        mp::autopilot::cycle::CycleVerdictRecord {
            cycle: 1,
            verdict: "fail".into(),
            findings: findings(0, 1, 0, 0),
            topology_mode: "full_matrix".into(),
        },
        mp::autopilot::cycle::CycleVerdictRecord {
            cycle: 2,
            verdict: "fail".into(),
            findings: findings(0, 1, 0, 0),
            topology_mode: "full_matrix".into(),
        },
        mp::autopilot::cycle::CycleVerdictRecord {
            cycle: 3,
            verdict: "fail".into(),
            findings: findings(0, 1, 0, 0),
            topology_mode: "full_matrix".into(),
        },
    ];
    let input = DecisionInput {
        cycle: 4,
        topology: Topology::ThreeAgent,
        policy: topology_policy(Topology::ThreeAgent),
        verdict: ReviewerVerdict::Pass,
        findings: findings(0, 0, 0, 0),
        cycle_history: history,
    };
    let out = apply_decision_matrix(&input);
    match out {
        CycleDecision::Escalate {
            reason:
                CycleEscalateReason::CycleCapExhausted {
                    verdict_history, ..
                },
        } => {
            assert_eq!(verdict_history.len(), 3);
            assert!(verdict_history.iter().all(|v| v == "fail"));
        }
        other => panic!("expected CycleCapExhausted, got {other:?}"),
    }
}
