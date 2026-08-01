use std::process::Command;

use crate::common::{mp_bin, repo_root};
use serde_json::Value;
use tempfile::TempDir;

fn run_doctor(args: &[&str]) -> (bool, Value) {
    let install_stub = TempDir::new().expect("temp");
    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_stub.path())
        .args(args)
        .arg("--format")
        .arg("json");
    let out = cmd.output().expect("doctor");
    let json: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    (out.status.success(), json)
}

#[test]
fn doctor_toolkit_has_harnesses() {
    let (ok, json) = run_doctor(&["doctor"]);
    assert!(ok, "doctor should exit 0");
    assert_eq!(json["ok"], true, "doctor should be ok");
    let harnesses = json["harnesses"].as_array();
    assert!(harnesses.is_some(), "doctor should have harnesses field");
    if let Some(hs) = harnesses {
        assert_eq!(hs.len(), 8, "should report 8 harnesses");
    }
}

#[test]
fn doctor_harness_entry_has_all_fields() {
    let (ok, json) = run_doctor(&["doctor"]);
    assert!(ok);
    let harnesses = json["harnesses"].as_array().unwrap();
    for h in harnesses {
        assert!(h["id"].is_string(), "harness entry should have id");
        assert!(
            h["display_name"].is_string(),
            "harness entry should have display_name"
        );
        assert!(
            h["skill_installed"].is_boolean(),
            "harness entry should have skill_installed"
        );
        assert!(
            h["spec_grill_installed"].is_boolean(),
            "harness entry should have spec_grill_installed"
        );
        assert!(
            h["convention_file_installed"].is_boolean(),
            "harness entry should have convention_file_installed"
        );
    }
}

#[test]
fn doctor_harness_entries_include_all_ids() {
    let (ok, json) = run_doctor(&["doctor"]);
    assert!(ok);
    let harnesses = json["harnesses"].as_array().unwrap();
    let ids: Vec<&str> = harnesses
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    for expected in &[
        "opencode",
        "cursor",
        "claude-code",
        "gemini",
        "codex",
        "windsurf",
        "cline",
        "pi",
    ] {
        assert!(ids.contains(expected), "missing harness id: {expected}");
    }
}

#[test]
fn doctor_has_integrity_checks() {
    let (ok, json) = run_doctor(&["doctor"]);
    assert!(ok);
    let checks = json["checks"].as_array().unwrap();
    let check_names: Vec<&str> = checks.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(
        check_names.iter().any(|n| n.contains("plan.json")),
        "should check plan default"
    );
    assert!(
        check_names.iter().any(|n| n.contains("milestone.schema")),
        "should check milestone schema"
    );
    // Post-M141: the CPD skill integrity checks are
    // `integrity:cpd:<skill_id>` for each of mp-flow, mp-runner,
    // mp-coordinator. The pre-M141 single `integrity:skill` is gone.
    assert!(
        check_names.iter().any(|n| n.contains("cpd:mp-flow")),
        "should check mp-flow skill integrity"
    );
    assert!(
        check_names.iter().any(|n| n.contains("cpd:mp-runner")),
        "should check mp-runner skill integrity"
    );
    assert!(
        check_names.iter().any(|n| n.contains("cpd:mp-coordinator")),
        "should check mp-coordinator skill integrity"
    );
    assert!(
        check_names.iter().any(|n| n.contains("spec-grill")),
        "should check spec-grill integrity"
    );
}

#[test]
fn doctor_ok_on_valid_tree() {
    let (ok, json) = run_doctor(&["doctor"]);
    assert!(ok, "doctor should pass on repo root");
    assert_eq!(json["ok"], true);
    assert_eq!(json["templates"], true);
    assert_eq!(json["schemas"], true);
}

#[test]
fn doctor_without_mp_home_still_works() {
    let install_stub = TempDir::new().expect("temp");
    let mut cmd = Command::new(mp_bin());
    cmd.env_remove("MP_HOME")
        .env_remove("MPH_HOME")
        .env("HOME", repo_root())
        .env("MP_INSTALL_DIR", install_stub.path())
        .args(["doctor", "--format", "json"]);
    let out = cmd.output().expect("doctor");
    assert!(out.status.success(), "doctor should work without MP_HOME");
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["ok"], true,
        "doctor should pass with only embedded assets"
    );
}
