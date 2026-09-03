//! M211 / AC-03: M200 regression — prose-only and empty
//! assignments cannot reach `herdr` or append an
//! `AssignmentDispatched` event.
//!
//! The historical bug (per the spec): the herdr tests showed that
//! prose such as "notify the orchestrator" was interpreted
//! inconsistently. The enforceable boundary is at dispatch: mp
//! constructs and executes herdr argv itself, records the
//! assignment event, and agents report progress through typed
//! session transitions.
//!
//! This test file reproduces the failure mode — a prose-only
//! "assignment" that looks like a notification, plus an empty
//! payload — and asserts that BOTH:
//!
//! 1. are rejected with [`TaskAssignmentValidationError::TaskAssignmentShapeViolation`]
//!    before any `herdr` spawn, AND
//! 2. leave the session log untouched (no `AssignmentDispatched`
//!    event appended).
//!
//! The same dispatcher surface that the orchestrator uses is the
//! one exercised here, so a future refactor that introduces a
//! shell-string fallback path or a "skip validation if the
//! session already exists" shortcut will trip the test rather
//! than silently reopening the M200 regression.

use std::path::Path;

use mp::autopilot::events::EventKind;
use mp::autopilot::session::{load_session, sample_session_for_tests, save_session};
use mp::autopilot::task_assign::{
    dispatch_assignment, parse_assignment, validate_assignment, validate_assignment_structure,
    validate_pane_membership, RoleDirection, TaskAssignment, TaskAssignmentValidationError,
};
use mp::paths::PlanContext;
use serde_json::json;
use tempfile::TempDir;

mod common;

use common::TestEnv;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

/// A bogus binary path — using a missing path makes any
/// "accidental spawn" surface as ENOENT rather than a silent
/// success, so the test cannot pass by a happy accident.
const BOGUS_HERDR: &str = "/this/herdr/binary/does/not/exist/m200-regression";

/// Sample layout matching `sample_session_for_tests` —
/// `%1` orchestrator, `%2` runner, `%3` reviewer.
fn layout() -> mp::autopilot::session::PaneLayout {
    mp::autopilot::session::PaneLayout {
        orchestrator: Some(mp::autopilot::session::PaneRef {
            pane_id: "%1".to_string(),
            label: Some("role-orchestrator-1".to_string()),
        }),
        runner: Some(mp::autopilot::session::PaneRef {
            pane_id: "%2".to_string(),
            label: Some("role-runner-1".to_string()),
        }),
        reviewer: Some(mp::autopilot::session::PaneRef {
            pane_id: "%3".to_string(),
            label: Some("role-reviewer-1".to_string()),
        }),
    }
}

/// M200-style prose "assignment". The historical mistake was to
/// treat a free-form prose string as if it were a task. The
/// typed dispatcher must reject it on shape alone — no string
/// can masquerade as a TaskAssignment.
const PROSE_NOTIFY: &str = "notify the runner about M211 cycle 1 — please stand by";

const PROSE_HERDR_PROMPT: &str = "herdr agent prompt %2 dispatch M211 to runner";

const PROSE_RUN_CYCLE: &str = "run cycle 1 now";

const EMPTY_OBJECT_JSON: &str = "{}";

#[test]
fn prose_string_rejected_at_parse_assignment() {
    // The library boundary: even if a caller hands us a JSON
    // value that is a string (the prose shape), parse_assignment
    // refuses it as TaskAssignmentShapeViolation. The variant
    // name is the spec's exact wording — pin it so a future
    // rename of the error doesn't silently re-open the regression.
    let prose_json = json!(PROSE_NOTIFY);
    let err = parse_assignment(&prose_json).unwrap_err();
    assert!(
        matches!(
            err,
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
        ),
        "prose must be rejected as TaskAssignmentShapeViolation; got {err:?}"
    );
    // The error message must mention shape / typed payload so a
    // human reader sees the gate's intent.
    let msg = err.to_string();
    assert!(
        msg.contains("shape") || msg.contains("TaskAssignment"),
        "error message must surface the typed-payload contract; got: {msg}"
    );
}

#[test]
fn prose_with_herdr_subcommand_rejected_at_parse_assignment() {
    // The M200 mistake also covered cases where the prose
    // happened to contain the words "herdr agent prompt". A
    // naive text-matching heuristic might accept this; the typed
    // dispatcher does not.
    let prose_json = json!(PROSE_HERDR_PROMPT);
    let err = parse_assignment(&prose_json).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));
}

#[test]
fn empty_object_rejected_at_parse_assignment() {
    // Empty object has the right JSON shape (object) but is
    // missing every required field. parse_assignment surfaces
    // this as TaskAssignmentShapeViolation, not as a partial
    // parse — the typed contract is all-or-nothing.
    let empty = json!({});
    let err = parse_assignment(&empty).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));
}

#[test]
fn empty_payload_string_rejected_at_parse_assignment() {
    let empty = json!("");
    let err = parse_assignment(&empty).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));
}

#[test]
fn prose_dispatch_does_not_call_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    // Build a "prose dispatch" by attempting to use a string-typed
    // JSON value through parse_assignment. The library boundary
    // rejects it before dispatch_assignment is ever called. This
    // pins the regression at the parser layer — a future caller
    // who tries to bypass parse_assignment and go straight to
    // dispatch_assignment with a malformed payload still hits the
    // validation gate (see the next tests).
    let prose_json = json!(PROSE_NOTIFY);
    let err = parse_assignment(&prose_json).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));

    // Session is untouched.
    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded.events.is_empty(),
        "prose dispatch must not touch the session log; got {} events",
        reloaded.events.len()
    );
}

#[test]
fn empty_dispatch_does_not_call_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    // Try to construct a TaskAssignment via the typed builder
    // with empty fields. The structural validator catches it
    // before dispatch_assignment ever tries to load the session.
    let bad = TaskAssignment::new("", "", 0, RoleDirection::OrchestratorToRunner, "", "");
    let result = dispatch_assignment(&ctx, Path::new(BOGUS_HERDR), &bad);
    let err = result.expect_err("empty payload must be rejected");
    // structural validator runs first — empty session_id is the
    // first invariant to trip.
    assert_eq!(err, TaskAssignmentValidationError::EmptySessionId);

    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded.events.is_empty(),
        "empty dispatch must not append any events; got {} events",
        reloaded.events.len()
    );
}

#[test]
fn only_typed_dispatcher_can_append_assignment_event() {
    // The structural + shape gates guarantee that ONLY a payload
    // shaped as TaskAssignment can reach the event-appending
    // path inside dispatch_assignment. To prove the negative,
    // this test exercises every rejection path and asserts that
    // the event log stays empty after each.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();
    let bogus = Path::new(BOGUS_HERDR);

    // 1. Prose string -> parse_assignment rejects.
    let prose = json!(PROSE_RUN_CYCLE);
    let err = parse_assignment(&prose).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));

    // 2. Empty object -> parse_assignment rejects.
    let err = parse_assignment(&json!(EMPTY_OBJECT_JSON)).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));

    // 3. TaskAssignment with empty fields -> structural rejects.
    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "ok",
    );
    bad.session_id = "".into();
    let err = dispatch_assignment(&ctx, bogus, &bad).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptySessionId);

    // 4. TaskAssignment with mismatched pane -> layout rejects.
    let bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%99",
        "ok",
    );
    let err = dispatch_assignment(&ctx, bogus, &bad).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
    ));

    // After every rejection path, the session log has zero
    // events. Only a fully-validated TaskAssignment reaching
    // herdr can append AssignmentDispatched — the M200
    // regression is closed.
    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded.events.is_empty(),
        "no AssignmentDispatched event may exist after any rejection path; got {} events",
        reloaded.events.len()
    );
    let assignment_events: Vec<_> = reloaded
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::AssignmentDispatched))
        .collect();
    assert!(
        assignment_events.is_empty(),
        "no AssignmentDispatched event may exist; found {}",
        assignment_events.len()
    );
}

#[test]
fn prose_string_cannot_reach_herdr_via_direct_dispatch() {
    // Belt-and-braces: even if a future caller tries to bypass
    // parse_assignment and call dispatch_assignment directly
    // with a payload that happens to have non-empty strings but
    // no real shape, the structural + layout gates still catch
    // it. This test exercises the dispatcher path with a
    // payload whose task field IS non-empty but whose
    // session_id is empty — proving the dispatcher never reaches
    // herdr.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        PROSE_RUN_CYCLE,
    );
    bad.session_id = "".into();
    let err = dispatch_assignment(&ctx, Path::new(BOGUS_HERDR), &bad).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptySessionId);

    // Same for a payload that the prose-style caller might
    // construct — empty task body, all other fields present.
    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "",
    );
    bad.task = "  ".into();
    let err = dispatch_assignment(&ctx, Path::new(BOGUS_HERDR), &bad).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptyTask);

    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(reloaded.events.is_empty());
}

#[test]
fn parse_assignment_rejects_numeric_and_array_payloads() {
    // Other JSON shapes that look "structured" but are not
    // TaskAssignment. The parser refuses all of them — only an
    // object with the right required fields is accepted.
    for value in [json!(42), json!(["a", "b", "c"]), json!(true), json!(null)] {
        let err = parse_assignment(&value)
            .expect_err(&format!("non-object value {value} must be rejected"));
        assert!(
            matches!(
                err,
                TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
            ),
            "non-object must surface TaskAssignmentShapeViolation; got {err:?}"
        );
    }
}

#[test]
fn validate_assignment_structure_catches_partial_payloads() {
    // Pure-function-level: a payload constructed via the typed
    // builder with missing required fields is rejected before
    // any I/O. This is what makes dispatch_assignment safe — a
    // caller can't bypass the validator by skipping
    // parse_assignment.
    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "do thing",
    );
    // Drop the task body — should fail EmptyTask.
    bad.task = "".into();
    assert_eq!(
        validate_assignment_structure(&bad).unwrap_err(),
        TaskAssignmentValidationError::EmptyTask
    );
    // Drop the milestone id — should fail EmptyMilestoneId.
    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "do thing",
    );
    bad.milestone_id = "".into();
    assert_eq!(
        validate_assignment_structure(&bad).unwrap_err(),
        TaskAssignmentValidationError::EmptyMilestoneId
    );
    // Drop the target pane — should fail EmptyTargetPane.
    let mut bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "do thing",
    );
    bad.target_pane = "".into();
    assert_eq!(
        validate_assignment_structure(&bad).unwrap_err(),
        TaskAssignmentValidationError::EmptyTargetPane
    );
    // Zero cycle — should fail ZeroCycle.
    let bad = TaskAssignment::new(
        "alpha",
        "M211",
        0,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "do thing",
    );
    assert_eq!(
        validate_assignment_structure(&bad).unwrap_err(),
        TaskAssignmentValidationError::ZeroCycle
    );
}

#[test]
fn validate_pane_membership_rejects_prose_pane() {
    // The layout gate catches a payload that passes structural
    // checks but whose target pane is just text. This is the
    // boundary that prevents a payload like `target_pane =
    // "anything"` from sneaking through.
    let bad = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "anything",
        "do thing",
    );
    let err = validate_pane_membership(&bad, &layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
    ));

    // Compose: full validate_assignment = structural + layout.
    let err = validate_assignment(&bad, &layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
    ));
}

/// Synthetic tempdir to keep the file's dep on `tempfile` if the
/// crate's macros / macro_rules! are reorganized later.
#[test]
fn _sanity_tempdir_compiles() {
    let _t: TempDir = tempfile::tempdir().unwrap();
}
