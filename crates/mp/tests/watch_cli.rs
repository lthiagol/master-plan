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
