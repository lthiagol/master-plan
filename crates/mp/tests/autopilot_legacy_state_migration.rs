//! M208 / S03+S4 / AC-03 + AC-04: versioned, idempotent legacy
//! watch-state migration.
//!
//! Black-box coverage:
//! - AC-03: a versioned legacy fixture migrates once into the M207
//!   session schema; status/list/show preserve session identity,
//!   queue order, pane ids, and lifecycle state. Re-running the
//!   migration is idempotent.
//! - AC-04: unknown, corrupt, or partially migrated legacy state
//!   fails with a typed diagnostic and leaves both the legacy source
//!   and any existing autopilot session unchanged.
//!
//! The tests exercise both the library API (via
//! `mp::autopilot::migrate_legacy_watch_state`) and the public CLI
//! (`mp autopilot migrate`, `mp autopilot session list`).

mod common;

use common::TestEnv;
use mp::autopilot::{
    migrate_legacy_watch_state, sample_session_for_tests, save_session, MigrationError,
    MigrationOutcome, MIGRATED_SESSION_ID,
};
use mp::paths::PlanContext;
use mp::watch::state::{PaneState, WatchState};
use mp::watch::Role;
use serde_json::Value;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

fn write_legacy(ctx: &PlanContext, state: &WatchState) {
    let path = mp::watch::default_state_path(&ctx.plan_dir);
    mp::autopilot::write_legacy_for_tests(&path, state).unwrap();
}

fn fixture_legacy() -> WatchState {
    let mut state = WatchState::fresh(&["207".to_string(), "209".to_string()]);
    state.milestones[0].last_lifecycle = "in-progress".to_string();
    state.milestones[0].target_lifecycle = "self-reviewed".to_string();
    state.milestones[1].last_lifecycle = "approved".to_string();
    state.milestones[1].target_lifecycle = "in-progress".to_string();
    state.panes.push(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%42".into(),
        spawned_at: "2026-09-01T00:00:00Z".into(),
        last_status: None,
    });
    state.panes.push(PaneState {
        role: Role::Coordinator,
        label: "role-coordinator-1".into(),
        pane_id: "%43".into(),
        spawned_at: "2026-09-01T00:00:00Z".into(),
        last_status: None,
    });
    state
}

#[test]
fn versioned_legacy_fixture_migrates_once_preserving_identity_and_order() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    write_legacy(&ctx, &fixture_legacy());

    let outcome = migrate_legacy_watch_state(&ctx).unwrap();
    match outcome {
        MigrationOutcome::Migrated {
            session_id,
            migrated_milestones,
            migrated_panes,
            ..
        } => {
            assert_eq!(session_id, MIGRATED_SESSION_ID);
            assert_eq!(migrated_milestones, 2);
            assert_eq!(migrated_panes, 2);
        }
        other => panic!("expected Migrated, got {other:?}"),
    }

    // Session identity, queue order, pane ids, and lifecycle state
    // are preserved on the migrated session.
    let session_path = mp::autopilot::session::SessionPath::new(&ctx, MIGRATED_SESSION_ID).unwrap();
    let session = mp::autopilot::load_session(&ctx, MIGRATED_SESSION_ID).unwrap();
    assert_eq!(session.id, MIGRATED_SESSION_ID);

    // Queue order is preserved verbatim (MilestoneState order ->
    // QueueItem order).
    assert_eq!(session.queue.len(), 2);
    assert_eq!(session.queue[0].milestone_id, "207");
    assert_eq!(session.queue[1].milestone_id, "209");

    // Lifecycle state is preserved on the queue items'
    // evidence_refs.
    assert_eq!(
        session.queue[0].evidence_refs.as_ref().unwrap().lifecycle,
        Some("in-progress".into())
    );
    assert_eq!(
        session.queue[1].evidence_refs.as_ref().unwrap().lifecycle,
        Some("approved".into())
    );

    // Pane ids are preserved (legacy `runner` and `coordinator` map
    // onto the autopilot topology slots). The reviewer pane is a
    // post-M208 addition; the migration seeds a placeholder so the
    // session passes schema validation, and a follow-on milestone
    // can spawn a real reviewer pane.
    assert_eq!(session.topology.runner.as_ref().unwrap().pane_id, "%42");
    assert_eq!(
        session.topology.orchestrator.as_ref().unwrap().pane_id,
        "%43"
    );
    assert!(
        session.topology.reviewer.is_some(),
        "reviewer pane slot must be populated (placeholder) so the schema passes"
    );

    // Source file is preserved (the migration never deletes the
    // legacy fixture).
    assert!(mp::watch::default_state_path(&ctx.plan_dir).exists());

    // Schema migration audit entry is recorded on the session so
    // future downgrades are detectable.
    assert_eq!(session.schema_migrations.len(), 1);
    assert_eq!(session.schema_migrations[0].from_version, 1);
    assert_eq!(
        session.schema_migrations[0].to_version,
        mp::autopilot::SESSION_SCHEMA_VERSION
    );
    // Path is touched (sanity).
    assert!(session_path.file.exists());
}

#[test]
fn re_running_migration_is_idempotent() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    write_legacy(&ctx, &fixture_legacy());

    let first = migrate_legacy_watch_state(&ctx).unwrap();
    assert!(matches!(first, MigrationOutcome::Migrated { .. }));

    // Capture the migrated session's content for byte-level
    // idempotency checks.
    let before = mp::autopilot::load_session(&ctx, MIGRATED_SESSION_ID).unwrap();
    let session_path = mp::autopilot::session::SessionPath::new(&ctx, MIGRATED_SESSION_ID).unwrap();
    let before_bytes = std::fs::read(&session_path.file).unwrap();

    let second = migrate_legacy_watch_state(&ctx).unwrap();
    match second {
        MigrationOutcome::AlreadyMigrated { session_id, .. } => {
            assert_eq!(session_id, MIGRATED_SESSION_ID);
        }
        other => panic!("expected AlreadyMigrated on re-run, got {other:?}"),
    }

    // The migrated session is untouched.
    let after_bytes = std::fs::read(&session_path.file).unwrap();
    let after = mp::autopilot::load_session(&ctx, MIGRATED_SESSION_ID).unwrap();
    assert_eq!(before_bytes, after_bytes);
    assert_eq!(before.queue, after.queue);
}

#[test]
fn corrupt_legacy_state_surfaces_typed_error_and_preserves_source() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let source_path = mp::watch::default_state_path(&ctx.plan_dir);
    std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    std::fs::write(&source_path, b"{not json").unwrap();

    let err = migrate_legacy_watch_state(&ctx).unwrap_err();
    match err {
        MigrationError::CorruptSource { path, reason } => {
            assert_eq!(path, source_path);
            assert!(
                reason.contains("parse") || reason.contains("read"),
                "reason should describe the parse / read failure; got: {reason}"
            );
        }
        other => panic!("expected CorruptSource, got {other:?}"),
    }
    // Source is preserved (never deleted by the migration).
    assert!(source_path.exists());
}

#[test]
fn unknown_legacy_schema_version_refuses_migration() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut state = fixture_legacy();
    state.schema_version = 99; // newer-than-known
    write_legacy(&ctx, &state);

    let err = migrate_legacy_watch_state(&ctx).unwrap_err();
    match err {
        MigrationError::UnknownLegacySchema {
            found, expected, ..
        } => {
            assert_eq!(found, 99);
            assert_eq!(expected, 1);
        }
        other => panic!("expected UnknownLegacySchema, got {other:?}"),
    }
    // Source preserved; no autopilot session was created.
    assert!(mp::watch::default_state_path(&ctx.plan_dir).exists());
    let session_path = mp::autopilot::session::SessionPath::new(&ctx, MIGRATED_SESSION_ID).unwrap();
    assert!(!session_path.file.exists());
}

#[test]
fn partial_migration_leaves_existing_session_unchanged() {
    // AC-04: "leaves both the legacy source and any existing
    // autopilot session unchanged" when input is malformed.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());

    // Pre-seed the migrated session with a known good sample so
    // the next run cannot overwrite it.
    let pre_existing = sample_session_for_tests(MIGRATED_SESSION_ID);
    save_session(&ctx, MIGRATED_SESSION_ID, &pre_existing).unwrap();
    let pre_bytes = mp::autopilot::load_session(&ctx, MIGRATED_SESSION_ID).unwrap();

    // Write a corrupt legacy file alongside.
    let source_path = mp::watch::default_state_path(&ctx.plan_dir);
    std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    std::fs::write(&source_path, b"\xff\xfe\xfd garbage").unwrap();

    let err = migrate_legacy_watch_state(&ctx).unwrap_err();
    assert!(matches!(err, MigrationError::CorruptSource { .. }));

    // Pre-existing session is unchanged.
    let post_bytes = mp::autopilot::load_session(&ctx, MIGRATED_SESSION_ID).unwrap();
    assert_eq!(pre_bytes.queue, post_bytes.queue);
    assert_eq!(pre_bytes.last_updated, post_bytes.last_updated);
}

#[test]
fn cli_migrate_emits_typed_outcome_as_json() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    write_legacy(&ctx, &fixture_legacy());

    let out = env.run(&["autopilot", "migrate", "--format", "json"]);
    assert!(
        out.status.success(),
        "migrate should exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["ok"], Value::Bool(true));
    assert_eq!(parsed["dry_run"], Value::Bool(false));
    let outcome = &parsed["outcome"];
    assert_eq!(outcome["kind"], "migrated");
    assert_eq!(outcome["session_id"], MIGRATED_SESSION_ID);
    assert_eq!(outcome["migrated_milestones"], 2);
    assert_eq!(outcome["migrated_panes"], 2);
}

#[test]
fn cli_migrate_dry_run_does_not_write() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    write_legacy(&ctx, &fixture_legacy());

    let out = env.run(&["autopilot", "migrate", "--dry-run", "--format", "json"]);
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["ok"], Value::Bool(true));
    assert_eq!(parsed["dry_run"], Value::Bool(true));
    let outcome = &parsed["outcome"];
    assert_eq!(outcome["kind"], "would_migrate");
    assert_eq!(outcome["milestones"], 2);
    assert_eq!(outcome["panes"], 2);

    // No autopilot session was written.
    let session_path = mp::autopilot::session::SessionPath::new(&ctx, MIGRATED_SESSION_ID).unwrap();
    assert!(!session_path.file.exists());
}

#[test]
fn cli_session_list_picks_up_migrated_session() {
    // AC-03: `mp autopilot session list` surfaces the migrated
    // session when a legacy file is present.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    write_legacy(&ctx, &fixture_legacy());

    let out = env.run(&["autopilot", "session", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "session list should exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let sessions = parsed["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .map(|s| s["id"].as_str().unwrap_or(""))
        .collect();
    assert!(
        ids.contains(&MIGRATED_SESSION_ID),
        "session list should contain migrated id {MIGRATED_SESSION_ID}; got {ids:?}"
    );
}
