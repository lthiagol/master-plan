//! M207 / S03 / AC-03: `mp autopilot session list` and
//! `mp autopilot session show <id>`.
//!
//! Black-box coverage of the CLI surface:
//! - `mp autopilot session list` emits an array of `{id, status,
//!   last_updated}` summaries.
//! - `mp autopilot session show <id>` renders the full session.json
//!   in canonical view (the typed struct).
//! - Both commands go through the bounded-read primitive and the
//!   embedded schema validator (a malformed session.json surfaces as
//!   a JSON parse error, not a panic).

mod common;

use common::TestEnv;
use mp::autopilot::{
    list_sessions, load_session, sample_session_for_tests, save_session, AutopilotSession,
    SessionListEntry,
};
use mp::paths::PlanContext;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn list_cli_emits_id_status_last_updated() {
    let env = TestEnv::new();
    let session = sample_session_for_tests("alpha");
    let ctx = ctx_in(env.tmp.path());
    save_session(&ctx, "alpha", &session).unwrap();
    save_session(&ctx, "beta", &sample_session_for_tests("beta")).unwrap();

    let out = env.run(&["autopilot", "session", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["ok"], Value::Bool(true));
    let sessions = parsed["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2);
    // Sort order is by id (alpha, beta).
    assert_eq!(sessions[0]["id"], "alpha");
    assert_eq!(sessions[1]["id"], "beta");
    // Required fields present.
    for entry in sessions {
        assert!(entry.get("id").is_some());
        assert!(entry.get("status").is_some());
        assert!(entry.get("last_updated").is_some());
    }
}

#[test]
fn show_cli_renders_full_session() {
    let env = TestEnv::new();
    let session = sample_session_for_tests("alpha");
    let ctx = ctx_in(env.tmp.path());
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&["autopilot", "session", "show", "alpha", "--format", "json"]);
    assert!(
        out.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["ok"], Value::Bool(true));
    assert_eq!(parsed["session_id"], "alpha");
    let session_obj = &parsed["session"];
    // Spec calls out: 3-pane topology, role configs, queue with
    // evidence_refs, role_state, working_on.
    assert!(session_obj["topology"]["orchestrator"].is_object());
    assert!(session_obj["topology"]["runner"].is_object());
    assert!(session_obj["topology"]["reviewer"].is_object());
    assert!(session_obj["roles"]["orchestrator"].is_object());
    assert_eq!(session_obj["queue"].as_array().unwrap().len(), 2);
    for item in session_obj["queue"].as_array().unwrap() {
        assert!(item["evidence_refs"].is_object());
    }
    assert!(session_obj["role_state"].is_object());
}

#[test]
fn show_cli_returns_error_for_missing_session() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "session", "show", "does-not-exist"]);
    assert!(!out.status.success());
}

#[test]
fn show_cli_refuses_to_load_schema_invalid_session() {
    let env = TestEnv::new();
    // Build a session.json directly that *fails* validation —
    // missing the `id` field — then try to show it. The CLI must
    // surface a JSON parse / validation error, not crash.
    let autopilot = env.tmp.path().join("master-plan/autopilot/broken");
    fs::create_dir_all(&autopilot).unwrap();
    let bad = r#"{
        "schema_version": 1,
        "topology": {},
        "roles": {},
        "queue": [],
        "status": "draft",
        "last_updated": "2026-01-01T00:00:00Z"
    }"#;
    fs::write(autopilot.join("session.json"), bad).unwrap();
    let out = env.run(&["autopilot", "session", "show", "broken"]);
    assert!(!out.status.success());
}

#[test]
fn list_lib_helper_sorts_and_skips_malformed() {
    // Library-level list helper (consumed by the CLI dispatch).
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    save_session(&ctx, "zeta", &sample_session_for_tests("zeta")).unwrap();
    save_session(&ctx, "alpha", &sample_session_for_tests("alpha")).unwrap();
    // Drop a directory without a session.json — must be skipped.
    fs::create_dir_all(ctx.plan_dir.join("autopilot/empty")).unwrap();

    let list: Vec<SessionListEntry> = list_sessions(&ctx).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "alpha");
    assert_eq!(list[1].id, "zeta");
}

#[test]
fn load_session_helper_round_trips_with_lib_api() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let original: AutopilotSession = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &original).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    let mut expected = original;
    expected.last_updated = loaded.last_updated.clone();
    assert_eq!(loaded, expected);
}