use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, path_with_install_bin, repo_root};
use serde_json::Value;
use tempfile::TempDir;

/// Round-trip smoke test: install + doctor on temp dir.
#[test]
fn install_doctor_roundtrip() {
    let root = TempDir::new().expect("temp");
    let source = repo_root().to_string_lossy().to_string();

    let path_with_install = path_with_install_bin(root.path());

    // Install
    let mut install_cmd = Command::new(mp_bin());
    install_cmd
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .env("PATH", &path_with_install)
        .args(["install", "--dev", "--source", &source, "--format", "json"]);
    isolated_harness_env(&mut install_cmd, root.path());
    let install_out = install_cmd.output().expect("install");
    assert!(
        install_out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&install_out.stderr)
    );
    let install_json: Value = serde_json::from_slice(&install_out.stdout).unwrap();
    assert_eq!(install_json["ok"], true, "install should succeed");

    // Doctor on the installed tree
    let doctor_out = Command::new(mp_bin())
        .env("MP_HOME", root.path())
        .env("MP_INSTALL_DIR", root.path())
        .env("PATH", &path_with_install)
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor");
    assert!(
        doctor_out.status.success(),
        "doctor should exit 0 after install"
    );
    let doctor_json: Value = serde_json::from_slice(&doctor_out.stdout).unwrap();
    assert_eq!(
        doctor_json["ok"], true,
        "doctor should pass on installed tree"
    );
}

/// Smoke: install opencode, verify print-paths consistency.
#[test]
fn print_paths_consistency() {
    let out = Command::new(mp_bin())
        .env("MP_HOME", repo_root())
        .args([
            "install",
            "--print-paths",
            "--harness",
            "opencode",
            "--format",
            "json",
        ])
        .output()
        .expect("print-paths");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);

    let h = &paths[0];
    assert_eq!(h["id"], "opencode");
    assert!(h["display_name"].as_str().unwrap().contains("OpenCode"));
    assert!(h["convention_file"]
        .as_str()
        .unwrap()
        .contains("opencoderules"));
    assert!(h["skill_dir"].as_str().unwrap().contains(".agents/skills"));
    assert!(h["profile_dir"].as_str().unwrap().contains("skills"));
}
