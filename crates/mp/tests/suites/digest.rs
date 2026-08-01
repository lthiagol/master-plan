use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root, TestEnv};
use tempfile::TempDir;

fn seed_handoff_gate(env: &TestEnv) {
    let create_json = r#"{
        "title": "Handoff gate",
        "intent": { "outcome": "Enable handoff." },
        "problem": { "description": "Need handoff gate." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: ok" }
        ]
    }"#;
    assert!(
        env.run(&["milestone", "create", "--json", create_json])
            .status
            .success(),
        "create handoff gate"
    );
    assert!(env.run(&["milestone", "approve", "01"]).status.success());
    assert!(env.run(&["milestone", "decompose", "01"]).status.success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            "01",
            "--wp",
            "WP1",
            "--action",
            "step",
            "--done-when",
            "done",
            "--tests",
            "manual: ok",
            "--covers-ac",
            "AC-01",
        ])
        .status
        .success());
}

fn do_handoff(env: &TestEnv) {
    let out = env.run(&["execution", "handoff"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn digest_reports_summary_and_since_window() {
    let env = TestEnv::new();
    let digest = env.run_json(&["digest", "--since", "7d", "--format", "json"]);
    assert!(digest["summary"].is_string());
    assert!(digest["since"].is_string());
    assert!(digest["until"].is_string());
    assert!(digest["validate_ok"].is_boolean());
}

#[test]
fn digest_rejects_invalid_since() {
    let env = TestEnv::new();
    let out = env.run(&["digest", "--since", "not-a-duration", "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn digest_since_handoff_outputs_markdown() {
    let env = TestEnv::new();
    seed_handoff_gate(&env);
    do_handoff(&env);

    let out = env.run(&["digest", "--since-handoff", "--markdown"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("# Progress Digest"));
    assert!(stdout.contains("**Period:**"));
}

#[test]
fn digest_days_flag_filters_window() {
    let env = TestEnv::new();
    let digest = env.run_json(&["digest", "--days", "14", "--format", "json"]);
    assert!(digest["since"].is_string());
    assert!(digest["until"].is_string());
}

#[test]
fn digest_markdown_writes_to_out_file() {
    let env = TestEnv::new();
    let out_path = env.tmp.path().join("digest.md");
    let out = env.run(&[
        "digest",
        "--since",
        "7d",
        "--markdown",
        "--out",
        &out_path.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = std::fs::read_to_string(&out_path).expect("read digest file");
    assert!(content.contains("# Progress Digest"));
}

#[test]
fn digest_rejects_conflicting_since_flags() {
    let env = TestEnv::new();
    let out = env.run(&[
        "digest",
        "--since-handoff",
        "--days",
        "7",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
}

#[test]
fn digest_since_handoff_fails_without_handoff() {
    let env = TestEnv::new();
    let out = env.run(&["digest", "--since-handoff", "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn uninstall_removes_installed_binaries() {
    let root = TempDir::new().expect("temp");
    let source = repo_root().to_string_lossy().to_string();
    let mut install = Command::new(mp_bin());
    install
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .args(["install", "--dev", "--source", &source, "--format", "json"]);
    isolated_harness_env(&mut install, root.path());
    let install_out = install.output().expect("install");
    assert!(
        install_out.status.success(),
        "{}",
        String::from_utf8_lossy(&install_out.stderr)
    );
    assert!(root.path().join("bin/mp").is_file());
    assert!(root.path().join("bin/raul").is_file());

    let mut uninstall = Command::new(mp_bin());
    uninstall
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .args(["uninstall", "--harness", "both", "--format", "json"]);
    isolated_harness_env(&mut uninstall, root.path());
    let uninstall_out = uninstall.output().expect("uninstall");
    assert!(
        uninstall_out.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall_out.stderr)
    );
    assert!(!root.path().join("bin/mp").exists());
    assert!(!root.path().join("bin/raul").exists());
}
