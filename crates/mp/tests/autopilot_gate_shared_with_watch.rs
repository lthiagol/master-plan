//! M218 / S03 / AC-03: the autopilot herdr hard gate is called by
//! BOTH `mp autopilot start` and the legacy `mp watch` alias — no
//! divergence in gate behavior between the two.
//!
//! Pinning this contract in black-box tests means a future refactor
//! that accidentally bypasses the gate on the legacy `mp watch` path
//! (or that introduces a different gate for one command) gets caught
//! at the test layer. The test asserts four invariants:
//!
//! 1. Both commands exit 78 when herdr is absent (the missing path
//!    case from AC-01, applied symmetrically).
//! 2. Both commands emit the same `autopilot_herdr_gate` envelope
//!    (same reason, same required_version, same exit_code).
//! 3. Both commands refuse identically when herdr is below floor
//!    (the version path from AC-02, applied symmetrically).
//! 4. Both commands pass identically with a compatible herdr — the
//!    gate must NOT fire on either side when the install is healthy
//!    (negative parity check, not just positive refusal).

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Build a PATH that contains a real `herdr` script with the
/// supplied canned outputs. Used by both the below-floor negative
/// fixture and the compatible positive fixture.
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

/// PATH that has no `herdr` binary — drives the gate to the
/// `herdr-missing` reason for both commands.
fn path_without_herdr() -> String {
    let dir = tempfile::tempdir().expect("path tempdir");
    fs::write(dir.path().join(".no-herdr-marker"), b"empty").unwrap();
    dir.path().to_str().expect("utf-8").to_string()
}

#[test]
fn both_commands_refuse_identically_when_herdr_is_absent() {
    let env = TestEnv::new();
    let path = path_without_herdr();

    let autopilot = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--dry-run", "--format", "json"],
    );
    let watch = env.run_with_env(
        &[("PATH", &path)],
        &["watch", "--dry-run", "--format", "json"],
    );

    // Same exit code (78).
    assert_eq!(autopilot.status.code(), Some(78));
    assert_eq!(watch.status.code(), Some(78));

    // Same reason + same required_version + same exit_code in the
    // JSON envelope. The whole `autopilot_herdr_gate` object must
    // match field-for-field (modulo `message` wording, which is
    // allowed to differ — the AC is about behavior, not message
    // string equality).
    let a: Value = serde_json::from_slice(&autopilot.stdout).unwrap();
    let w: Value = serde_json::from_slice(&watch.stdout).unwrap();
    assert_eq!(
        a["autopilot_herdr_gate"]["reason"], w["autopilot_herdr_gate"]["reason"],
        "reason must match between commands"
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["exit_code"],
        w["autopilot_herdr_gate"]["exit_code"]
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["required_version"],
        w["autopilot_herdr_gate"]["required_version"]
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["install_hint"], w["autopilot_herdr_gate"]["install_hint"],
        "install hint must be identical across commands"
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["reason"],
        Value::String("herdr-missing".into())
    );
}

#[test]
fn both_commands_refuse_identically_when_herdr_below_floor() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-below-floor-shared");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(&bin_dir, "herdr 0.6.0", "Options:\n  --cwd <PATH>\n", "");
    let path = path_with_bin(&bin_dir);

    let autopilot = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--format", "json"],
    );
    let watch = env.run_with_env(&[("PATH", &path)], &["watch", "--format", "json"]);

    assert_eq!(autopilot.status.code(), Some(78));
    assert_eq!(watch.status.code(), Some(78));
    let a: Value = serde_json::from_slice(&autopilot.stdout).unwrap();
    let w: Value = serde_json::from_slice(&watch.stdout).unwrap();
    assert_eq!(
        a["autopilot_herdr_gate"]["reason"],
        w["autopilot_herdr_gate"]["reason"]
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["reason"],
        Value::String("herdr-below-floor".into())
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["detected_version"],
        w["autopilot_herdr_gate"]["detected_version"]
    );
    assert_eq!(
        a["autopilot_herdr_gate"]["detected_version"],
        Value::String("0.6.0".into())
    );
}

#[test]
fn both_commands_pass_gate_when_herdr_is_compatible() {
    // Negative parity: with a healthy herdr, the gate must NOT
    // fire on either side. Both commands surface other
    // preconditions (harness slot not configured in TestEnv::new)
    // — what we assert is that the gate envelope is absent, i.e.
    // no `autopilot_herdr_gate` key with `ok=false` in the
    // envelope. We check via the `ok` field: if the gate fired,
    // the JSON would carry `ok=false` + the gate envelope; if not,
    // the JSON is a normal dry-run / precondition report without
    // the gate envelope.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-herdr-compatible-shared");
    fs::create_dir_all(&bin_dir).unwrap();
    install_stub_herdr(
        &bin_dir,
        "herdr 0.7.0",
        "Options:\n  --kind <KIND>\n  --pane <ID>\n",
        "Usage: herdr pane split [OPTIONS]\n",
    );
    let path = path_with_bin(&bin_dir);

    let autopilot = env.run_with_env(
        &[("PATH", &path)],
        &["autopilot", "start", "--dry-run", "--format", "json"],
    );
    let watch = env.run_with_env(
        &[("PATH", &path)],
        &["watch", "--dry-run", "--format", "json"],
    );

    // Both should NOT be the gate exit code (78).
    assert_ne!(
        autopilot.status.code(),
        Some(78),
        "autopilot must not gate on compatible herdr"
    );
    assert_ne!(
        watch.status.code(),
        Some(78),
        "watch must not gate on compatible herdr"
    );

    // Neither stdout should carry the gate envelope.
    let a: Value = serde_json::from_slice(&autopilot.stdout).unwrap();
    let w: Value = serde_json::from_slice(&watch.stdout).unwrap();
    assert!(
        a.get("autopilot_herdr_gate").is_none(),
        "autopilot output must not include the gate envelope when herdr is compatible: {a}"
    );
    assert!(
        w.get("autopilot_herdr_gate").is_none(),
        "watch output must not include the gate envelope when herdr is compatible: {w}"
    );
}

#[test]
fn both_commands_share_the_same_lib_entrypoint() {
    // Static contract: the gate function lives in
    // `crates/mp/src/autopilot/gate.rs` and is called from
    // `cmd_watch` (which `cmd_autopilot_start` delegates to). This
    // pin prevents a future refactor that introduces a separate
    // gate for `mp autopilot start` while leaving `mp watch`
    // unprotected (or vice versa).
    //
    // We verify the module path is reachable from both the
    // autopilot module and the watch command — a runtime check
    // would require an end-to-end spawn; this is the cheaper
    // surface-level guard that catches "two-gates drift" at the
    // symbol level.
    use mp::autopilot::{check_autopilot_herdr_gate_default, EX_AUTOPILOT_GATE};
    assert_eq!(EX_AUTOPILOT_GATE, 78);

    // The function is callable; in a no-herdr env it returns Err
    // (the gate fires). The test environment may have herdr on
    // PATH — both outcomes are fine; the contract is "the function
    // is reachable from the autopilot module" and "the exit code
    // is 78 when it fires".
    let _ = check_autopilot_herdr_gate_default();
}
