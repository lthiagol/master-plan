//! M212 / AC-03: `recommend_remediation` returns `Resend` for any
//! violation under 3-pane, `EscalateToUser` under 2-pane or 1-pane.

use mp::autopilot::role::Topology;
use mp::autopilot::verifier::{
    recommend_remediation, Remediation, RunnerReviewViolation, Violation,
};

fn sample_violation() -> Violation {
    Violation::RunnerReviewViolation(RunnerReviewViolation {
        milestone_id: "207".into(),
        pane_id: "%2".into(),
        event_seq: Some(1),
    })
}

#[test]
fn three_pane_remediation_is_resend() {
    let r = recommend_remediation(&sample_violation(), Topology::ThreeAgent);
    match r {
        Remediation::Resend { corrective_message } => {
            assert!(!corrective_message.is_empty());
            assert!(
                corrective_message.contains("runner-review-violation"),
                "corrective message must name the violation: {corrective_message}"
            );
        }
        other => panic!("expected Resend, got {other:?}"),
    }
}

#[test]
fn two_pane_remediation_is_escalate_to_user() {
    let r = recommend_remediation(&sample_violation(), Topology::TwoAgent);
    match r {
        Remediation::EscalateToUser { violation_kind } => {
            assert_eq!(violation_kind, "runner-review-violation");
        }
        other => panic!("expected EscalateToUser, got {other:?}"),
    }
}

#[test]
fn one_pane_remediation_is_escalate_to_user() {
    let r = recommend_remediation(&sample_violation(), Topology::OneAgent);
    assert!(matches!(r, Remediation::EscalateToUser { .. }));
}

#[test]
fn resend_carries_violation_kind_in_message() {
    let r = recommend_remediation(&sample_violation(), Topology::ThreeAgent);
    let kind_str = sample_violation().kind_str();
    if let Remediation::Resend { corrective_message } = r {
        assert!(
            corrective_message.contains(kind_str),
            "corrective message must reference {kind_str}: {corrective_message}"
        );
    } else {
        panic!("expected Resend");
    }
}

#[test]
fn remediation_kind_strings_are_stable() {
    // Pin the wire form so the raul autopilot tab's filter
    // remains stable.
    let r = recommend_remediation(&sample_violation(), Topology::ThreeAgent);
    assert_eq!(r.kind_str(), "resend");
    let r = recommend_remediation(&sample_violation(), Topology::TwoAgent);
    assert_eq!(r.kind_str(), "escalate-to-user");
}
