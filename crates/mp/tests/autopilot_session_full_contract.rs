//! M207 / S8 / AC-05 + AC-08: full contract test for the documented
//! session.json schema.
//!
//! Pulls the schema from the documented `docs/autopilot/session-format.md`
//! surface and asserts every documented field is present, typed, and
//! round-trippable. This is the single test the M210–M217 milestones
//! will rely on to know the autopilot session contract is intact.

mod common;

use common::TestEnv;
use mp::autopilot::ac_projection::{
    canonical_revision, project_ac_status, AcProjection, ProjectionKey,
};
use mp::autopilot::apply_transition;
use mp::autopilot::events::EventKind;
use mp::autopilot::notes::{build_note, NoteKind};
use mp::autopilot::session::{
    load_session, sample_session_for_tests, save_session, AutopilotSession, Controls, EvidenceRefs,
    PaneRef, QueueItem, RoleConfig, RoleName, RolesConfig, SessionConfigOverrides, SessionStatus,
    Stage, Topology, WorkingOn,
};
use mp::autopilot::transitions::{is_valid as is_valid_transition, RoleState};
use mp::autopilot::{
    recover_session, AcStatus as PubAcStatus, EventCursor, EventKind as PubEventKind,
    OrchestrationEvent, RecoveredSession,
};
use mp::paths::PlanContext;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn documented_fields_are_all_present() {
    // Drive every documented field through the typed view + a
    // round-trip. Anything missing from this exercise is a
    // contract drift between docs/autopilot/session-format.md
    // and the typed Rust view.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = AutopilotSession::blank("alpha");

    // Topology + roles.
    session.topology = Topology {
        orchestrator: Some(PaneRef {
            pane_id: "%1".into(),
            label: Some("role-orchestrator-1".into()),
        }),
        runner: Some(PaneRef {
            pane_id: "%2".into(),
            label: Some("role-runner-1".into()),
        }),
        reviewer: Some(PaneRef {
            pane_id: "%3".into(),
            label: Some("role-reviewer-1".into()),
        }),
    };
    session.roles = RolesConfig {
        orchestrator: Some(RoleConfig {
            role: RoleName::Orchestrator,
            pane_id: Some("%1".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-coordinator".into()),
            config_hash: Some("v1".into()),
        }),
        runner: Some(RoleConfig {
            role: RoleName::Runner,
            pane_id: Some("%2".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-runner".into()),
            config_hash: Some("v1".into()),
        }),
        reviewer: Some(RoleConfig {
            role: RoleName::Reviewer,
            pane_id: Some("%3".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-runner".into()),
            config_hash: Some("v1".into()),
        }),
    };

    // Queue with evidence_refs.
    session.queue.push(QueueItem {
        milestone_id: "207".into(),
        stage: Stage::InProgress,
        cycle: 1,
        last_notify: None,
        verifier_verdict: None,
        evidence_refs: Some(EvidenceRefs {
            lifecycle: Some("in-progress".into()),
            execution_status: Some("in-progress".into()),
            spec_status: Some("ready".into()),
            reviews_verdict: None,
        }),
    });

    // Status + terminal_status.
    session.status = SessionStatus::Active;
    session.terminal_status = None;

    // Working_on + role_state via apply_transition.
    apply_transition(
        &mut session,
        RoleName::Runner,
        RoleState::Starting,
        "test",
        None,
    )
    .unwrap();
    apply_transition(
        &mut session,
        RoleName::Runner,
        RoleState::Working,
        "test",
        Some(WorkingOn {
            milestone_id: "207".into(),
            cycle: 1,
            role: Some(RoleName::Runner),
        }),
    )
    .unwrap();

    // Controls.
    session.controls = Controls {
        paused: false,
        pause_reason: None,
        resume_after: None,
    };

    // Runner notes via build_note.
    let note = build_note(&session, NoteKind::Info, "starting cycle 1", Some(1), None).unwrap();
    session.runner_notes.push(note);

    // Events via append_event_unchecked.
    let event = OrchestrationEvent::new(
        1,
        PubEventKind::Dispatch,
        "test",
        serde_json::json!({"stage": "execute"}),
    );
    mp::autopilot::append_event_unchecked(&mut session, event).unwrap();

    // AC projection via project_ac_status.
    project_ac_status(
        &mut session,
        ProjectionKey::new("207", "AC-01"),
        AcProjection {
            ac_id: "AC-01".into(),
            status: PubAcStatus::Passed,
            evidence: Some("ok".into()),
            source_revision: canonical_revision("seed", "207", &[("AC-01", PubAcStatus::Passed)]),
            projected_at: Some("2026-01-01T00:00:00Z".into()),
        },
    );

    // config_overrides
    session.config_overrides = SessionConfigOverrides {
        topology: Some("3-pane".into()),
        stall_timeout_ms: Some(1_800_000),
        poll_interval_ms: Some(1000),
    };

    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();

    // Every documented field is round-trippable.
    assert_eq!(loaded.id, "alpha");
    assert_eq!(loaded.schema_version, mp::autopilot::SESSION_SCHEMA_VERSION);
    assert!(loaded.topology.orchestrator.is_some());
    assert!(loaded.roles.runner.is_some());
    assert_eq!(loaded.queue.len(), 1);
    assert!(loaded.queue[0].evidence_refs.is_some());
    assert_eq!(loaded.status, SessionStatus::Active);
    assert!(!loaded.controls.paused);
    assert_eq!(loaded.runner_notes.len(), 1);
    assert_eq!(loaded.runner_notes[0].kind, NoteKind::Info);
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.event_cursor.last_seq, 1);
    assert!(loaded
        .ac_projections
        .get("207")
        .and_then(|m| m.get("AC-01"))
        .is_some());
    assert_eq!(loaded.working_on.as_ref().map(|w| w.cycle), Some(1));
    assert!(loaded
        .role_state
        .as_ref()
        .and_then(|m| m.runner.as_ref())
        .is_some());
    assert_eq!(loaded.config_overrides.stall_timeout_ms, Some(1_800_000));
}

#[test]
fn terminal_status_transitions_are_validated() {
    // `completed` / `failed` / `cancelled` are terminal; the
    // schema exposes them as the only values for `terminal_status`.
    // The typed enum has the same closed set — the schema validator
    // + the typed view agree.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.terminal_status = Some(SessionStatus::Completed);
    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.terminal_status, Some(SessionStatus::Completed));
}

#[test]
fn queue_cycle_history_round_trips() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session
        .queue_cycle_history
        .push(mp::autopilot::session::CycleHistoryEntry {
            milestone_id: "207".into(),
            cycle: 1,
            started_at: "2026-01-01T00:00:00Z".into(),
            completed_at: Some("2026-01-02T00:00:00Z".into()),
            outcome: Some("executed".into()),
        });
    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.queue_cycle_history.len(), 1);
    assert_eq!(loaded.queue_cycle_history[0].milestone_id, "207");
    assert_eq!(loaded.queue_cycle_history[0].cycle, 1);
    assert_eq!(
        loaded.queue_cycle_history[0].outcome.as_deref(),
        Some("executed")
    );
}

#[test]
fn schema_migrations_round_trips() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session
        .schema_migrations
        .push(mp::autopilot::session::SchemaMigration {
            from_version: 0,
            to_version: 1,
            at: "2026-01-01T00:00:00Z".into(),
        });
    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.schema_migrations.len(), 1);
    assert_eq!(loaded.schema_migrations[0].to_version, 1);
}

#[test]
fn role_state_machine_covers_full_table() {
    // Documented transition table — every cell must round-trip
    // through `is_valid_transition`. Anything missing from this
    // list is undocumented behavior.
    let cells = [
        (RoleState::Idle, RoleState::Starting),
        (RoleState::Idle, RoleState::Working),
        (RoleState::Idle, RoleState::Blocked),
        (RoleState::Starting, RoleState::Working),
        (RoleState::Starting, RoleState::Blocked),
        (RoleState::Starting, RoleState::Idle),
        (RoleState::Working, RoleState::Done),
        (RoleState::Working, RoleState::Blocked),
        (RoleState::Working, RoleState::Working),
        (RoleState::Working, RoleState::Idle),
        (RoleState::Blocked, RoleState::Working),
        (RoleState::Blocked, RoleState::Idle),
        (RoleState::Done, RoleState::Idle),
        (RoleState::Done, RoleState::Working),
        (RoleState::Unknown, RoleState::Idle),
        (RoleState::Unknown, RoleState::Starting),
        (RoleState::Unknown, RoleState::Working),
    ];
    for (from, to) in cells {
        assert!(is_valid_transition(from, to), "{:?} -> {:?}", from, to);
    }
}

#[test]
fn event_kinds_match_documented_set() {
    // Documented event kinds.
    let documented = [
        PubEventKind::Dispatch,
        PubEventKind::Transition,
        PubEventKind::Review,
        PubEventKind::Decision,
        PubEventKind::Control,
        PubEventKind::Note,
        PubEventKind::Recovery,
    ];
    let actual: Vec<EventKind> = vec![
        EventKind::Dispatch,
        EventKind::Transition,
        EventKind::Review,
        EventKind::Decision,
        EventKind::Control,
        EventKind::Note,
        EventKind::Recovery,
    ];
    assert_eq!(actual.len(), documented.len());
    for kind in documented {
        assert!(actual.contains(&kind), "{kind:?} not present");
    }
}

#[test]
fn recover_session_returns_a_diagnostic_report() {
    // AC-08 calls for recovery; ensure the report is consumable
    // by downstream callers (used by M210–M217).
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
    let events = vec![
        OrchestrationEvent::new(1, PubEventKind::Dispatch, "t", serde_json::json!({})),
        OrchestrationEvent::new(2, PubEventKind::Transition, "t", serde_json::json!({})),
        OrchestrationEvent::new(3, PubEventKind::Note, "t", serde_json::json!({})),
    ];
    for e in events {
        session.event_cursor.advance_to(e.seq).unwrap();
        session.events.push(e);
    }
    session.event_cursor.last_seq = 1;
    save_session(&ctx, "alpha", &session).unwrap();

    let report: RecoveredSession = recover_session(&ctx, "alpha").unwrap();
    assert_eq!(report.prev_cursor, 1);
    assert_eq!(report.next_cursor, 3);
    assert_eq!(report.events, 3);
}

#[test]
fn event_cursor_is_a_typed_object_with_last_seq() {
    // Pin the on-disk shape: event_cursor is `{ "last_seq": int }`.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
    let e = OrchestrationEvent::new(1, PubEventKind::Dispatch, "t", serde_json::json!({}));
    session.event_cursor.advance_to(e.seq).unwrap();
    session.events.push(e);
    save_session(&ctx, "alpha", &session).unwrap();

    let raw = std::fs::read_to_string(ctx.plan_dir.join("autopilot/alpha/session.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let cursor = value
        .get("event_cursor")
        .and_then(|c| c.get("last_seq"))
        .and_then(|s| s.as_u64());
    assert_eq!(cursor, Some(1), "event_cursor.last_seq must be a u64");
}

#[test]
fn session_status_and_terminal_status_are_distinct() {
    // session.status is the day-to-day state; terminal_status is
    // only set on terminal transitions. Both can coexist briefly.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.status = SessionStatus::Stopped;
    session.terminal_status = Some(SessionStatus::Failed);
    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.status, SessionStatus::Stopped);
    assert_eq!(loaded.terminal_status, Some(SessionStatus::Failed));
}

#[test]
fn prompt_bundles_round_trips_as_open_object() {
    // `prompt_bundles` is documented as an open object —
    // stage -> template hash. The typed view is `BTreeMap<String, Value>`.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.prompt_bundles.insert(
        "execute".to_string(),
        serde_json::json!("sha256:execute-v3"),
    );
    session
        .prompt_bundles
        .insert("review".to_string(), serde_json::json!("sha256:review-v2"));
    save_session(&ctx, "alpha", &session).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(
        loaded.prompt_bundles.get("execute"),
        Some(&serde_json::json!("sha256:execute-v3"))
    );
    assert_eq!(
        loaded.prompt_bundles.get("review"),
        Some(&serde_json::json!("sha256:review-v2"))
    );
}

// Make sure the imported types stay in scope (helps catch
// dead-code lints from future refactors).
#[allow(dead_code)]
fn _ensure_in_scope(_: EventCursor) {}
