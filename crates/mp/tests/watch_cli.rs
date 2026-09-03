//! M149 S2 / AC-01: `mp watch <ids...>` CLI entry point.
//!
//! S2 contract:
//! - `mp watch --help` prints usage (verified by spawning the binary).
//! - `mp watch <ids...> --dry-run` resolves each id, reports
//!   lifecycle / spec / execution status, the next action the runner
//!   *would* take, and precondition failures — without modifying
//!   `plan.json` or spawning agents.
//! - Unknown milestone ids surface as per-entry errors, not panics.

mod common;

use crate::common::TestEnv;
use serde_json::Value;

fn watch(env: &TestEnv, args: &[&str]) -> Value {
    let mut full = vec!["watch"];
    full.extend(args.iter());
    full.push("--format");
    full.push("json");
    env.run_json(&full)
}

#[test]
fn watch_help_lists_usage() {
    let env = TestEnv::new();
    let out = env.run(&["watch", "--help"]);
    assert!(
        out.status.success(),
        "mp watch --help should exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("dry-run") && (stdout.contains("[IDS]") || stdout.contains("ids")),
        "expected usage to mention ids + dry-run; got: {stdout}"
    );
}

#[test]
fn watch_requires_at_least_one_id() {
    let env = TestEnv::new();
    let out = env.run(&["watch", "--format", "json"]);
    assert!(
        !out.status.success(),
        "mp watch with no ids should be a usage error"
    );
}

#[test]
fn dry_run_reports_preconditions_and_empty_milestone_list() {
    let env = TestEnv::new();
    let report = watch(&env, &["--dry-run"]);
    assert_eq!(report["dry_run"], serde_json::json!(true));
    assert!(report["preconditions"]["checks"].is_array());
    // Fresh project: role configs unset, so precondition ok must be false.
    assert_eq!(report["preconditions"]["ok"], serde_json::json!(false));
    assert!(report["milestones"].is_array());
}

#[test]
fn dry_run_surfaces_unknown_milestone_as_per_entry_error() {
    let env = TestEnv::new();
    let report = watch(&env, &["--dry-run", "999999"]);
    let milestones = report["milestones"].as_array().unwrap();
    assert_eq!(milestones.len(), 1, "exactly one entry for one input id");
    let entry = &milestones[0];
    assert_eq!(entry["input"], serde_json::json!("999999"));
    assert!(
        entry["error"].as_str().is_some(),
        "missing milestone should produce an error string, not a panic"
    );
    assert!(entry["id"].is_null());
}

#[test]
fn dry_run_resolves_known_milestone_state() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "watch dry-run target",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "test outcome for watch dry-run" },
        "problem": { "description": "test problem" },
        "scope": {
            "in_scope": ["one thing"],
            "out_of_scope": ["something else", "a third thing"]
        },
        "acceptance_criteria": [
            { "description": "it works", "verification": "manual: yes" }
        ]
    }"#;
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    let id = created["milestone"]["id"].as_str().expect("milestone id");

    let report = watch(&env, &["--dry-run", id]);
    let milestones = report["milestones"].as_array().unwrap();
    assert_eq!(milestones.len(), 1);
    let entry = &milestones[0];
    assert_eq!(entry["input"].as_str(), Some(id));
    assert_eq!(entry["id"].as_str(), Some(id));
    assert!(entry["title"].is_string());
    assert!(entry["lifecycle"].is_string());
    // A freshly-created milestone has lifecycle=draft — not ready.
    assert_eq!(entry["ready"], serde_json::json!(false));
    assert!(
        entry["next_action"].as_str().unwrap().starts_with("skip_"),
        "fresh draft milestone should route to a skip_* action: {}",
        entry["next_action"]
    );
}

#[test]
fn log_file_override_is_reflected_in_report() {
    let env = TestEnv::new();
    let custom = env.tmp.path().join("custom-watch.log");
    let report = watch(&env, &["--dry-run", "--log-file", custom.to_str().unwrap()]);
    assert_eq!(report["log_file"].as_str(), Some(custom.to_str().unwrap()));
}

#[test]
fn short_m_prefix_id_resolves_like_bare_id() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "M-prefixed id resolves",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "M-prefixed id resolves" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["y", "z"] },
        "acceptance_criteria": [
            { "description": "ac", "verification": "manual: yes" }
        ]
    }"#;
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    let id = created["milestone"]["id"].as_str().expect("milestone id");
    let prefixed = format!("M{id}");

    let report = watch(&env, &["--dry-run", &prefixed]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["id"].as_str(), Some(id));
    assert!(entry["error"].is_null());
}

// ─── M208 / S5 / AC-05: namespace guards between planning sessions
// and autopilot drive sessions. The two domains live under different
// directories (`<plan_dir>/sessions/<id>/` vs
// `<plan_dir>/autopilot/<id>/`); neither command may read or mutate
// the other session domain. ──────────────────────────────────────────

/// `mp autopilot session list` outputs go through the autopilot
/// directory; `mp session list` outputs go through the planning
/// directory. The two listings must not be cross-contaminated.
#[test]
fn autopilot_and_planning_sessions_are_separate_domains() {
    let env = TestEnv::new();

    // 1. Create one planning session via `mp session start`.
    let planning = env.run_json(&["session", "start", "--title", "plan-A"]);
    let planning_id = planning["session_id"]
        .as_str()
        .expect("planning session_id");

    // 2. Create one autopilot session via the autopilot CLI helper
    //    (write directly to the autopilot dir — the public CLI is
    //    exercised separately by autopilot_session_cli.rs).
    let ctx = mp::paths::PlanContext {
        project_root: env.tmp.path().to_path_buf(),
        plan_dir: env.tmp.path().join("master-plan"),
    };
    let drive_id = "drive-a";
    let drive = mp::autopilot::sample_session_for_tests(drive_id);
    mp::autopilot::save_session(&ctx, drive_id, &drive).unwrap();

    // 3. Both lists report success.
    let plan_list = env.run_json(&["session", "list"]);
    assert!(
        plan_list["sessions"].as_array().unwrap().len() >= 1,
        "planning list should contain the planning session"
    );

    let drive_list = env.run_json(&["autopilot", "session", "list"]);
    let drive_sessions = drive_list["sessions"].as_array().unwrap();
    let drive_ids: Vec<&str> = drive_sessions
        .iter()
        .map(|s| s["id"].as_str().unwrap_or(""))
        .collect();
    assert!(
        drive_ids.contains(&drive_id),
        "autopilot list should contain {drive_id}; got {drive_ids:?}"
    );

    // 4. Cross-domain ids must not be retrievable through the
    //    other command. `mp session show` against an autopilot id
    //    must fail (or surface a typed not-found diagnostic), and
    //    vice versa.
    let show_drive_via_plan = env.run(&["session", "show", drive_id]);
    assert!(
        !show_drive_via_plan.status.success(),
        "`mp session show {drive_id}` must NOT resolve an autopilot id"
    );

    let show_plan_via_drive = env.run(&["autopilot", "session", "show", planning_id]);
    assert!(
        !show_plan_via_drive.status.success(),
        "`mp autopilot session show {planning_id}` must NOT resolve a planning id"
    );
}

/// Storage paths: planning sessions live under
/// `<plan_dir>/sessions/<id>/session.json`; autopilot drive sessions
/// live under `<plan_dir>/autopilot/<id>/session.json`. The two paths
/// must never collide even when both ids happen to match.
#[test]
fn planning_and_autopilot_storage_paths_do_not_collide() {
    let env = TestEnv::new();
    let ctx = mp::paths::PlanContext {
        project_root: env.tmp.path().to_path_buf(),
        plan_dir: env.tmp.path().join("master-plan"),
    };
    // Use the SAME id in both domains to prove the storage paths
    // are not derived from the id alone.
    let shared_id = "shared-id";
    let planning_path = mp::store::session_dir(&ctx, shared_id)
        .unwrap()
        .join("session.json");
    let autopilot_path = mp::autopilot::session::SessionPath::new(&ctx, shared_id)
        .unwrap()
        .file;
    assert_ne!(
        planning_path, autopilot_path,
        "planning and autopilot sessions must not share an on-disk path"
    );
    assert!(
        planning_path.starts_with(ctx.plan_dir.join("sessions")),
        "planning session should live under <plan_dir>/sessions/: {planning_path:?}"
    );
    assert!(
        autopilot_path.starts_with(ctx.plan_dir.join("autopilot")),
        "autopilot session should live under <plan_dir>/autopilot/: {autopilot_path:?}"
    );
}

/// `mp autopilot --help` (the verb tree) and `mp session --help`
/// (the planning session tree) must have unambiguously distinct
/// top-level commands — neither command is a subcommand of the other.
#[test]
fn autopilot_help_does_not_advertise_planning_session_verbs() {
    let env = TestEnv::new();
    let autopilot_help =
        String::from_utf8_lossy(&env.run(&["autopilot", "--help"]).stdout).to_string();
    let session_help = String::from_utf8_lossy(&env.run(&["session", "--help"]).stdout).to_string();

    // Planning-only verbs (start/focus/unfocus/archive/promote)
    // should appear in `mp session --help`, not in the autopilot tree.
    for planning_only in ["focus", "unfocus", "archive", "promote"] {
        assert!(
            session_help.contains(planning_only),
            "planning session help should advertise {planning_only:?}"
        );
        assert!(
            !autopilot_help.contains(planning_only),
            "autopilot help must not advertise planning-only verb {planning_only:?}"
        );
    }
}
