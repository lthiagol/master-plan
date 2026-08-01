use std::process::Command;

use crate::common::{
    isolated_harness_env, mp_bin, path_with_install_bin, repo_root, run_with_retry,
    unstable_mp_bin_str,
};
use tempfile::TempDir;

/// mp install --source pointing at a binary path should error with
/// "repo root" or "binary path" message.
#[test]
fn install_source_binary_path_errors() {
    let install_root = TempDir::new().expect("install");
    let work = TempDir::new().expect("work");

    // Point --source at the mp binary itself (or any file)
    let install_root_path = install_root.path().to_path_buf();
    let work_path = work.path().to_path_buf();
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.current_dir(&work_path)
                .env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .args([
                    "install",
                    "--harness",
                    "both",
                    "--dev",
                    "--source",
                    unstable_mp_bin_str(), // raw CARGO_BIN_EXE_mp path: binary, not repo root
                    "--format",
                    "json",
                ]);
            cmd
        },
        5,
    );

    assert!(
        !out.status.success(),
        "install should fail with binary --source"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("repo root") || stderr.contains("binary path"),
        "error should mention repo root or binary path, got: {stderr}"
    );
}

/// mp install with --source pointing at a non-existent dir errors about templates missing.
#[test]
fn install_source_no_templates_errors() {
    let install_root = TempDir::new().expect("install");
    let work = TempDir::new().expect("work");
    let empty_dir = TempDir::new().expect("empty");

    let install_root_path = install_root.path().to_path_buf();
    let work_path = work.path().to_path_buf();
    let empty_path = empty_dir.path().to_string_lossy().to_string();
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.current_dir(&work_path)
                .env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .args([
                    "install",
                    "--harness",
                    "both",
                    "--dev",
                    "--source",
                    &empty_path,
                    "--format",
                    "json",
                ]);
            cmd
        },
        5,
    );

    assert!(
        !out.status.success(),
        "install should fail without templates/"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("templates/ missing") || stderr.contains("repo root"),
        "error should mention templates/ missing, got: {stderr}"
    );
}

/// Default install (no --source, no --dev) succeeds with MP_HOME set to repo_root.
#[test]
fn install_default_succeeds() {
    let install_root = TempDir::new().expect("install");

    let install_root_path = install_root.path().to_path_buf();
    let path_with_install = path_with_install_bin(&install_root_path);
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .env("PATH", &path_with_install)
                .args(["install", "--harness", "opencode", "--format", "json"]);
            isolated_harness_env(&mut cmd, install_root.path());
            cmd
        },
        5,
    );

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert!(install_root.path().join("bin/mp").is_file());
}
