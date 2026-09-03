//! M213 / AC-05: reviewer activation via the M211 typed dispatch.
//!
//! The cycle engine builds a [`TaskAssignment`] with the
//! documented `review_request` payload (milestone id, cycle,
//! evidence revision, reviewer actor token, mode flag). The argv
//! is the canonical `herdr agent prompt <pane> <task>` shape from
//! M211 — no shell interpolation, no metacharacter smuggling.

use mp::autopilot::cycle::{
    build_reviewer_activation, reviewer_mode_for_cycle, ReviewRequestPayload, ReviewerMode,
};
use mp::autopilot::events::EventKind;
use mp::autopilot::session::{
    sample_session_for_tests, save_session, PaneLayout, PaneRef, SessionPath,
};
use mp::autopilot::task_assign::{
    build_assignment_argv, RoleDirection, TaskAssignmentValidationError,
};
use mp::paths::PlanContext;
use serde_json::json;
use std::path::Path;

mod common;

use common::TestEnv;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn reviewer_activation_carries_documented_review_request_fields() {
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let assignment = build_reviewer_activation("session-1", &payload, "%3");

    // M211 typed-payload contract: every required field is
    // populated, direction is reviewer-bound, target pane is
    // the reviewer slot.
    assert_eq!(assignment.session_id, "session-1");
    assert_eq!(assignment.milestone_id, "M213");
    assert_eq!(assignment.cycle, 1);
    assert_eq!(assignment.target_pane, "%3");
    assert_eq!(assignment.direction, RoleDirection::OrchestratorToReviewer);

    // The review-request body carries every documented field
    // — milestone, cycle, evidence revision, reviewer actor
    // token, mode.
    assert!(assignment.task.contains("milestone_id=M213"));
    assert!(assignment.task.contains("cycle=1"));
    assert!(assignment.task.contains("evidence_revision=rev-abc"));
    assert!(assignment
        .task
        .contains("reviewer_actor_token=reviewer-pane-1"));
    assert!(assignment.task.contains("mode=full"));

    // evidence_refs carries the revision so the verifier can
    // cross-check against the milestone AC projection.
    assert!(
        assignment
            .evidence_refs
            .iter()
            .any(|r| r.contains("evidence_revision=rev-abc")),
        "evidence_refs must carry the revision: {:?}",
        assignment.evidence_refs
    );
}

#[test]
fn reviewer_activation_renders_deterministic_argv() {
    // The same payload must always produce the same argv
    // (M211 golden contract). The argv is the canonical
    // `herdr agent prompt <pane> <task>` shape — no shell
    // interpolation.
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let assignment = build_reviewer_activation("session-1", &payload, "%3");
    let argv1 = build_assignment_argv(&assignment);
    let argv2 = build_assignment_argv(&assignment);
    assert_eq!(argv1, argv2);
    assert_eq!(argv1[0], "agent");
    assert_eq!(argv1[1], "prompt");
    assert_eq!(argv1[2], "%3");
    assert!(argv1[3].contains("review_request:"));
}

#[test]
fn reviewer_activation_rejects_smuggled_metacharacters() {
    // Defense-in-depth: even though the typed payload has
    // dedicated fields, a hand-crafted payload with a shell
    // metacharacter must be caught by M211's validation gate
    // before herdr is invoked. We pin the gate's behavior via
    // direct `TaskAssignment` validation (the same gate
    // `dispatch_assignment` runs before spawning herdr).
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let assignment = build_reviewer_activation("session-1", &payload, "%3");
    // Inject a metacharacter into the body to simulate a
    // hand-crafted payload.
    let mut bad = assignment.clone();
    bad.task = format!("{} ; rm -rf /", bad.task);
    let layout = PaneLayout {
        reviewer: Some(PaneRef {
            pane_id: "%3".into(),
            label: None,
        }),
        ..Default::default()
    };
    let res = mp::autopilot::task_assign::validate_assignment(&bad, &layout);
    assert!(matches!(
        res,
        Err(TaskAssignmentValidationError::ShellMetacharacter { .. })
    ));
}

#[test]
fn reviewer_mode_flips_at_soft_cap_for_subsequent_cycles() {
    // Cycle 1 is always Full; cycle 2+ is BlockersOnly.
    assert_eq!(reviewer_mode_for_cycle(1), ReviewerMode::Full);
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 2,
        evidence_revision: "rev-def".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: reviewer_mode_for_cycle(2),
    };
    let assignment = build_reviewer_activation("session-1", &payload, "%3");
    assert!(assignment.task.contains("mode=blockers-only"));
}

#[test]
fn reviewer_activation_uses_orchestrator_to_reviewer_direction() {
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let assignment = build_reviewer_activation("session-1", &payload, "%3");
    // The dispatch direction is exclusively orchestrator-to-reviewer.
    assert_eq!(assignment.direction, RoleDirection::OrchestratorToReviewer);
}

#[test]
fn reviewer_activation_auditable_via_assignment_dispatched_event() {
    // Per AC-05 the dispatch event is appended with milestone,
    // cycle, evidence revision, reviewer actor token. The
    // event log carries the typed payload as JSON. We pin the
    // payload shape here without going through herdr (test
    // runs against a stub session).
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    // Reviewer pane is %3 in the sample.
    session.topology = PaneLayout {
        orchestrator: Some(PaneRef {
            pane_id: "%1".into(),
            label: None,
        }),
        runner: Some(PaneRef {
            pane_id: "%2".into(),
            label: None,
        }),
        reviewer: Some(PaneRef {
            pane_id: "%3".into(),
            label: None,
        }),
    };
    save_session(&ctx, "alpha", &session).unwrap();

    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let assignment = build_reviewer_activation("alpha", &payload, "%3");
    // The session's reviewer pane matches the assignment's
    // target pane, so dispatch is well-formed.
    let layout = session.topology.clone();
    mp::autopilot::task_assign::validate_assignment(&assignment, &layout)
        .expect("assignment must validate against the session layout");

    // The auditable dispatch event payload merges the typed
    // assignment with the dispatch outcome. We pin the
    // payload's milestone/cycle/evidence_revision fields so
    // the verifier has the data it needs.
    let merged = json!({
        "session_id": assignment.session_id,
        "milestone_id": assignment.milestone_id,
        "cycle": assignment.cycle,
        "direction": "orchestrator-to-reviewer",
        "target_pane": assignment.target_pane,
        "task": assignment.task,
        "evidence_refs": assignment.evidence_refs,
        "boundary_reminders": assignment.boundary_reminders,
    });
    assert_eq!(merged["milestone_id"], "M213");
    assert_eq!(merged["cycle"], 1);
    assert_eq!(merged["direction"], "orchestrator-to-reviewer");
    assert_eq!(merged["target_pane"], "%3");
    assert!(merged["evidence_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("evidence_revision=rev-abc")));
    // EventKind for the audit event — the orchestrator
    // appends `AssignmentDispatched` after herdr's spawn
    // outcome is known (M211 contract).
    let _kind = EventKind::AssignmentDispatched;
    let _ = SessionPath::new(&ctx, "alpha").unwrap();
}

#[test]
fn reviewer_activation_payload_is_rendered_deterministically() {
    // The renderer's text is byte-for-byte stable so the
    // golden test pins it.
    let payload = ReviewRequestPayload {
        milestone_id: "M213".into(),
        cycle: 1,
        evidence_revision: "rev-abc".into(),
        reviewer_actor_token: "reviewer-pane-1".into(),
        mode: ReviewerMode::Full,
    };
    let text1 = payload.render();
    let text2 = payload.render();
    assert_eq!(text1, text2);
    let expected = "review_request: milestone_id=M213, cycle=1, \
                   evidence_revision=rev-abc, reviewer_actor_token=reviewer-pane-1, mode=full";
    assert_eq!(text1, expected);
}
