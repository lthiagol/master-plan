//! M207 / S6 / AC-06: typed role-state transition CLI + table.
//!
//! Black-box coverage:
//! - `mp autopilot session transition --session <id> --role <r>
//!   --state <s> [--working-on <m:n>]` applies the transition and
//!   stamps actor / sequence / timestamp on a fresh event.
//! - Valid transitions are recorded in the session.json event log
//!   and reflected in `role_state.<role>.state`.
//! - Invalid transitions are rejected; the session is not modified.
//! - Direct session-file edits are unnecessary — every role-state
//!   change goes through the transition CLI.

mod common;

use common::TestEnv;
use mp::autopilot::session::{load_session, sample_session_for_tests, save_session, RoleName};
use mp::autopilot::transitions::{is_valid as is_valid_transition, RoleState, TransitionError};
use mp::autopilot::AutopilotSession;
use mp::paths::PlanContext;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn transition_cli_starts_runner_and_persists_event() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    // Clean slate role_state for a deterministic test.
    session.role_state = None;
    session.working_on = None;
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "session",
        "transition",
        "--session",
        "alpha",
        "--role",
        "runner",
        "--state",
        "starting",
        "--actor",
        "test-actor",
    ]);
    assert!(
        out.status.success(),
        "transition failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let loaded = load_session(&ctx, "alpha").unwrap();
    let runner = loaded
        .role_state
        .as_ref()
        .and_then(|m| m.runner.as_ref())
        .expect("runner role-state must be set");
    assert_eq!(runner.state, RoleState::Starting);
    assert_eq!(runner.actor.as_deref(), Some("test-actor"));
    assert!(!loaded.events.is_empty());
    assert_eq!(
        loaded.events.last().unwrap().kind,
        mp::autopilot::EventKind::Transition
    );
}

#[test]
fn transition_cli_working_state_sets_session_working_on() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "session",
        "transition",
        "--session",
        "alpha",
        "--role",
        "runner",
        "--state",
        "working",
        "--working-on",
        "207:2",
    ]);
    assert!(
        out.status.success(),
        "transition failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let loaded = load_session(&ctx, "alpha").unwrap();
    let wo = loaded.working_on.expect("working_on must be set");
    assert_eq!(wo.milestone_id, "207");
    assert_eq!(wo.cycle, 2);
}

#[test]
fn transition_cli_rejects_invalid_transition() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    save_session(&ctx, "alpha", &session).unwrap();

    // Idle -> Done is not in the table.
    let out = env.run(&[
        "autopilot",
        "session",
        "transition",
        "--session",
        "alpha",
        "--role",
        "runner",
        "--state",
        "done",
    ]);
    assert!(
        !out.status.success(),
        "expected rejection; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );

    // The session must not have been mutated.
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert!(loaded.role_state.is_none() || {
        loaded
            .role_state
            .as_ref()
            .and_then(|m| m.runner.as_ref())
            .map(|r| r.state != RoleState::Done)
            .unwrap_or(true)
    });
}

#[test]
fn transition_table_covers_happy_path() {
    assert!(is_valid_transition(RoleState::Idle, RoleState::Starting));
    assert!(is_valid_transition(RoleState::Starting, RoleState::Working));
    assert!(is_valid_transition(RoleState::Working, RoleState::Done));
    assert!(is_valid_transition(RoleState::Done, RoleState::Idle));
    assert!(is_valid_transition(RoleState::Working, RoleState::Blocked));
    assert!(is_valid_transition(RoleState::Blocked, RoleState::Working));
}

#[test]
fn transition_table_rejects_skip() {
    // Idle -> Done skips Working; not allowed.
    assert!(!is_valid_transition(RoleState::Idle, RoleState::Done));
}

#[test]
fn apply_transition_stamps_actor_and_since() {
    // Library-level: transitions mutate the session in place and
    // stamp actor / since / working_on.
    let mut s: AutopilotSession = sample_session_for_tests("alpha");
    s.role_state = None;
    s.working_on = None;
    let outcome = mp::autopilot::apply_transition(
        &mut s,
        RoleName::Reviewer,
        RoleState::Starting,
        "reviewer-agent",
        None,
    )
    .unwrap();
    assert!(outcome.was_applied());
    let record = outcome.record();
    assert_eq!(record.role, RoleName::Reviewer);
    assert_eq!(record.state, RoleState::Starting);
    assert_eq!(record.actor.as_deref(), Some("reviewer-agent"));
    assert!(record.since.is_some());
}

#[test]
fn apply_transition_returns_invalid_transition_error() {
    let mut s = AutopilotSession::blank("alpha");
    let err = mp::autopilot::apply_transition(
        &mut s,
        RoleName::Orchestrator,
        RoleState::Done, // idle -> done not in table
        "test",
        None,
    )
    .unwrap_err();
    match err {
        TransitionError::InvalidTransition { from, to } => {
            assert_eq!(from, RoleState::Idle);
            assert_eq!(to, RoleState::Done);
        }
        other => panic!("expected InvalidTransition, got {other:?}"),
    }
}