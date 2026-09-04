use crate::common::TestEnv;
use std::path::PathBuf;

/// status json includes can_handoff.
#[test]
fn status_includes_can_handoff() {
    let env = TestEnv::new();

    let out = env.run(&["status", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["execution"]["can_handoff"].is_boolean(),
        "status should include can_handoff"
    );
}

/// Install a fake `herdr` on PATH that satisfies the herdr_cli_shape
/// precondition (prints `--kind` and `--pane` from `agent start --help`)
/// without performing any real work. Returns the PATH the caller should
/// pass via `env.run_with_env(&[("PATH", &path)], …)` so the
/// `command_on_path("herdr")` check inside `execution_check` finds it.
///
/// M197 F-07: `can_handoff` now also requires watch readiness. Tests
/// that previously asserted `can_handoff == true` under the planning-
/// mode exemption must now set up the full happy path — fake herdr
/// on PATH plus `agent.runner.harness` / `agent.coordinator.harness`
/// configured — for the gate to go green.
fn install_fake_herdr_for_preconditions(env: &TestEnv) -> String {
    use std::fs;
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join("herdr");
    let body = r#"#!/bin/sh
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
"#;
    fs::write(&bin, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
    }
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, bin_dir.clone());
    std::env::join_paths(parts)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// Configure the harness slot so the role_config_present
/// preconditions are green. Without this the `runner_config_present`
/// / `coordinator_config_present` checks fail and `can_handoff` is
/// blocked regardless of the herdr binary on PATH.
fn configure_harness(env: &TestEnv) {
    assert!(env
        .run(&["config", "set", "agent.runner.harness", "opencode"])
        .status
        .success());
    assert!(env
        .run(&["config", "set", "agent.coordinator.harness", "opencode"])
        .status
        .success());
}

/// Negative fixture for F-07: with watch readiness red (no fake herdr),
/// `can_handoff` must be false even when there is a decomposed
/// milestone ready to hand off. This pins the gate parity contract
/// the new `execution_handoff` path relies on.
#[test]
fn execution_check_can_handoff_false_when_autopilot_readiness_red() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "Ready milestone",
        "intent": { "outcome": "Do something." },
        "problem": { "description": "Need to do something." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Must work", "verification": "cargo test" }
        ]
    }"#;

    env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    env.run(&["milestone", "approve", "01", "--format", "json"]);
    env.run(&["milestone", "decompose", "01", "--format", "json"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "do it",
        "--done-when",
        "done",
        "--tests",
        "test_foo",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);

    // No fake herdr, no harness config: watch readiness red.
    let out = env.run(&["execution", "check", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let watch_ok = json["autopilot_readiness"]["ok"].as_bool().unwrap_or(true);
    assert!(
        !watch_ok,
        "watch readiness should be red without fake herdr: {json}"
    );
    assert_eq!(
        json["can_handoff"],
        serde_json::Value::Bool(false),
        "can_handoff must be false when watch readiness is red (F-07 / Issue E): {json}"
    );
}

/// Positive fixture for F-07: with watch readiness green (fake herdr on
/// PATH plus harness config), `can_handoff` is true for a project
/// that has decomposed milestones ready to execute.
#[test]
fn execution_check_can_handoff_with_decomposed() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "Ready milestone",
        "intent": { "outcome": "Do something." },
        "problem": { "description": "Need to do something." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Must work", "verification": "cargo test" }
        ]
    }"#;

    env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    env.run(&["milestone", "approve", "01", "--format", "json"]);
    env.run(&["milestone", "decompose", "01", "--format", "json"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "do it",
        "--done-when",
        "done",
        "--tests",
        "test_foo",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);

    // M197 F-07: full happy path requires fake herdr on PATH plus
    // harness config so the role_config_present + herdr_on_path +
    // herdr_cli_shape preconditions all go green.
    configure_harness(&env);
    let path = install_fake_herdr_for_preconditions(&env);
    let out = env.run_with_env(
        &[("PATH", &path)],
        &["execution", "check", "--format", "json"],
    );
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let watch_ok = json["autopilot_readiness"]["ok"].as_bool().unwrap_or(false);
    assert!(
        watch_ok,
        "watch readiness should be green with fake herdr + harness config: {json}"
    );
    assert_eq!(
        json["can_handoff"],
        serde_json::Value::Bool(true),
        "decomposed milestone + green watch readiness should enable can_handoff: {json}"
    );
}
