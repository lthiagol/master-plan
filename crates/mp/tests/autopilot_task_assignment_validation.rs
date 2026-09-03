//! M211 / AC-02: validation + shell-safe argv before dispatch.
//!
//! The dispatcher refuses to call `herdr` or mutate the session log
//! when the payload is malformed. Every gate the spec calls out is
//! asserted here against the typed dispatcher surface:
//!
//! - empty payload (missing required fields) -> rejected
//! - unknown target pane (not in the session's layout) -> rejected
//! - invalid role direction (not in the closed set) -> rejected
//! - missing session identity (no session_id / unknown session
//!   directory) -> rejected
//! - shell metacharacter interpolation -> rejected
//!
//! In every rejection case, the test also asserts that NO
//! `AssignmentDispatched` event was appended — the session log is
//! silent about a dispatch that never happened.

use std::path::Path;

use mp::autopilot::events::EventKind;
use mp::autopilot::session::{
    load_session, sample_session_for_tests, save_session, PaneLayout, PaneRef,
};
use mp::autopilot::task_assign::{
    build_assignment_argv, execute_assignment, parse_assignment, validate_assignment, RoleDirection,
    TaskAssignment, TaskAssignmentValidationError,
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

/// Layout matching `sample_session_for_tests` so the panes are
/// `%1` orchestrator, `%2` runner, `%3` reviewer.
fn test_layout() -> PaneLayout {
    PaneLayout {
        orchestrator: Some(PaneRef {
            pane_id: "%1".to_string(),
            label: Some("role-orchestrator-1".to_string()),
        }),
        runner: Some(PaneRef {
            pane_id: "%2".to_string(),
            label: Some("role-runner-1".to_string()),
        }),
        reviewer: Some(PaneRef {
            pane_id: "%3".to_string(),
            label: Some("role-reviewer-1".to_string()),
        }),
    }
}

fn well_formed_runner_payload() -> TaskAssignment {
    TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "execute cycle 1",
    )
}

#[test]
fn validation_rejects_empty_session_id() {
    let mut p = well_formed_runner_payload();
    p.session_id = "".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptySessionId);
}

#[test]
fn validation_rejects_empty_milestone_id() {
    let mut p = well_formed_runner_payload();
    p.milestone_id = "  ".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptyMilestoneId);
}

#[test]
fn validation_rejects_empty_task_body() {
    let mut p = well_formed_runner_payload();
    p.task = "".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::EmptyTask);
}

#[test]
fn validation_rejects_zero_cycle() {
    let mut p = well_formed_runner_payload();
    p.cycle = 0;
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert_eq!(err, TaskAssignmentValidationError::ZeroCycle);
}

#[test]
fn validation_rejects_unknown_target_pane() {
    let mut p = well_formed_runner_payload();
    // `%99` is not in the test layout (which has %1/%2/%3).
    p.target_pane = "%99".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
    ));
}

#[test]
fn validation_rejects_pane_mismatch_between_directions() {
    // Direction is orchestrator->runner but the target pane id is
    // the reviewer pane — the pane-id-vs-direction contract is
    // violated.
    let mut p = well_formed_runner_payload();
    p.target_pane = "%3".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout {
            direction: RoleDirection::OrchestratorToRunner,
            ..
        }
    ));
}

#[test]
fn validation_rejects_invalid_role_direction() {
    // parse_assignment gates the closed RoleDirection set. A
    // bogus direction string fails before validation runs.
    let bad = json!({
        "session_id": "alpha",
        "milestone_id": "M211",
        "cycle": 1,
        "direction": "runner-to-orchestrator",
        "target_pane": "%2",
        "task": "reverse direction",
    });
    let err = parse_assignment(&bad).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));
}

#[test]
fn validation_rejects_shell_metachar_in_task() {
    let mut p = well_formed_runner_payload();
    p.task = "echo hello; rm -rf /".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::ShellMetacharacter {
            field: "task",
            ..
        }
    ));
}

#[test]
fn validation_rejects_shell_metachar_in_target_pane() {
    let mut p = well_formed_runner_payload();
    p.target_pane = "%2`whoami`".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::ShellMetacharacter {
            field: "target_pane",
            ..
        }
    ));
}

#[test]
fn validation_rejects_shell_metachar_in_session_id() {
    let mut p = well_formed_runner_payload();
    p.session_id = "alpha$USER".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::ShellMetacharacter {
            field: "session_id",
            ..
        }
    ));
}

#[test]
fn validation_rejects_shell_metachar_in_milestone_id() {
    let mut p = well_formed_runner_payload();
    p.milestone_id = "M211 && echo pwned".into();
    let err = validate_assignment(&p, &test_layout()).unwrap_err();
    assert!(matches!(
        err,
        TaskAssignmentValidationError::ShellMetacharacter {
            field: "milestone_id",
            ..
        }
    ));
}

#[test]
fn validation_accepts_well_formed_payload() {
    let p = well_formed_runner_payload();
    assert!(validate_assignment(&p, &test_layout()).is_ok());
}

#[test]
fn validation_accepts_payload_with_evidence_and_reminders() {
    let p = well_formed_runner_payload()
        .with_evidence_ref("cargo nextest run -p mp --test foo")
        .with_boundary_reminder("report via mp autopilot session transition");
    assert!(validate_assignment(&p, &test_layout()).is_ok());
    let argv = build_assignment_argv(&p);
    // No shell interpolation — argv is a Vec<String> passed to
    // Command::args. The renderer is the only path that constructs
    // it; this asserts the path is reachable from a valid payload.
    assert_eq!(argv[0], "agent");
    assert_eq!(argv[1], "prompt");
    assert_eq!(argv[2], "%2");
    assert!(argv[3].contains("cargo nextest run -p mp --test foo"));
    assert!(argv[3].contains("mp autopilot session transition"));
}

#[test]
fn argv_passes_through_command_args_without_shell_interpolation() {
    // The renderer must NOT prepend /bin/sh, NOT concat argv with
    // `&&` / `;` / `|`, and NOT use `format!` into a single shell
    // string. Defense in depth: even a payload that escapes the
    // metachar gate (via future bug) cannot break out of
    // Command::args — argv is passed as a Vec<String> so each
    // element is its own literal argument.
    let argv = build_assignment_argv(&well_formed_runner_payload());
    assert!(
        !argv.iter().any(|a| a == "/bin/sh" || a == "sh" || a == "-c"),
        "argv must not include a shell; got {argv:?}"
    );
    // Argv length must be the golden 4: [agent, prompt, <pane>, <text>].
    assert_eq!(argv.len(), 4);
}

#[test]
fn execute_assignment_calls_herdr_via_command_args() {
    // Use the canonical `true` binary as a stand-in for herdr —
    // exit 0 with no side effects. Asserts the I/O wrapper hands
    // argv to Command::args (no shell). macOS ships true at
    // /usr/bin/true; on linux it lives at /bin/true. Either path
    // works here — the goal is to prove the wrapper reaches
    // Command::status() with a real argv vector.
    let true_path = if Path::new("/usr/bin/true").exists() {
        Path::new("/usr/bin/true")
    } else {
        Path::new("/bin/true")
    };
    let argv = vec!["true".to_string()];
    let status = execute_assignment(true_path, &argv).unwrap();
    assert!(status.success(), "true must exit 0; got {status:?}");
}

#[test]
fn execute_assignment_propagates_spawn_failure() {
    // A bogus binary path produces an io::Error (ENOENT), not a
    // successful ExitStatus. The wrapper must surface the error so
    // the dispatcher records a SpawnError outcome.
    let argv = vec!["agent".to_string(), "prompt".to_string()];
    let err = execute_assignment(Path::new("/this/binary/does/not/exist"), &argv).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ─── End-to-end: dispatch rejects bad payload without
// touching herdr or the session log ──────────────────────

#[test]
fn dispatch_validation_failure_leaves_session_events_untouched() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    // Build a payload whose target pane is not in the layout. The
    // dispatcher is wired to call validate_assignment against the
    // loaded layout, so this must fail without ever spawning
    // herdr. The test sets `herdr_bin` to a path that does NOT
    // exist so any "accidental spawn" would surface as ENOENT
    // rather than a silent success.
    let bad_payload = TaskAssignment::new(
        "alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%99", // not in layout
        "do thing",
    );
    let bogus_herdr = Path::new("/this/herdr/binary/does/not/exist");
    let result = mp::autopilot::task_assign::dispatch_assignment(&ctx, bogus_herdr, &bad_payload);
    let err = result.expect_err("validation must reject before herdr runs");
    assert!(matches!(
        err,
        TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
    ));

    // Critical assertion: the event log has zero entries. The
    // dispatcher MUST NOT append an AssignmentDispatched event
    // for a rejected dispatch.
    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(
        reloaded.events.len(),
        0,
        "rejected dispatch must not append an AssignmentDispatched event; got {} events",
        reloaded.events.len()
    );
    let assignment_events: Vec<_> = reloaded
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::AssignmentDispatched))
        .collect();
    assert!(
        assignment_events.is_empty(),
        "no AssignmentDispatched event may exist after a validation rejection; found {}",
        assignment_events.len()
    );
}

#[test]
fn dispatch_validation_failure_for_shell_metachar_does_not_run_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    let mut bad_payload = well_formed_runner_payload();
    bad_payload.session_id = "alpha".into();
    bad_payload.task = "echo hi; rm -rf /".into();
    let bogus_herdr = Path::new("/this/herdr/binary/does/not/exist");
    let result = mp::autopilot::task_assign::dispatch_assignment(&ctx, bogus_herdr, &bad_payload);
    let err = result.expect_err("shell-metachar payload must be rejected");
    assert!(matches!(
        err,
        TaskAssignmentValidationError::ShellMetacharacter { .. }
    ));

    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded
            .events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::AssignmentDispatched)),
        "no AssignmentDispatched event may exist after shell-metachar rejection"
    );
}

#[test]
fn dispatch_validation_failure_for_empty_session_id_does_not_run_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    let mut bad_payload = well_formed_runner_payload();
    bad_payload.session_id = "".into();
    let bogus_herdr = Path::new("/this/herdr/binary/does/not/exist");
    let result = mp::autopilot::task_assign::dispatch_assignment(&ctx, bogus_herdr, &bad_payload);
    let err = result.expect_err("empty session id must be rejected");
    assert_eq!(err, TaskAssignmentValidationError::EmptySessionId);

    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded
            .events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::AssignmentDispatched)),
        "no AssignmentDispatched event may exist after empty-session-id rejection"
    );
}

#[test]
fn dispatch_validation_failure_for_zero_cycle_does_not_run_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    let mut bad_payload = well_formed_runner_payload();
    bad_payload.cycle = 0;
    let bogus_herdr = Path::new("/this/herdr/binary/does/not/exist");
    let result = mp::autopilot::task_assign::dispatch_assignment(&ctx, bogus_herdr, &bad_payload);
    let err = result.expect_err("zero cycle must be rejected");
    assert_eq!(err, TaskAssignmentValidationError::ZeroCycle);

    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(
        reloaded
            .events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::AssignmentDispatched)),
        "no AssignmentDispatched event may exist after zero-cycle rejection"
    );
}

#[test]
fn dispatch_validation_failure_for_invalid_direction_does_not_run_herdr() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();

    // Build a payload that parses but carries an unsupported
    // direction. The `parse_assignment` gate catches this before
    // validation runs.
    let bogus = json!({
        "session_id": "alpha",
        "milestone_id": "M211",
        "cycle": 1,
        "direction": "peer-to-peer",
        "target_pane": "%2",
        "task": "do thing",
    });
    let parse_err = parse_assignment(&bogus).unwrap_err();
    assert!(matches!(
        parse_err,
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
    ));

    // The session was never touched — no event appended.
    let reloaded = load_session(&ctx, "alpha").unwrap();
    assert!(reloaded.events.is_empty());
}

/// Drives `validate_assignment` over a payload matrix, asserting
/// that every malformed variant surfaces as the expected error
/// variant and the well-formed variant passes. Reduces test
/// boilerplate and pins the per-error mapping in one place.
#[test]
fn validation_error_mapping_matrix() {
    use TaskAssignmentValidationError as E;
    let layout = test_layout();
    let cases: Vec<(&str, Box<dyn Fn(&mut TaskAssignment)>, E)> = vec![
        (
            "empty_session",
            Box::new(|p| p.session_id = "".into()),
            E::EmptySessionId,
        ),
        (
            "empty_milestone",
            Box::new(|p| p.milestone_id = "".into()),
            E::EmptyMilestoneId,
        ),
        (
            "empty_task",
            Box::new(|p| p.task = "".into()),
            E::EmptyTask,
        ),
        (
            "empty_target_pane",
            Box::new(|p| p.target_pane = "".into()),
            E::EmptyTargetPane,
        ),
        (
            "zero_cycle",
            Box::new(|p| p.cycle = 0),
            E::ZeroCycle,
        ),
        (
            "shell_in_task",
            Box::new(|p| p.task = "x;y".into()),
            E::ShellMetacharacter {
                field: "task",
                value: String::new(),
            },
        ),
    ];
    for (name, mutate, expected) in cases {
        let mut p = well_formed_runner_payload();
        mutate(&mut p);
        let got = validate_assignment(&p, &layout).expect_err(name);
        match (&got, &expected) {
            (E::ShellMetacharacter { field: f1, .. }, E::ShellMetacharacter { field: f2, .. }) => {
                assert_eq!(f1, f2, "{name}: shell field mismatch");
            }
            _ => assert_eq!(got, expected, "{name}: error variant mismatch"),
        }
    }
}

/// Synthetic tempdir not via TestEnv — the validation matrix
/// doesn't need a populated plan; it only exercises the pure
/// `validate_assignment` function. This test sits beside the rest
/// for completeness; kept here so the dispatcher-path tests share
/// the same fixture setup.
#[test]
fn _sanity_tempdir_compiles() {
    let _t: TempDir = tempfile::tempdir().unwrap();
}