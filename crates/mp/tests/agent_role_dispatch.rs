use common::TestEnv;

mod common;

#[test]
fn agent_role_set_coordinator() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "coordinator", "--format", "json"]);
    assert!(
        out.status.success(),
        "agent role coordinator failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["role"], "coordinator");

    let session_file = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("session.json");
    assert!(
        session_file.exists(),
        "session.json should exist after setting role"
    );
    let raw = std::fs::read_to_string(&session_file).unwrap();
    let session: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(session["role"], "coordinator");
    assert!(session["set_at"].is_string());
}

#[test]
fn agent_role_set_runner() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "runner", "--format", "json"]);
    assert!(
        out.status.success(),
        "agent role runner failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["role"], "runner");
}

#[test]
fn agent_role_aliases() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "mp-coordinator", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["role"], "coordinator");

    let out = env.run(&["agent", "role", "mp-runner", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["role"], "runner");
}

#[test]
fn agent_role_clear() {
    let env = TestEnv::new();

    env.run(&["agent", "role", "coordinator", "--format", "json"]);

    let out = env.run(&["agent", "role", "--clear", "--format", "json"]);
    assert!(
        out.status.success(),
        "agent role --clear failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["role"], serde_json::Value::Null);

    let session_file = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("session.json");
    assert!(
        !session_file.exists(),
        "session.json should be removed after --clear"
    );
}

#[test]
fn agent_role_clear_when_no_file() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "--clear", "--format", "json"]);
    assert!(
        out.status.success(),
        "clearing non-existent file should succeed"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["role"], serde_json::Value::Null);
}

#[test]
fn agent_role_bogus_errors() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "bogus", "--format", "json"]);
    assert!(
        !out.status.success(),
        "bogus role should fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown role"),
        "expected 'unknown role' in stderr, got: {stderr}"
    );
}

#[test]
fn agent_role_no_args_shows_error() {
    let env = TestEnv::new();

    let out = env.run(&["agent", "role", "--format", "json"]);
    assert!(!out.status.success(), "no role arg should fail");
}

#[test]
fn agent_role_overwrite() {
    let env = TestEnv::new();

    env.run(&["agent", "role", "coordinator", "--format", "json"]);
    env.run(&["agent", "role", "runner", "--format", "json"]);

    let session_file = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("session.json");
    let raw = std::fs::read_to_string(&session_file).unwrap();
    let session: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(session["role"], "runner");
}
