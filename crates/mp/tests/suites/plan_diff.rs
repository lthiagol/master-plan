//! mp plan diff and handoff show (M70).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::common::lib_api;
use crate::common::TestEnv;

fn git_init_and_commit(cwd: &std::path::Path, message: &str) {
    for args in [
        &["init"][..],
        &["config", "user.email", "test@example.com"][..],
        &["config", "user.name", "test"][..],
        &["add", "-A"][..],
        &["commit", "-m", message][..],
    ] {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git");
        if args.first() == Some(&"commit") {
            assert!(
                out.status.success(),
                "git commit: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

fn seed_handoff_gate(env: &TestEnv) {
    let create_json = r#"{
        "title": "Handoff gate",
        "intent": { "outcome": "Enable handoff." },
        "problem": { "description": "Need handoff gate." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: ok" }
        ]
    }"#;
    assert!(
        lib_api::run(env, &["milestone", "create", "--json", create_json])
            .status
            .success(),
        "create handoff gate"
    );
    assert!(lib_api::run(env, &["milestone", "approve", "01"])
        .status
        .success());
    assert!(lib_api::run(env, &["milestone", "decompose", "01"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            "01",
            "--wp",
            "WP1",
            "--action",
            "step",
            "--done-when",
            "done",
            "--tests",
            "manual: ok",
            "--covers-ac",
            "AC-01",
        ])
        .status
        .success());
}

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
            "title": "{title}",
            "intent": {{ "outcome": "x" }},
            "problem": {{ "description": "y" }},
            "scope": {{ "in_scope": ["a"], "out_of_scope": ["b", "c"] }},
            "acceptance_criteria": [
                {{ "description": "ac", "verification": "manual: ok" }}
            ]
        }}"#
    );
    let create = lib_api::run(env, &["milestone", "create", "--json", &create_json]);
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// M197 F-07: `execution handoff` now also requires watch readiness.
/// Install a fake `herdr` on PATH (the precondition's
/// `command_on_path("herdr")` + `herdr_cli_shape` probes both pass
/// against the fake) and configure the harness slots so the
/// `role_config_present` checks are green. The fake does NOT need to
/// run `agent start --help` for real — the help text just has to
/// list `--kind` and `--pane`.
fn enable_autopilot_readiness_for_handoff(env: &TestEnv) {
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_herdr = bin_dir.join("herdr");
    fs::write(
        &fake_herdr,
        r#"#!/bin/sh
case "$1:$2:$3" in
  agent:start:--help)
    cat <<'HELP'
Usage: herdr agent start <NAME> --kind <KIND> --pane <ID>

Options:
  --kind <KIND>  Harness kind
  --pane <ID>    Existing pane id
HELP
    ;;
  pane:split:--help)
    echo "Usage: herdr pane split [OPTIONS]"
    ;;
  *)
    echo ok
    ;;
esac
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake_herdr).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_herdr, perms).unwrap();
    }

    assert!(
        lib_api::run(env, &["config", "set", "agent.runner.harness", "opencode"])
            .status
            .success()
    );
    assert!(lib_api::run(
        env,
        &["config", "set", "agent.coordinator.harness", "opencode"]
    )
    .status
    .success());

    let prev_path = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&prev_path).collect();
    parts.insert(0, bin_dir);
    std::env::set_var("PATH", std::env::join_paths(parts).unwrap());
    // NB: PATH override is process-global; the test process exits after
    // each test so we do not restore here.
}

fn do_handoff(env: &TestEnv) {
    enable_autopilot_readiness_for_handoff(env);
    let out = lib_api::run(env, &["execution", "handoff"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn plan_diff_since_handoff_lists_changed_milestones() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);
    let id = create_milestone(&env, "Diff fixture");

    let approve = lib_api::run(&env, &["milestone", "approve", &id]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );

    let diff = lib_api::run(
        &env,
        &["plan", "diff", "--since-handoff", "--format", "json"],
    );
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(json["clean"], false);
    let changed = json["changed_milestones"].as_array().unwrap();
    assert!(
        changed
            .iter()
            .any(|m| m["id"].as_str() == Some(id.as_str())),
        "expected milestone {id} in diff: {json}"
    );
}

#[test]
fn plan_diff_clean_tree_exits_ok() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);
    let diff = lib_api::run(
        &env,
        &["plan", "diff", "--since-handoff", "--format", "json"],
    );
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(json["clean"], true);
    assert!(json["plan_changes"]
        .as_array()
        .unwrap_or(&vec![])
        .is_empty());
    assert!(json["changed_milestones"].as_array().unwrap().is_empty());
}

#[test]
fn plan_diff_since_handoff_ignores_mtime_only_touch() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);

    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    let milestone_path = std::fs::read_dir(&milestones_dir)
        .expect("read milestones")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("milestone file");
    let touch = Command::new("touch")
        .arg(&milestone_path)
        .status()
        .expect("touch");
    assert!(touch.success(), "touch milestone file");

    let diff = lib_api::run(
        &env,
        &["plan", "diff", "--since-handoff", "--format", "json"],
    );
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(
        json["clean"], true,
        "mtime-only touch must not appear in baseline diff: {json}"
    );
}

#[test]
fn plan_diff_since_handoff_reports_field_level_from_to() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    let id = create_milestone(&env, "Field diff");
    do_handoff(&env);
    lib_api::run(&env, &["milestone", "approve", &id]);

    let diff = lib_api::run(
        &env,
        &["plan", "diff", "--since-handoff", "--format", "json"],
    );
    assert!(diff.status.success());
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    let entry = json["changed_milestones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"].as_str() == Some(id.as_str()))
        .expect("changed milestone");
    let spec_change = entry["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["field"] == "milestone.spec_status")
        .expect("spec_status change");
    assert_eq!(spec_change["from"], "draft");
    assert_eq!(spec_change["to"], "ready");
}

#[test]
fn plan_diff_markdown_includes_heading() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);
    let id = create_milestone(&env, "Markdown diff");
    lib_api::run(&env, &["milestone", "approve", &id]);

    let diff = lib_api::run(
        &env,
        &[
            "plan",
            "diff",
            "--since-handoff",
            "--markdown",
            "--format",
            "json",
        ],
    );
    assert!(diff.status.success());
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    let md = json["markdown"].as_str().expect("markdown field");
    assert!(md.contains("# Plan diff since"));
    assert!(md.contains("Markdown diff"));
}

#[test]
fn handoff_records_changed_milestones_and_show_returns_them() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);
    let id = create_milestone(&env, "Handoff touch");
    lib_api::run(&env, &["milestone", "approve", &id]);

    let handoff2 = lib_api::run(&env, &["execution", "handoff"]);
    assert!(
        handoff2.status.success(),
        "{}",
        String::from_utf8_lossy(&handoff2.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&handoff2.stdout).unwrap();
    let changed = payload["changed_milestone_ids"]
        .as_array()
        .expect("changed ids");
    assert!(
        changed.iter().any(|v| v.as_str() == Some(id.as_str())),
        "handoff should record changed milestone: {payload}"
    );

    let show = lib_api::run(&env, &["execution", "handoff-show", "--format", "json"]);
    assert!(show.status.success());
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert!(show_json["changed_milestone_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(id.as_str())));
}

#[test]
fn plan_diff_git_ref_reports_field_changes() {
    let env = TestEnv::new();
    git_init_and_commit(env.tmp.path(), "baseline");

    let id = create_milestone(&env, "Git diff");
    git_init_and_commit(env.tmp.path(), "add milestone");

    lib_api::run(&env, &["milestone", "approve", &id]);

    let diff = lib_api::run(
        &env,
        &["plan", "diff", "--git", "HEAD~1", "--format", "json"],
    );
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(json["clean"], false);
    let changed = json["changed_milestones"].as_array().unwrap();
    assert!(
        changed
            .iter()
            .any(|m| m["id"].as_str() == Some(id.as_str())),
        "git diff should include new milestone: {json}"
    );
}
