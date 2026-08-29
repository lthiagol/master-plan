use crate::common::{lib_api, TestEnv};
use std::fs;
use std::path::PathBuf;

#[test]
fn path_pin_focus_plan_gaps_and_execution_check() {
    let env = TestEnv::from_fixture("linear-deps");

    // M197 F-07: `can_handoff` now also requires watch readiness, so
    // the smoke assertion at the end of this test needs a fake herdr
    // on PATH plus the harness slots configured. The fake herdr
    // answers `agent start --help` with a help text that lists
    // `--kind` and `--pane` so the herdr_cli_shape gate goes green.
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

    let pin = lib_api::run(
        &env,
        &["path", "pin", "03", "--before", "02", "--format", "json"],
    );
    assert!(
        pin.status.success(),
        "{}",
        String::from_utf8_lossy(&pin.stderr)
    );

    let focus = lib_api::run(&env, &["path", "focus", "02", "--format", "json"]);
    assert!(focus.status.success());

    let gaps = lib_api::run(&env, &["plan", "gaps", "02", "--format", "json"]);
    assert!(gaps.status.success());
    let gaps_json: serde_json::Value = serde_json::from_slice(&gaps.stdout).unwrap();
    assert_eq!(gaps_json["milestone_id"], "02");

    let steps = lib_api::run(
        &env,
        &["list", "steps", "--milestone", "02", "--format", "json"],
    );
    assert!(steps.status.success());
    let steps_json: serde_json::Value = serde_json::from_slice(&steps.stdout).unwrap();
    assert!(steps_json["steps"].as_array().unwrap().len() >= 2);

    let status_json = lib_api::run_json(&env, &["status", "--format", "json"]);
    assert!(status_json.get("suggested_path").is_some());

    // Configure harness so role_config_present checks are green, and
    // prepend the fake-bin dir to PATH so command_on_path("herdr")
    // finds the stub. Both must hold for F-07's tightened can_handoff
    // gate.
    assert!(
        lib_api::run(&env, &["config", "set", "agent.runner.harness", "opencode"],)
            .status
            .success()
    );
    assert!(lib_api::run(
        &env,
        &["config", "set", "agent.coordinator.harness", "opencode"],
    )
    .status
    .success());

    let prev_path = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&prev_path).collect();
    parts.insert(0, bin_dir.clone());
    let new_path = std::env::join_paths(parts).unwrap();
    std::env::set_var("PATH", &new_path);

    let check_json = lib_api::run_json(&env, &["execution", "check", "--format", "json"]);

    std::env::set_var("PATH", &prev_path);

    assert_eq!(check_json["can_handoff"], true);
}

#[test]
fn milestone_decompose_scaffolds_work_packages() {
    let env = TestEnv::blank();
    assert!(lib_api::run(&env, &["init", "--format", "json"])
        .status
        .success());

    let create_json = r#"{
        "title": "Feature",
        "intent": { "outcome": "Ship it." },
        "problem": { "description": "Missing feature." },
        "scope": { "in_scope": ["core"], "out_of_scope": ["mobile", "admin"] },
        "acceptance_criteria": [
            { "description": "Works", "verification": "cargo test" }
        ]
    }"#;
    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ],
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    for status in ["interview", "review", "ready"] {
        assert!(env
            .run(&[
                "milestone",
                "set-spec-status",
                id,
                status,
                "--format",
                "json"
            ])
            .status
            .success());
    }

    let decompose = lib_api::run(
        &env,
        &[
            "milestone",
            "decompose",
            id,
            "--work-packages",
            "2",
            "--format",
            "json",
        ],
    );
    assert!(decompose.status.success());
    let report: serde_json::Value = serde_json::from_slice(&decompose.stdout).unwrap();
    assert_eq!(report["scaffolded"], true);
    assert!(!report["gaps"]["missing"].as_array().unwrap().is_empty());
}
