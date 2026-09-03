//! M218 / S01 / AC-01: `mp autopilot start` (and the legacy `mp watch`
//! alias) refuse to run when herdr is absent on PATH. The gate exits
//! 78 (EX_CONFIG) with a structured JSON report and an install hint,
//! BEFORE creating any session directory, writing any plan state, or
//! invoking any spawn operation.
//!
//! Strategy: build a PATH-only env where the herdr binary does NOT
//! exist, then run `mp autopilot start` (and `mp watch` for symmetry)
//! and assert (a) exit code 78, (b) the JSON report has
//! `ok=false` + `autopilot_herdr_gate.reason == "herdr-missing"`, and
//! (c) the install hint is present.

mod common;

use crate::common::TestEnv;
use serde_json::Value;

/// Strip every directory from PATH that might contain a real `herdr`
/// binary so the test environment deterministically has no herdr on
/// PATH. The PATH is set to a temp dir containing no `herdr` script —
/// enough to drive `which herdr` to `None` from inside the spawned
/// `mp` process. We intentionally do NOT inherit the parent PATH here:
/// a developer's installed `herdr` (via `make install` or otherwise)
/// must not make this test silently pass.
fn path_without_herdr() -> String {
    use std::fs;
    let dir = tempfile::tempdir().expect("path tempdir");
    // Keep a marker so the test author can confirm the env took
    // effect — useful when debugging the test in isolation.
    fs::write(dir.path().join(".no-herdr-marker"), b"empty").unwrap();
    dir.path()
        .to_str()
        .expect("path tempdir is utf-8")
        .to_string()
}

/// Helper: assert the structured gate report is the AC-01 contract.
fn assert_missing_gate_report(parsed: &Value) {
    assert_eq!(
        parsed["ok"],
        Value::Bool(false),
        "ok flag must be false: {parsed}"
    );
    let gate = &parsed["autopilot_herdr_gate"];
    assert_eq!(
        gate["reason"],
        Value::String("herdr-missing".into()),
        "reason must be herdr-missing: {parsed}"
    );
    assert_eq!(
        gate["exit_code"],
        Value::from(78),
        "exit_code must be 78 (EX_CONFIG): {parsed}"
    );
    assert_eq!(
        gate["required_version"],
        Value::String("0.7.0".into()),
        "required_version must name the floor: {parsed}"
    );
    assert!(
        gate["install_hint"]
            .as_str()
            .is_some_and(|h| h.contains("install") && h.contains("herdr")),
        "install_hint must mention install + herdr: {parsed}"
    );
    assert!(
        gate["message"]
            .as_str()
            .is_some_and(|m| m.contains("not on PATH") || m.contains("not on path")),
        "message must name the absence: {parsed}"
    );
    assert!(
        gate["detected_version"].is_null(),
        "detected_version must be null when herdr is absent: {parsed}"
    );
}

#[test]
fn autopilot_start_refuses_when_herdr_absent_with_install_hint_and_exit_78() {
    let env = TestEnv::new();
    let path = path_without_herdr();
    let out = env.run_with_env(&[("PATH", &path)], &["autopilot", "start", "--format", "json"]);

    assert_eq!(
        out.status.code(),
        Some(78),
        "exit code must be 78 (EX_CONFIG), got {:?}; stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout was not JSON: {e}; raw: {}", String::from_utf8_lossy(&out.stdout)));
    assert_missing_gate_report(&parsed);
}

#[test]
fn watch_legacy_alias_refuses_when_herdr_absent() {
    // AC-03 parity: the legacy `mp watch` command shares the same
    // gate as `mp autopilot start`. This test pins the shared-path
    // contract from the autopilot-side entry; the parity is also
    // exercised by `autopilot_gate_shared_with_watch`.
    let env = TestEnv::new();
    let path = path_without_herdr();
    let out = env.run_with_env(&[("PATH", &path)], &["watch", "--format", "json"]);

    assert_eq!(
        out.status.code(),
        Some(78),
        "exit code must be 78, got {:?}; stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout was not JSON: {e}; raw: {}", String::from_utf8_lossy(&out.stdout)));
    assert_missing_gate_report(&parsed);
}

#[test]
fn gate_refuses_before_creating_session_directory() {
    // AC-01 contract: the gate fires BEFORE any session directory
    // creation. After a refused run, the plan dir must NOT contain
    // an autopilot/ subdir.
    let env = TestEnv::new();
    let path = path_without_herdr();
    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    assert_eq!(out.status.code(), Some(78));

    // Verify no session.json landed under master-plan/autopilot/.
    let autopilot_dir = env.tmp.path().join("master-plan").join("autopilot");
    assert!(
        !autopilot_dir.exists(),
        "autopilot/ subdir must not be created when the gate refuses: {}",
        autopilot_dir.display()
    );
}

#[test]
fn gate_refuses_for_dry_run_path_too() {
    // AC-01 wording focuses on the non-dry-run path; we also gate
    // --dry-run so the dry-run preview matches the live behavior
    // (the user should never see a successful dry-run when herdr is
    // genuinely missing). The shared-path test in
    // `autopilot_gate_shared_with_watch` pins the symmetry contract.
    let env = TestEnv::new();
    let path = path_without_herdr();
    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--dry-run", "--format", "json"],
    );
    assert_eq!(
        out.status.code(),
        Some(78),
        "dry-run must also refuse when herdr is absent"
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_missing_gate_report(&parsed);
}
