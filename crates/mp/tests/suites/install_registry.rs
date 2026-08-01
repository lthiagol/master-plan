use std::process::Command;

use crate::common::{
    isolated_harness_env, mp_bin, path_with_install_bin, repo_root, run_with_retry,
};
use tempfile::TempDir;

fn run(args: &[&str]) -> std::process::Output {
    let args = args.to_vec();
    run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root()).args(&args);
            cmd
        },
        5,
    )
}

fn run_install(install_root: &TempDir, args: &[&str]) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    let mut all = Vec::from(args);
    all.extend_from_slice(&["--dev", "--source", &source, "--format", "json"]);
    let install_root_path = install_root.path().to_path_buf();
    let path_with_install = path_with_install_bin(&install_root_path);
    run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .env("PATH", &path_with_install);
            isolated_harness_env(&mut cmd, install_root.path());
            cmd.args(&all);
            cmd
        },
        5,
    )
}

#[test]
fn install_default_harness_opencode() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    let harnesses = json["harnesses"].as_array().unwrap();
    assert_eq!(harnesses.len(), 1);
    assert_eq!(harnesses[0], "opencode");
}

#[test]
fn install_multi_harness_comma_separated() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install", "--harness", "opencode,cursor"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    let harnesses = json["harnesses"].as_array().unwrap();
    assert_eq!(harnesses.len(), 2);
    let ids: Vec<&str> = harnesses.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(ids.contains(&"cursor"));
    assert!(ids.contains(&"opencode"));
}

#[test]
fn install_harness_both_expands_all() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install", "--harness", "both"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    let harnesses = json["harnesses"].as_array().unwrap();
    assert_eq!(harnesses.len(), 8, "both should expand to 8 harnesses");
}

#[test]
fn install_unknown_harness_errors() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install", "--harness", "nonexistent"]);
    assert!(!out.status.success(), "should error on unknown harness");
}

#[test]
fn install_deploys_binaries() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        root.path().join("bin/mp").is_file(),
        "mp binary should exist"
    );
}

#[test]
fn print_paths_default_returns_one() {
    let out = run(&["install", "--print-paths", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(
        paths.len(),
        1,
        "default --print-paths should return 1 harness (opencode)"
    );
    assert_eq!(paths[0]["id"], "opencode");
}

#[test]
fn print_paths_both_returns_all() {
    let out = run(&[
        "install",
        "--print-paths",
        "--harness",
        "both",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 8, "both should return all 8 harnesses");
}

#[test]
fn print_paths_opencode() {
    let out = run(&[
        "install",
        "--print-paths",
        "--harness",
        "opencode",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["id"], "opencode");
}

#[test]
fn print_paths_multi_harness() {
    let out = run(&[
        "install",
        "--print-paths",
        "--harness",
        "cursor,claude-code",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0]["id"], "claude-code");
    assert_eq!(paths[1]["id"], "cursor");
}

#[test]
fn uninstall_removes_harness_artifacts() {
    let root = TempDir::new().expect("temp");
    let _ = run_install(&root, &["install"]);
    let install_root_path = root.path().to_path_buf();
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .args(["uninstall", "--harness", "opencode", "--format", "json"]);
            isolated_harness_env(&mut cmd, root.path());
            cmd
        },
        5,
    );
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    let removed = json["removed"].as_array().unwrap();
    assert!(!removed.is_empty(), "should remove something");
}

#[test]
fn uninstall_purge_removes_everything() {
    let root = TempDir::new().expect("temp");
    let _ = run_install(&root, &["install"]);
    let install_root_path = root.path().to_path_buf();
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .args(["uninstall", "--purge", "--format", "json"]);
            isolated_harness_env(&mut cmd, root.path());
            cmd
        },
        5,
    );
    assert!(
        out.status.success(),
        "uninstall --purge failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
}

#[test]
fn install_toolkit_only_skips_harness_artifacts() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install", "--toolkit-only"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.path().join("bin/mp").is_file());
    assert!(root.path().join("env.sh").is_file());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["harnesses"].as_array().unwrap().len(), 0);
}

#[test]
fn install_pi_deploys_agent_layout() {
    let root = TempDir::new().expect("temp");
    let pi_skill = root.path().join("harness/pi/agent/skills");
    let out = run_install(&root, &["install", "--harness", "pi"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The default CPD skill set installs three skills per harness.
    // spec-grill is opt-in via --skills=spec-grill (AC-05).
    assert!(
        pi_skill.join("mp-flow").join("SKILL.md").is_file(),
        "pi mp-flow skill"
    );
    assert!(
        pi_skill.join("mp-runner").join("SKILL.md").is_file(),
        "pi mp-runner skill"
    );
    assert!(
        pi_skill.join("mp-coordinator").join("SKILL.md").is_file(),
        "pi mp-coordinator skill"
    );
    let agent_root = root.path().join("harness/pi/agent");
    assert!(
        agent_root.join("AGENTS.md").is_file(),
        "pi convention at agent root"
    );
    assert!(
        !pi_skill.join("spec-grill").join("SKILL.md").is_file(),
        "pi spec-grill must NOT install by default"
    );
}

#[test]
fn install_report_has_doctor() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["doctor"].is_object(),
        "install report should include doctor"
    );
    assert!(
        json["mp_home"].is_string(),
        "install report should include mp_home"
    );
    assert!(
        json["path_snippet"].is_string(),
        "install report should include path_snippet"
    );
}
