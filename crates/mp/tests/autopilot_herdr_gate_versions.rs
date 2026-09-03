//! M218 / S02 / AC-02: semantic-version + required-command-shape
//! compatibility check for herdr. The gate accepts every version
//! >= 0.7.0 whose required command shape is present, rejects lower
//! versions or incompatible shapes with exit 78, and reports the
//! detected version + upgrade hint.
//!
//! Strategy: build stub `herdr` scripts in a temp bin dir, prepend
//! the dir to PATH, and exercise `mp autopilot start` against each
//! stub. The stubs branch on `$1 $2 $3` to emit the canned
//! `--version`, `agent start --help`, and `pane split --help` shapes
//! the M197 `detect_herdr_cli` probe expects.

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Install a fake `herdr` script with the supplied `version_output`,
/// `start_help_output`, and `pane_help_output` returned by the three
/// probe subcommands. Returns the bin dir (already chmod 755) so the
/// caller can prepend it to PATH.
fn install_stub_herdr(
    dir: &Path,
    version_output: &str,
    start_help_output: &str,
    pane_help_output: &str,
) -> std::path::PathBuf {
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
    dir.to_path_buf()
}

fn prepend_path(dir: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<std::path::PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, dir.to_path_buf());
    std::env::join_paths(parts)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// Assert the gate refuses with the AC-02 contract: exit 78 +
/// `ok=false` + reason + detected_version + upgrade_hint that names
/// the floor.
fn assert_version_refusal(parsed: &Value, expected_reason: &str, expected_detected: Option<&str>) {
    assert_eq!(
        parsed["ok"],
        Value::Bool(false),
        "ok flag must be false: {parsed}"
    );
    let gate = &parsed["autopilot_herdr_gate"];
    assert_eq!(
        gate["reason"],
        Value::String(expected_reason.to_string()),
        "reason must be {expected_reason}: {parsed}"
    );
    assert_eq!(
        gate["exit_code"],
        Value::from(78),
        "exit_code must be 78: {parsed}"
    );
    assert_eq!(
        gate["required_version"],
        Value::String("0.7.0".into()),
        "required_version must be the floor: {parsed}"
    );
    if let Some(detected) = expected_detected {
        assert_eq!(
            gate["detected_version"],
            Value::String(detected.to_string()),
            "detected_version must name the installed version: {parsed}"
        );
    }
    assert!(
        gate["upgrade_hint"]
            .as_str()
            .is_some_and(|h| h.contains("0.7.0")),
        "upgrade_hint must name the floor 0.7.0: {parsed}"
    );
    assert!(
        gate["message"]
            .as_str()
            .is_some_and(|m| m.contains("0.7.0")),
        "message must mention the floor: {parsed}"
    );
}

#[test]
fn refuses_herdr_below_floor_0_6_0() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-below-floor");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.6.0",
        "Options:\n  --cwd <PATH>\n",
        "",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    assert_eq!(
        out.status.code(),
        Some(78),
        "below-floor herdr must trigger exit 78; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_version_refusal(&parsed, "herdr-below-floor", Some("0.6.0"));
}

#[test]
fn refuses_herdr_0_6_5_with_upgrade_hint_naming_detected_version() {
    // Pin the upgrade_hint message shape: when detected_version is
    // known, the hint must surface it so the operator sees the
    // exact delta they need to upgrade across.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-0-6-5");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.6.5",
        "Options:\n  --cwd <PATH>\n",
        "",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    assert_eq!(out.status.code(), Some(78));
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let upgrade = parsed["autopilot_herdr_gate"]["upgrade_hint"]
        .as_str()
        .unwrap();
    assert!(
        upgrade.contains("0.6.5"),
        "upgrade_hint must name the detected 0.6.5: {upgrade}"
    );
    assert!(
        upgrade.contains("0.7.0"),
        "upgrade_hint must name the floor: {upgrade}"
    );
}

#[test]
fn accepts_herdr_at_floor_0_7_0() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-at-floor");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.7.0",
        "Options:\n  --kind <KIND>\n  --pane <ID>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--dry-run", "--format", "json"],
    );
    // The hard gate must NOT fire at floor (0.7.0 is the threshold).
    // The dry-run may still report other precondition failures
    // (herdr_on_path IS satisfied here; the harness slot might be
    // missing) — what we care about is that the gate layer let
    // the request through. The gate report would have a
    // `autopilot_herdr_gate` field if it fired, so absence of that
    // field OR an `ok=true` envelope is the pass criterion.
    assert_ne!(
        out.status.code(),
        Some(78),
        "gate must NOT refuse at floor; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn refuses_incompatible_shape_when_version_is_at_or_above_floor() {
    // Edge case: herdr self-reports 0.7.0 (or higher) but the wire
    // shape has drifted — `agent start --help` no longer lists
    // --kind / --pane (the F-05 pair). Gate must refuse with
    // reason `herdr-incompatible-shape` and surface the missing
    // flags so the operator knows to rebuild herdr against the
    // 0.7.x contract.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-shape-drifted");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.8.0",
        "Options:\n  --harness <HARNESS>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    assert_eq!(
        out.status.code(),
        Some(78),
        "shape-drifted herdr must trigger exit 78 even at above-floor version"
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let gate = &parsed["autopilot_herdr_gate"];
    assert_eq!(gate["reason"], Value::String("herdr-incompatible-shape".into()));
    let missing = gate["missing_flags"].as_array().expect("missing_flags array");
    let missing_strs: Vec<&str> = missing.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        missing_strs.contains(&"--kind") && missing_strs.contains(&"--pane"),
        "missing_flags must include --kind and --pane: {missing_strs:?}"
    );
    assert_eq!(gate["detected_version"], Value::String("0.8.0".into()));
}

#[test]
fn accepts_higher_compatible_version() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-0-9-9");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.9.9",
        "Options:\n  --kind <KIND>\n  --pane <ID>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--dry-run", "--format", "json"],
    );
    assert_ne!(
        out.status.code(),
        Some(78),
        "compatible 0.9.9 herdr must pass the gate"
    );
}

#[test]
fn refuses_when_pane_split_subcommand_missing() {
    // Even with a compatible version string and present --kind /
    // --pane flags, if `pane split --help` returns nothing the
    // shape check fails (the F-05 surface). The reason is
    // `herdr-incompatible-shape` because version parses fine.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-no-pane-split");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.8.0",
        "Options:\n  --kind <KIND>\n  --pane <ID>\n",
        "",
    );
    let path = prepend_path(&bin_dir);

    let out = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    assert_eq!(out.status.code(), Some(78));
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let gate = &parsed["autopilot_herdr_gate"];
    assert_eq!(gate["reason"], Value::String("herdr-incompatible-shape".into()));
    assert!(
        gate["message"]
            .as_str()
            .is_some_and(|m| m.contains("pane split")),
        "message must call out the missing pane split subcommand: {gate}"
    );
}

#[test]
fn version_floor_pin_protects_against_silent_floor_drift() {
    // The agent contract requires the floor at exactly 0.7.0; a
    // future refactor that lowers it (e.g. "we accept 0.6.x
    // again") would silently break the gate contract. Pin the
    // floor at 0.7.0 so this test catches drift.
    use mp::watch::REQUIRED_HERDR_VERSION_FLOOR;
    assert_eq!(REQUIRED_HERDR_VERSION_FLOOR, "0.7.0");
}
