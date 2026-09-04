//! Config load reliability — corrupt config and disk read failures must not fail silently.

use std::fs;

use crate::common::TestEnv;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    // AC verification placeholder; must be `manual:` to pass the
    // M121 verify-ac Approve gate (F-08: gate fails on
    // UNRESOLVABLE/empty/unknown). The AC content is unrelated to the
    // corrupt-config write-block test; this just needs the milestone
    // in `approved` state.
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "manual: setup sanity check" }}
        ]
    }}"#
    );
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn corrupt_config_surfaces_validate_warning() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    fs::write(&config_path, "{[[[ not valid json").unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(out.status.success(), "validate should still run");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w["code"] == "W50"),
        "expected W50 for corrupt config, got: {warnings:?}"
    );
}

#[test]
fn corrupt_config_blocks_milestone_write() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "config-write-block");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "wp",
            "add",
            &id,
            "--name",
            "Work",
            "--id",
            "WP1",
            "--goal",
            "Do it",
            "--format",
            "json",
        ])
        .status
        .success());

    let config_path = env.tmp.path().join("master-plan/config.json");
    fs::write(&config_path, "{[[[ not valid json").unwrap();

    let out = env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "work",
        "--tests",
        "echo ok",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "write should fail when config.toml is corrupt"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("config") || stderr.contains("parse"),
        "expected config parse error, got: {stderr}"
    );
}

#[test]
fn corrupt_config_surfaces_doctor_warning() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    fs::write(&config_path, "{[[[ not valid json").unwrap();

    // CI runners ship without `herdr` on PATH; doctor gates
    // `report.ok` on the herdr shape check, so without this stub
    // the test would exit non-zero and `out.status.success()`
    // would fail even though the W50 warning is correctly
    // emitted. The stub satisfies `which_herdr` + the
    // `agent start --help` / `pane split --help` shape probes
    // (`crates/mp/src/autopilot/drive/herdr_version.rs`).
    let path = crate::common::fake_herdr::install_fake_herdr_for_doctor(&env);
    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    assert!(out.status.success(), "doctor should exit 0 on warnings");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let checks = json["checks"].as_array().expect("checks array");
    assert!(
        checks.iter().any(|c| {
            c["name"] == "config_parse"
                && c["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("W50") && m.contains("config.json"))
        }),
        "expected config_parse W50 warning in doctor checks, got: {checks:?}"
    );
}

#[test]
fn invalid_annotations_file_surfaces_validate_error() {
    let env = TestEnv::new();
    fs::write(
        env.tmp.path().join("master-plan/annotations.json"),
        "{[[[ not valid json",
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!json["ok"].as_bool().unwrap());
    let errors = json["errors"].as_array().unwrap();
    assert!(
        errors.iter().any(|e| e["code"] == "E03"),
        "expected E03 for annotation load failure, got: {errors:?}"
    );
}

#[test]
fn invalid_milestone_file_surfaces_validate_error() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    fs::write(milestones_dir.join("99-bad.json"), "{[[[ not valid json").unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "validate should emit JSON report even on failure: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert!(
        !json["ok"].as_bool().unwrap(),
        "validate report should not be ok"
    );
    let errors = json["errors"].as_array().unwrap();
    assert!(
        errors.iter().any(|e| e["code"] == "E02"),
        "expected E02 for milestone load failure, got: {errors:?}"
    );
    assert!(
        !out.status.success(),
        "validate should exit non-zero when errors are present"
    );
}
