//! M212 / AC-04: regression test for M201 cycle 1 — a fabricated
//! `lifecycle=executed` notification with no matching activity.json
//! event and a milestone at `lifecycle=approved` is rejected by
//! the verifier as a typed mismatch.

use mp::activity::ActivityLog;
use mp::autopilot::verifier::{
    check_notification, ActorAttribution, Lane, LaneNotification, Verdict, VerifierInputs,
    VerifierState, Violation,
};
use mp::model::{MilestoneFile, MilestoneMeta};

fn attribution() -> ActorAttribution {
    ActorAttribution {
        session_id: "s1".into(),
        role: Lane::Runner,
        actor_token: "%2".into(),
        dispatch_id: "dispatch-1".into(),
        seq: 1,
    }
}

fn m201_fabricated_state() -> VerifierState {
    let mut m = MilestoneFile::default();
    m.milestone = MilestoneMeta {
        id: "201".into(),
        title: "M201 fixture".into(),
        slug: "m201-fixture".into(),
        lifecycle: "approved".into(),
        ..Default::default()
    };
    VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(), // No lifecycle-transition event
        milestone_path: "201.json".into(),
    }
}

#[test]
fn m201_fabricated_lifecycle_executed_is_rejected_as_typed_mismatch() {
    let state = m201_fabricated_state();
    // The runner (M201's M201 cycle 1 pattern) claimed the
    // milestone reached lifecycle=executed without producing
    // any matching activity event.
    let notification = LaneNotification::runner_done(
        "201",
        1,
        "executed", // fabricated
        "done",
        "implemented",
        attribution(),
    );
    let verdict = check_notification(
        &state,
        &notification,
        VerifierInputs {
            diff_hunk: None,
            orchestrator_prompted_cycle: 1,
            started_dispatch_ids: &["dispatch-1".into()],
            orchestrator_pane_id: "%1",
        },
    );
    match verdict {
        Verdict::Reject { violations, .. } => {
            // The LifecycleClaimUnbacked detector must fire — that
            // is the typed mismatch the orchestrator surfaces.
            assert!(
                violations
                    .iter()
                    .any(|v| matches!(v, Violation::LifecycleClaimUnbacked(_))),
                "expected LifecycleClaimUnbacked, got {violations:?}"
            );
        }
        Verdict::Accept => panic!("verifier accepted a fabricated lifecycle claim"),
        other => panic!("expected Reject, got {other:?}"),
    }
}

#[test]
fn m201_regression_passes_when_activity_event_present() {
    // If the runner had produced a real lifecycle-transition
    // event, the verifier accepts. The LifecycleClaimUnbacked
    // detector specifically requires NO event AND canonical
    // milestone at lifecycle=approved.
    let mut state = m201_fabricated_state();
    state.activity.events.push(mp::activity::ActivityEvent::now(
        "lifecycle-transition",
        "201",
        "lifecycle: approved → in-progress",
    ));
    let notification = LaneNotification::runner_done(
        "201",
        1,
        "executed",
        "done",
        "implemented",
        attribution(),
    );
    let verdict = check_notification(
        &state,
        &notification,
        VerifierInputs {
            diff_hunk: None,
            orchestrator_prompted_cycle: 1,
            started_dispatch_ids: &["dispatch-1".into()],
            orchestrator_pane_id: "%1",
        },
    );
    // No LifecycleClaimUnbacked — the activity event exists.
    match verdict {
        Verdict::Reject { violations, .. } => {
            assert!(
                !violations
                    .iter()
                    .any(|v| matches!(v, Violation::LifecycleClaimUnbacked(_))),
                "LifecycleClaimUnbacked must NOT fire when an event is present"
            );
        }
        Verdict::Accept => {}
        other => panic!("expected Reject without LifecycleClaimUnbacked, got {other:?}"),
    }
}

#[test]
fn m201_regression_does_not_fire_when_canonical_lifecycle_already_executed() {
    // The detector's contract: fires only when canonical=approved
    // AND no event. If the canonical is already at "executed",
    // the notification agrees with the milestone JSON — no
    // unbacked claim.
    let mut state = m201_fabricated_state();
    state.milestone.milestone.lifecycle = "executed".into();
    state.activity.events.push(mp::activity::ActivityEvent::now(
        "lifecycle-transition",
        "201",
        "lifecycle: approved → executed",
    ));
    let notification = LaneNotification::runner_done(
        "201",
        1,
        "executed",
        "done",
        "implemented",
        attribution(),
    );
    let verdict = check_notification(
        &state,
        &notification,
        VerifierInputs {
            diff_hunk: None,
            orchestrator_prompted_cycle: 1,
            started_dispatch_ids: &["dispatch-1".into()],
            orchestrator_pane_id: "%1",
        },
    );
    assert!(
        !matches!(verdict, Verdict::Reject { ref violations, .. } if violations.iter().any(|v| matches!(v, Violation::LifecycleClaimUnbacked(_)))),
        "LifecycleClaimUnbacked must NOT fire when canonical lifecycle already matches"
    );
}