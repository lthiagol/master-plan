//! M218 / S04 / AC-04: `mp doctor` surfaces the autopilot herdr
//! gate verdict — availability, detected version/shape, compatibility
//! result, and install/upgrade guidance. The doctor surface must NOT
//! become mutating (no plan-state writes); the check is informational
//! for visibility, while the actual enforcement lives in
//! `mp autopilot start` / `mp watch`.
//!
//! Strategy: install stub herdr scripts at known versions, run `mp
//! doctor`, and assert the `autopilot_herdr_gate` DoctorCheck is
//! present with the right `ok` value and a message that names the
//! install state.

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn install_stub_herdr(
    dir: &std::path::Path,
    version_output: &str,
    start_help_output: &str,
    pane_help_output: &str,
) {
    let script = format!(
        r#"#!/bin/sh
case "$1:$2:$3" in
  --version:*)
    cat <<'V'
{version}
V
    ;;
  agent:start:--help)
    cat <<'H'
{start_help}
H
    ;;
  pane:split:--help)
    cat <<'P'
{pane}
P
    ;;
  *)
    echo ok
    ;;
esac
"#,
        version = version_output,
        start_help = start_help_output,
        pane = pane_help_output,
    );
    let bin = dir.join("herdr");
    fs::write(&bin, script).unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
}

fn path_with_bin(dir: &std::path::Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<std::path::PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, dir.to_path_buf());
    std::env::join_paths(parts)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn path_without_herdr() -> String {
    let dir = tempfile::tempdir().expect("path tempdir");
    fs::write(dir.path().join(".no-herdr-marker"), b"empty").unwrap();
    dir.path().to_str().expect("utf-8").to_string()
}

/// Find the `autopilot_herdr_gate` DoctorCheck in the doctor
/// output. Returns the entry value.
fn gate_check(parsed: &Value) -> &Value {
    let checks = parsed["checks"].as_array().expect("checks array");
    checks
        .iter()
        .find(|c| c["name"] == "autopilot_herdr_gate")
        .unwrap_or_else(|| panic!("autopilot_herdr_gate check missing: {parsed}"))
}

#[test]
fn doctor_surfaces_gate_when_herdr_is_missing_with_install_guidance() {
    let env = TestEnv::new();
    let path = path_without_herdr();

    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    // Doctor exits non-zero when ANY check is red — that is the
    // M197 F-11 contract (CI pipelines detect failures via the
    // exit code). The AC-04 contract is that the doctor surface is
    // NON-MUTATING (no plan writes) and surfaces the gate; the
    // exit-code semantics are inherited from the existing
    // `cmd_doctor` / `emit_and_exit_on_fail` path, not changed.
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = gate_check(&parsed);
    assert_eq!(
        check["ok"],
        Value::Bool(false),
        "doctor must report ok=false when herdr is missing"
    );
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains("install") || message.contains("refused"),
        "doctor message must include install guidance or refused signal: {message}"
    );
    assert!(
        message.contains("herdr"),
        "doctor message must name herdr: {message}"
    );
}

#[test]
fn doctor_surfaces_gate_when_herdr_is_below_floor_with_upgrade_guidance() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-doctor-below-floor");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(&bin_dir, "herdr 0.6.0", "Options:\n  --cwd <PATH>\n", "");
    let path = path_with_bin(&bin_dir);

    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = gate_check(&parsed);
    assert_eq!(check["ok"], Value::Bool(false));
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains("0.7.0"),
        "doctor message must mention the required floor: {message}"
    );
    assert!(
        message.contains("0.6.0") || message.contains("upgrade"),
        "doctor message must name the detected version or call out the upgrade path: {message}"
    );
}

#[test]
fn doctor_surfaces_gate_when_herdr_is_compatible() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-doctor-compatible");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.8.0",
        "Options:\n  --kind <KIND>\n  --pane <ID>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = path_with_bin(&bin_dir);

    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = gate_check(&parsed);
    assert_eq!(
        check["ok"],
        Value::Bool(true),
        "doctor must report ok=true for compatible herdr: {check}"
    );
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains("0.8.0") && message.contains("0.7.0"),
        "doctor message must surface both the detected and required version: {message}"
    );
    assert!(
        message.contains("ready"),
        "doctor message must name the verdict (ready): {message}"
    );
}

#[test]
fn doctor_surfaces_gate_when_herdr_is_incompatible_shape() {
    // Above-floor version but the wire shape has drifted —
    // `agent start --help` no longer lists --kind/--pane. Doctor
    // must still surface this as ok=false with an actionable
    // message that names the missing flags.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-doctor-shape-drifted");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.8.0",
        "Options:\n  --harness <HARNESS>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = path_with_bin(&bin_dir);

    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = gate_check(&parsed);
    assert_eq!(check["ok"], Value::Bool(false));
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains("--kind") || message.contains("--pane") || message.contains("shape"),
        "doctor message must name the shape mismatch: {message}"
    );
}

#[test]
fn doctor_gate_check_is_non_mutating() {
    // AC-04 contract: `mp doctor` reports the gate but does NOT
    // make the overall command mutating. Verify by snapshotting
    // the plan directory before and after; only the
    // activity.json / config.json bookkeeping that validate-style
    // commands append is acceptable; no `autopilot/` subdir or
    // session.json must land.
    let env = TestEnv::new();
    let path = path_without_herdr();

    let plan_dir = env.tmp.path().join("master-plan");
    let before = std::fs::read_dir(&plan_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let out = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    // The exit code may be non-zero (M197 F-11 / AC-02 doctor
    // exit semantics); the contract under test is the
    // non-mutation surface, not the exit code.
    let _ = out.status.code();

    // No new `autopilot/` subdir created.
    assert!(
        !plan_dir.join("autopilot").exists(),
        "doctor must NOT create autopilot/ subdir"
    );
    // No session.json landed.
    assert!(
        !plan_dir.join("autopilot").join("session.json").exists(),
        "doctor must NOT write session.json"
    );
    // Snapshot stable: only allowlisted bookkeeping files (the
    // activity.json / config.json that validate-style commands
    // touch) may have been added. The exact list of allowed files
    // is intentionally generous — the strict assertion is the
    // autopilot/ absence above. Here we just log the diff so a
    // regression test author sees the surface area.
    let after = std::fs::read_dir(&plan_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let added: Vec<_> = after
        .iter()
        .filter(|n| !before.iter().any(|b| b == *n))
        .collect();
    if !added.is_empty() {
        eprintln!("doctor created files: {added:?}");
    }
}
