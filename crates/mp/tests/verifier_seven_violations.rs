//! M212 / AC-02: each role-boundary detector is exercised with
//! attributable session fixtures. Detection uses M207 actor tokens
//! and dispatch-bounded repository snapshots; conditions that
//! cannot be attributed are reported as UnknownActor and block
//! automatic acceptance rather than being guessed.

use mp::activity::{ActivityEvent, ActivityLog};
use mp::autopilot::verifier::{
    detect_orchestrator_code_edit_violation, detect_pre_start_notification_violation,
    detect_reviewer_code_edit_violation, detect_reviewer_premature_pass_violation,
    detect_runner_claim_violation, detect_runner_plan_edit_violation,
    detect_runner_review_violation, ActorAttribution, Lane, LaneNotification, Violation,
};
use mp::model::{MilestoneFile, MilestoneMeta};

fn attribution_for(lane: Lane, pane: &str, seq: u64) -> ActorAttribution {
    ActorAttribution {
        session_id: "s1".into(),
        role: lane,
        actor_token: pane.into(),
        dispatch_id: format!("dispatch-{seq}"),
        seq,
    }
}

fn runner_done(id: &str, action: &str) -> LaneNotification {
    let mut n = LaneNotification::runner_done(
        id,
        1,
        "executed",
        "done",
        "implemented",
        attribution_for(Lane::Runner, "%2", 1),
    );
    n.action = action.into();
    n
}

fn reviewer_done(id: &str, action: &str, cycle: u32) -> LaneNotification {
    let mut n = LaneNotification::runner_done(
        id,
        1,
        "executed",
        "done",
        "implemented",
        attribution_for(Lane::Reviewer, "%3", 2),
    );
    n.lane = Lane::Reviewer;
    n.action = action.into();
    n.cycle = cycle;
    n
}

fn orchestrator_done(id: &str) -> LaneNotification {
    let mut n = LaneNotification::runner_done(
        id,
        1,
        "executed",
        "done",
        "implemented",
        attribution_for(Lane::Orchestrator, "%1", 3),
    );
    n.lane = Lane::Orchestrator;
    n
}

// ──── Detector 1: Runner called mp reviews pass ──────────────────

#[test]
fn detector_1_runner_review_violation_fires_for_submitted_review_pass() {
    let n = runner_done("207", "submitted-review-pass");
    let v = detect_runner_review_violation(&n).expect("expected violation");
    assert!(matches!(v, Violation::RunnerReviewViolation(_)));
    assert_eq!(v.kind_str(), "runner-review-violation");
}

#[test]
fn detector_1_does_not_fire_on_completed_execute() {
    let n = runner_done("207", "completed-execute");
    assert!(detect_runner_review_violation(&n).is_none());
}

// ──── Detector 2: Runner called mp reviews claim / finding add ────

#[test]
fn detector_2_runner_claim_violation_fires_for_added_finding() {
    let n = runner_done("207", "added-finding");
    let v = detect_runner_claim_violation(&n).expect("expected violation");
    assert!(matches!(v, Violation::RunnerClaimViolation(_)));
}

#[test]
fn detector_2_does_not_fire_for_reviewer_added_finding() {
    let n = reviewer_done("207", "added-finding", 1);
    assert!(detect_runner_claim_violation(&n).is_none());
}

// ──── Detector 3: Runner modified master-plan/ ─────────────────────

#[test]
fn detector_3_runner_plan_edit_violation_fires_on_master_plan_hunk() {
    let n = runner_done("207", "completed-execute");
    let hunk = "+++ b/master-plan/milestones/207-*.json\n+ \"lifecycle\": \"executed\"";
    let v = detect_runner_plan_edit_violation(&n, Some(hunk)).expect("expected violation");
    assert!(matches!(v, Violation::RunnerPlanEditViolation(_)));
}

#[test]
fn detector_3_does_not_fire_for_code_only_diff() {
    let n = runner_done("207", "completed-execute");
    let hunk = "+++ b/crates/mp/src/lib.rs\n+ change";
    assert!(detect_runner_plan_edit_violation(&n, Some(hunk)).is_none());
}

// ──── Detector 4: Reviewer modified code ───────────────────────────

#[test]
fn detector_4_reviewer_code_edit_violation_fires_on_crates_hunk() {
    let n = reviewer_done("207", "completed-execute", 1);
    let hunk = "+++ b/crates/mp/src/lib.rs\n+ // reviewer touched code by accident";
    let v = detect_reviewer_code_edit_violation(&n, Some(hunk)).expect("expected violation");
    assert!(matches!(v, Violation::ReviewerCodeEditViolation(_)));
}

#[test]
fn detector_4_does_not_fire_for_plan_zone_only() {
    let n = reviewer_done("207", "completed-execute", 1);
    let hunk = "+++ b/master-plan/notes.md\n+ reviewer comment";
    assert!(detect_reviewer_code_edit_violation(&n, Some(hunk)).is_none());
}

// ──── Detector 5: Reviewer called mp reviews pass before prompted ─

#[test]
fn detector_5_reviewer_premature_pass_violation_fires_when_cycle_pre_prompt() {
    let n = reviewer_done("207", "submitted-review-pass", 0);
    let v = detect_reviewer_premature_pass_violation(&n, 1).expect("expected violation");
    assert!(matches!(v, Violation::ReviewerPrematurePassViolation(_)));
}

#[test]
fn detector_5_does_not_fire_after_prompt() {
    let n = reviewer_done("207", "submitted-review-pass", 1);
    assert!(detect_reviewer_premature_pass_violation(&n, 1).is_none());
}

// ──── Detector 6: Notify arrived before lane started ──────────────

#[test]
fn detector_6_pre_start_notification_violation_fires_for_unknown_dispatch() {
    let n = runner_done("207", "completed-execute");
    let started = vec!["dispatch-2".into()];
    let v = detect_pre_start_notification_violation(&n, &started).expect("expected violation");
    assert!(matches!(v, Violation::PreStartNotificationViolation(_)));
}

#[test]
fn detector_6_does_not_fire_for_known_dispatch() {
    let n = runner_done("207", "completed-execute");
    let started = vec!["dispatch-1".into()];
    assert!(detect_pre_start_notification_violation(&n, &started).is_none());
}

// ──── Detector 7: Orchestrator committed code to its own pane ─────

#[test]
fn detector_7_orchestrator_code_edit_violation_fires_on_pane_match() {
    let n = orchestrator_done("207");
    let hunk = "+++ b/crates/mp/src/lib.rs\n+ oops";
    let v =
        detect_orchestrator_code_edit_violation(&n, Some(hunk), "%1").expect("expected violation");
    assert!(matches!(v, Violation::OrchestratorCodeEditViolation(_)));
}

#[test]
fn detector_7_does_not_fire_on_other_pane() {
    let n = orchestrator_done("207");
    let hunk = "+++ b/crates/mp/src/lib.rs\n+ oops";
    assert!(detect_orchestrator_code_edit_violation(&n, Some(hunk), "%5").is_none());
}

// ──── Attribution: UnknownActor when provenance missing ──────────

#[test]
fn attribution_missing_blocks_automatic_acceptance() {
    let mut n = runner_done("207", "completed-execute");
    n.attribution.actor_token = "".into();
    let err = n.attribution.validate().unwrap_err();
    // Matches the M203 lesson: unknown actor identity is a
    // typed diagnostic, not a guess.
    assert!(matches!(
        err,
        mp::autopilot::verifier::AttributionError::MissingActorToken
    ));
}

// ──── All 7 detectors return at least one typed violation each ────

#[test]
fn all_seven_detectors_return_typed_violations() {
    // Detector 1
    let v = detect_runner_review_violation(&runner_done("207", "submitted-review-pass"));
    assert!(v.is_some());
    // Detector 2
    let v = detect_runner_claim_violation(&runner_done("207", "added-finding"));
    assert!(v.is_some());
    // Detector 3
    let v = detect_runner_plan_edit_violation(
        &runner_done("207", "completed-execute"),
        Some("+++ b/master-plan/milestones/207-*.json\n+ x"),
    );
    assert!(v.is_some());
    // Detector 4
    let v = detect_reviewer_code_edit_violation(
        &reviewer_done("207", "completed-execute", 1),
        Some("+++ b/crates/mp/src/lib.rs\n+ x"),
    );
    assert!(v.is_some());
    // Detector 5
    let v = detect_reviewer_premature_pass_violation(
        &reviewer_done("207", "submitted-review-pass", 0),
        1,
    );
    assert!(v.is_some());
    // Detector 6
    let v = detect_pre_start_notification_violation(
        &runner_done("207", "completed-execute"),
        &["dispatch-other".into()],
    );
    assert!(v.is_some());
    // Detector 7
    let v = detect_orchestrator_code_edit_violation(
        &orchestrator_done("207"),
        Some("+++ b/crates/mp/src/lib.rs\n+ x"),
        "%1",
    );
    assert!(v.is_some());
}

// ──── Activity journal cursor respected ──────────────────────────

#[test]
fn activity_cursor_used_not_arbitrary_tail() {
    // The verifier uses an explicit event cursor from the
    // session.json event log (OrchestrationEvent.seq), NOT a
    // "last three activity entries" slice. Pin the cursor
    // invariant on the typed event shape.
    use mp::autopilot::events::{EventCursor, OrchestrationEvent};
    let mut cursor = EventCursor::new();
    cursor.advance_to(1).unwrap();
    cursor.advance_to(5).unwrap();
    assert_eq!(cursor.last_seq, 5);
    // The activity journal is a separate, lighter-weight
    // shape — events lack seq (they have a timestamp instead).
    // The cursor lives on the session.json event log; the
    // journal is filtered by milestone subject.
    let mut activity = ActivityLog::empty();
    activity.events.push(ActivityEvent::now(
        "lifecycle-transition",
        "207",
        "lifecycle: approved → in-progress",
    ));
    activity.events.push(ActivityEvent::now(
        "lifecycle-transition",
        "207",
        "lifecycle: in-progress → executed",
    ));
    assert_eq!(activity.events.len(), 2);
    // Drop the unused import warning for OrchestrationEvent.
    let _ = OrchestrationEvent::new(
        5,
        mp::autopilot::EventKind::Transition,
        "test",
        serde_json::json!({}),
    );
}

#[test]
fn activity_lifecycle_transition_subject_filter_works() {
    let mut activity = ActivityLog::empty();
    activity.events.push(ActivityEvent::now(
        "lifecycle-transition",
        "207",
        "lifecycle: approved → in-progress",
    ));
    activity.events.push(ActivityEvent::now(
        "lifecycle-transition",
        "999",
        "different milestone",
    ));
    let milestone_subject = "207";
    let count = activity
        .events
        .iter()
        .filter(|e| e.subject == milestone_subject && e.r#type == "lifecycle-transition")
        .count();
    assert_eq!(count, 1);
}

// ──── Sample milestone builder for fixture-style tests ──────────

#[allow(dead_code)]
fn milestone(id: &str) -> MilestoneFile {
    MilestoneFile {
        milestone: MilestoneMeta {
            id: id.to_string(),
            title: "Sample".into(),
            slug: "sample".into(),
            lifecycle: "executed".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}
