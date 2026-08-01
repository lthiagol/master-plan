use std::fs;
use std::process::Command;

use crate::common::TestEnv;

fn init_git(env: &TestEnv) {
    let root = env.tmp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .expect("git config name");
}

#[test]
fn git_status_suggest_and_commit_plan_only() {
    let env = TestEnv::blank();
    init_git(&env);

    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    Command::new("git")
        .args(["add", "."])
        .current_dir(env.tmp.path())
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(env.tmp.path())
        .output()
        .expect("git commit");

    let plan_path = env.tmp.path().join("master-plan/plan.json");
    let mut plan = fs::read_to_string(&plan_path).unwrap();
    plan = plan.replace(
        "\"planning_status\": \"planning\"",
        "\"planning_status\": \"in-execution\"",
    );
    fs::write(&plan_path, plan).unwrap();

    let status = env.run(&["git", "status", "--format", "json"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status_json: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(!status_json["clean"].as_bool().unwrap());
    assert!(!status_json["changed"].as_array().unwrap().is_empty());

    let suggest = env.run(&["git", "suggest-message", "--format", "json"]);
    assert!(
        suggest.status.success(),
        "{}",
        String::from_utf8_lossy(&suggest.stderr)
    );
    let suggest_json: serde_json::Value = serde_json::from_slice(&suggest.stdout).unwrap();
    let msg = suggest_json["message"].as_str().unwrap();
    assert!(msg.starts_with("plan:"));

    let commit = env.run(&[
        "git",
        "commit",
        "--message",
        "plan: activate planning",
        "--format",
        "json",
    ]);
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    let commit_json: serde_json::Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert!(commit_json["committed"].as_bool().unwrap());

    let clean = env.run(&["git", "status", "--format", "json"]);
    let clean_json: serde_json::Value = serde_json::from_slice(&clean.stdout).unwrap();
    assert!(clean_json["clean"].as_bool().unwrap());
}

#[test]
fn git_commit_auto_push_when_configured() {
    use std::process::Command;

    let env = TestEnv::blank();
    let root = env.tmp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .unwrap();

    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    assert!(env
        .run(&["config", "set", "git.auto_push", "true", "--format", "json"])
        .status
        .success());

    let plan_path = root.join("master-plan/plan.json");
    let mut plan = std::fs::read_to_string(&plan_path).unwrap();
    plan = plan.replace(
        "\"planning_status\": \"planning\"",
        "\"planning_status\": \"active\"",
    );
    std::fs::write(&plan_path, plan).unwrap();

    let commit = env.run(&[
        "git",
        "commit",
        "--message",
        "plan: update status",
        "--format",
        "json",
    ]);
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert!(json["committed"].as_bool().unwrap());
    assert!(!json
        .get("pushed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false));
}
