//! M152 S1 / AC-01: crash-safe `.mp/watch.state.json` for `mp watch`.
//!
//! Black-box coverage of the on-disk shape:
//! - state file path is `<plan_dir>/.mp/watch.state.json`
//! - JSON shape: `schema_version`, `pid`, `started_at`,
//!   `last_updated_at`, `panes`, `milestones`
//! - corrupting the file mid-write must not corrupt the published
//!   document (atomic write through temp+rename)
//! - loading a missing or torn file is non-fatal; `mp watch
//!   --resume` falls through to a fresh spawn rather than crashing

mod common;

use common::TestEnv;
use mp::watch::{
    default_run_state_path, LifecycleTarget, MilestoneState, PaneState, PromptStage, Role,
    RunOutcome, WatchRunState, WatchRunStore, WatchState, WatchTransition,
    WATCH_RUN_STATE_SCHEMA_VERSION, WATCH_STATE_SCHEMA_VERSION,
};
use std::fs;
use std::path::Path;

fn state_path(env: &TestEnv) -> std::path::PathBuf {
    WatchState::path_for(&env.tmp.path().join("master-plan"))
}

fn load_state(env: &TestEnv) -> Option<WatchState> {
    WatchState::load_from(&state_path(env)).unwrap()
}

#[test]
fn state_path_default_is_under_mp_subdir_of_plan_dir() {
    let env = TestEnv::new();
    let p = state_path(&env);
    assert!(p.ends_with(".mp/watch.state.json"), "got {}", p.display());
}

#[test]
fn fresh_state_has_schema_v1_and_current_pid() {
    let _env = TestEnv::new();
    let s = WatchState::fresh(&["M152".to_string()]);
    assert_eq!(s.schema_version, WATCH_STATE_SCHEMA_VERSION);
    assert_eq!(s.pid, std::process::id());
    assert!(s.started_at.contains('T'));
    // chrono's `to_rfc3339()` defaults to `+00:00`. Some
    // implementations emit `Z`. Both are valid RFC3339 UTC
    // suffixes — accept either.
    assert!(
        s.started_at.ends_with('Z') || s.started_at.ends_with("+00:00"),
        "ts must end with Z or +00:00; got {}",
        s.started_at
    );
}

#[test]
fn save_creates_parent_dir_when_missing() {
    let env = TestEnv::new();
    let path = state_path(&env);
    assert!(
        !path.parent().unwrap().exists(),
        ".mp/ should not yet exist on a fresh init"
    );
    let s = WatchState::fresh(&["M152".into()]);
    s.save(&path).unwrap();
    assert!(path.is_file(), "save must persist the state file");
    assert!(path.parent().unwrap().is_dir(), ".mp/ must be created");
}

#[test]
fn load_returns_none_when_state_file_is_absent() {
    let env = TestEnv::new();
    assert!(!state_path(&env).exists());
    assert!(load_state(&env).is_none());
}

#[test]
fn save_then_load_round_trips_losslessly() {
    let env = TestEnv::new();
    let mut s = WatchState::fresh(&["M152".to_string()]);
    s.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%5".into(),
        spawned_at: "2026-01-01T00:00:00Z".into(),
        last_status: Some("working".into()),
    });
    s.upsert_milestone(MilestoneState {
        id: "M152".into(),
        last_lifecycle: "in-progress".into(),
        target_lifecycle: "self-reviewed".into(),
        last_action_at: "2026-01-01T00:00:01Z".into(),
    });
    let path = state_path(&env);
    s.save(&path).unwrap();
    let loaded = load_state(&env).expect("state file must load");
    assert_eq!(loaded, s);
}

#[test]
fn save_overwrites_in_place_not_appends() {
    // Two consecutive saves must produce a single coherent document
    // (no clobber / no duplicate entries).
    let env = TestEnv::new();
    let path = state_path(&env);
    let mut a = WatchState::fresh(&["M152".into()]);
    a.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%5".into(),
        spawned_at: "t1".into(),
        last_status: None,
    });
    a.save(&path).unwrap();

    let mut b = WatchState::fresh(&["M152".into()]);
    b.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%9".into(),
        spawned_at: "t2".into(),
        last_status: None,
    });
    b.save(&path).unwrap();

    let loaded = load_state(&env).expect("state loads");
    assert_eq!(loaded.panes.len(), 1, "no duplication across re-saves");
    assert_eq!(loaded.panes[0].pane_id, "%9", "re-save replaces");
}

#[test]
fn save_is_atomic_no_torn_file() {
    // `atomic_write` uses temp + rename; the destination file must
    // always parse as a complete document when it exists. We can't
    // simulate a true SIGKILL from a unit test, so we approximate
    // the contract by ensuring the published document is a valid
    // JSON parse after every save.
    let env = TestEnv::new();
    let path = state_path(&env);
    for i in 0..10 {
        let mut s = WatchState::fresh(&[format!("M{i}")]);
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: format!("%{i}"),
            spawned_at: "t".into(),
            last_status: None,
        });
        s.save(&path).unwrap();
        let raw = std::fs::read(&path).unwrap();
        let _: serde_json::Value =
            serde_json::from_slice(&raw).expect("published state must parse");
    }
}

#[test]
fn load_returns_none_on_corrupt_state_file() {
    let env = TestEnv::new();
    let path = state_path(&env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json {{{ torn").unwrap();
    // --resume must not crash on a torn file. It falls through to
    // spawn-normal and leaves the corrupt file in place for
    // forensics.
    assert!(load_state(&env).is_none());
    assert!(
        path.is_file(),
        "corrupt state file is preserved for diagnosis"
    );
}

#[test]
fn load_returns_none_when_schema_version_is_unknown() {
    let env = TestEnv::new();
    let path = state_path(&env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let raw = r#"{
        "schema_version": 99,
        "pid": 1,
        "started_at": "t",
        "last_updated_at": "t",
        "panes": [],
        "milestones": []
    }"#;
    std::fs::write(&path, raw).unwrap();
    assert!(
        load_state(&env).is_none(),
        "schema-incompatible file must be ignored"
    );
}

#[test]
fn upsert_pane_replaces_same_role() {
    let _env = TestEnv::new();
    let mut s = WatchState::fresh(&["M152".into()]);
    s.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%5".into(),
        spawned_at: "t".into(),
        last_status: None,
    });
    s.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%9".into(),
        spawned_at: "t".into(),
        last_status: None,
    });
    assert_eq!(s.panes.len(), 1);
    assert_eq!(s.pane_for(Role::Runner).unwrap().pane_id, "%9");
}

#[test]
fn pane_and_milestone_lookups_return_none_when_missing() {
    let s = WatchState::fresh(&[]);
    assert!(s.pane_for(Role::Runner).is_none());
    assert!(s.pane_for(Role::Coordinator).is_none());
    assert!(s.milestone("M999").is_none());
}

#[test]
fn state_file_role_serializes_as_kebab_case_string() {
    let env = TestEnv::new();
    let mut s = WatchState::fresh(&["M152".into()]);
    s.upsert_pane(PaneState {
        role: Role::Coordinator,
        label: "role-coordinator-1".into(),
        pane_id: "%7".into(),
        spawned_at: "t".into(),
        last_status: None,
    });
    let path = state_path(&env);
    s.save(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("\"role\": \"coordinator\""),
        "role must serialize kebab-case; got: {raw}"
    );
}

#[test]
fn state_file_path_matches_default_path_helper() {
    let env = TestEnv::new();
    let dir = env.tmp.path().join("master-plan");
    assert_eq!(
        WatchState::path_for(&dir),
        mp::watch::default_state_path(&dir)
    );
}

#[test]
fn last_updated_at_is_bumped_on_upsert() {
    let env = TestEnv::new();
    let path = state_path(&env);
    let mut s = WatchState::fresh(&["M152".into()]);
    let first = s.last_updated_at.clone();
    s.upsert_pane(PaneState {
        role: Role::Runner,
        label: "x".into(),
        pane_id: "y".into(),
        spawned_at: "z".into(),
        last_status: None,
    });
    let second = s.last_updated_at.clone();
    assert!(second >= first, "{first} → {second}");
    s.save(&path).unwrap();
    let reloaded = WatchState::load_from(&path).unwrap().unwrap();
    assert_eq!(reloaded.last_updated_at, second);
}

#[test]
fn fresh_state_with_no_milestones_has_empty_arrays() {
    let s = WatchState::fresh(&[]);
    assert!(s.panes.is_empty());
    assert!(s.milestones.is_empty());
    // Round-trip via serde to confirm the empty arrays serialize
    // correctly (a `#[serde(default)]` would otherwise elide them).
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["panes"].as_array().unwrap().len(), 0);
    assert_eq!(v["milestones"].as_array().unwrap().len(), 0);
}

#[test]
#[allow(non_snake_case)]
fn M152_state_file_lives_in_mp_subdir_of_plan_dir() {
    // Defensive pin: catching a future refactor that puts the file
    // somewhere else (e.g. the project root) is a silent operability
    // regression — resume, crash recovery, and forensic tail all
    // depend on the canonical `.mp/` location.
    let env = TestEnv::new();
    let canonical = env.tmp.path().join("master-plan/.mp/watch.state.json");
    assert_eq!(state_path(&env), canonical);
    // The path also surfaces through the in-memory helper used by
    // `mp watch` startup.
    let dir = env.tmp.path().join("master-plan");
    let computed = Path::new(&dir).join(".mp").join("watch.state.json");
    assert_eq!(canonical, computed);
}

// ─── M178 S1+S2: v2 control-plane state contract (AC-01, AC-06) ──────

fn run_state_path(env: &TestEnv) -> std::path::PathBuf {
    default_run_state_path(&env.tmp.path().join("master-plan"))
}

#[test]
fn v2_fresh_state_has_schema_v2_and_empty_control_fields() {
    let _env = TestEnv::new();
    let s = WatchRunState::fresh(&["170".into()]);
    assert_eq!(s.schema_version, WATCH_RUN_STATE_SCHEMA_VERSION);
    assert_eq!(s.schema_version, 2);
    assert_eq!(s.pid, std::process::id());
    assert_eq!(s.queue, vec!["170"]);
    assert!(s.active_queue_index.is_none());
    assert!(s.active_milestone.is_none());
    assert!(s.run_outcome.is_none());
    assert!(s.pane_ids.is_empty());
    // Legacy v1 panes/milestones preserved.
    assert_eq!(s.milestones.len(), 1);
}

#[test]
fn v2_state_round_trip_preserves_ac01_contract_fields() {
    let env = TestEnv::new();
    let path = run_state_path(&env);
    let mut s = WatchRunState::fresh(&["170".to_string(), "171".to_string()]);
    s.set_active_milestone(0, "170");
    s.set_current_lifecycle("in-progress");
    s.watch_stage = Some("execute".into());
    s.target_lifecycle = Some("self-reviewed".into());
    s.active_role = Some(Role::Runner);
    s.record_pane(Role::Runner, "%5");
    s.record_pane(Role::Coordinator, "%7");
    s.log_path = Some("/tmp/watch.log".into());
    s.state_path = Some(path.display().to_string());
    s.push_milestone_outcome(mp::watch::MilestoneRunOutcome {
        id: "170".into(),
        outcome: RunOutcome::Completed,
    });
    s.save(&path).unwrap();
    let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.queue, vec!["170", "171"]);
    assert_eq!(loaded.active_milestone.as_deref(), Some("170"));
    assert_eq!(loaded.current_lifecycle.as_deref(), Some("in-progress"));
    assert_eq!(loaded.watch_stage.as_deref(), Some("execute"));
    assert_eq!(loaded.target_lifecycle.as_deref(), Some("self-reviewed"));
    assert_eq!(loaded.active_role, Some(Role::Runner));
    assert_eq!(loaded.pane_ids.get(&Role::Runner).unwrap(), "%5");
    assert_eq!(loaded.pane_ids.get(&Role::Coordinator).unwrap(), "%7");
    assert_eq!(loaded.milestone_outcomes.len(), 1);
    // Terminal outcome NOT yet set — sequencer runs to completion
    // stamp `RunOutcome::Completed` post-loop.
    assert!(loaded.run_outcome.is_none());
}

#[test]
fn v2_state_with_terminal_outcome_round_trips() {
    let env = TestEnv::new();
    let path = run_state_path(&env);
    let mut s = WatchRunState::fresh(&["170".to_string()]);
    s.set_run_outcome(RunOutcome::GracefullyStopped);
    s.push_milestone_outcome(mp::watch::MilestoneRunOutcome {
        id: "170".into(),
        outcome: RunOutcome::GracefullyStopped,
    });
    s.save(&path).unwrap();
    let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(
        loaded.run_outcome.as_ref().unwrap().label(),
        "gracefully-stopped"
    );
    assert!(loaded.active_milestone.is_none());
    assert!(loaded.watch_stage.is_none());
}

#[test]
fn durable_transition_publishes_each_generation_atomically() {
    let env = TestEnv::new();
    let path = run_state_path(&env);
    let state = WatchRunState::fresh(&["170".to_string()]);
    state.save(&path).unwrap();
    let mut store = WatchRunStore::new(path.clone(), state);

    store
        .transition(WatchTransition::ActiveMilestone {
            index: 0,
            id: "170".to_string(),
        })
        .unwrap();
    let first = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.active_milestone.as_deref(), Some("170"));

    store
        .transition(WatchTransition::ActiveStage {
            stage: PromptStage::Execute,
            target: LifecycleTarget::Complete,
        })
        .unwrap();
    let second = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(second.generation, 2);
    assert_eq!(second.watch_stage.as_deref(), Some("execute"));
    assert_eq!(second.target_lifecycle.as_deref(), Some("complete"));
}

#[test]
fn run_outcome_transition_does_not_overwrite_newer_terminal() {
    // M190 F-01: stop's GracefullyStopped must not clobber a Completed
    // that landed under the lock between stop's pre-check and transition.
    let env = TestEnv::new();
    let path = run_state_path(&env);
    let pre_outcome = WatchRunState::fresh(&["170".to_string()]);
    pre_outcome.save(&path).unwrap();

    let mut winner = WatchRunStore::new(path.clone(), pre_outcome.clone());
    winner
        .transition(WatchTransition::RunOutcome(RunOutcome::Completed))
        .unwrap();

    // Stale stop-style store: constructed from the pre-outcome snapshot.
    let mut late_stop = WatchRunStore::new(path.clone(), pre_outcome);
    late_stop
        .transition(WatchTransition::RunOutcome(RunOutcome::GracefullyStopped))
        .unwrap();

    let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(
        loaded.run_outcome.as_ref().map(RunOutcome::label),
        Some("completed"),
        "Completed must survive a late GracefullyStopped transition"
    );
    assert_eq!(
        loaded.generation, 1,
        "no-op overwrite must not bump generation"
    );
}

#[test]
fn v2_load_accepts_v1_file_via_migration() {
    let env = TestEnv::new();
    let path = run_state_path(&env);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let v1_json = serde_json::json!({
        "schema_version": 1,
        "pid": 99999,
        "started_at": "2026-07-01T00:00:00+00:00",
        "last_updated_at": "2026-07-01T00:00:00+00:00",
        "panes": [{
            "role": "runner",
            "label": "role-runner-1",
            "pane_id": "%5",
            "spawned_at": "2026-07-01T00:00:00+00:00"
        }],
        "milestones": [{
            "id": "170",
            "last_lifecycle": "approved",
            "target_lifecycle": "in-progress",
            "last_action_at": "2026-07-01T00:00:00+00:00"
        }]
    });
    fs::write(&path, serde_json::to_vec_pretty(&v1_json).unwrap()).unwrap();
    let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.pid, 99999);
    assert_eq!(loaded.queue, vec!["170"]);
    assert_eq!(loaded.panes.len(), 1);
    // No v2 control fields fabricated.
    assert!(loaded.active_milestone.is_none());
    assert!(loaded.run_outcome.is_none());
}

#[test]
fn v1_load_accepts_v2_file_for_resume_reconciliation() {
    // AC-08 backcompat: the legacy M152 test surface calls
    // `WatchState::load_from` to read panes/milestones for
    // reconciliation. The v2 file has all v1 fields plus extras;
    // the legacy loader must accept it.
    let env = TestEnv::new();
    let path = state_path(&env);
    // Hand-craft a v2 file with both panes AND milestones populated
    // (mirrors the on-disk shape the running driver writes):
    let v2_json = serde_json::json!({
        "schema_version": 2,
        "pid": 12345,
        "started_at": "2026-07-01T00:00:00+00:00",
        "last_updated_at": "2026-07-01T00:00:00+00:00",
        "queue": ["170"],
        "panes": [{
            "role": "runner",
            "label": "role-runner-1",
            "pane_id": "%5",
            "spawned_at": "2026-07-01T00:00:00+00:00"
        }],
        "milestones": [{
            "id": "170",
            "last_lifecycle": "approved",
            "target_lifecycle": "in-progress",
            "last_action_at": "2026-07-01T00:00:00+00:00"
        }]
    });
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&v2_json).unwrap()).unwrap();
    let loaded = WatchState::load_from(&path).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.panes.len(), 1);
    assert_eq!(loaded.panes[0].role, Role::Runner);
    assert_eq!(loaded.milestones[0].id, "170");
}
