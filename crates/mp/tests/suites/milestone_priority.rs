use crate::common::lib_api;
use crate::common::TestEnv;

fn create_milestone(env: &TestEnv) -> String {
    let json = r#"{
        "title": "Priority Test",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "priority": "normal",
        "intent": { "outcome": "Test priority setting" },
        "problem": { "description": "Testing priority." },
        "scope": {
            "in_scope": ["priority"],
            "out_of_scope": ["x", "y"]
        },
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ]
    }"#;
    let out = lib_api::run(
        env,
        &["milestone", "create", "--json", json, "--format", "json"],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn set_priority_to_high() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = lib_api::run(&env, &["milestone", "set-priority", &id, "high"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["priority"].as_str().unwrap(), "high");

    let show = lib_api::run(&env, &["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(m["milestone"]["priority"].as_str().unwrap(), "high");

    // Verify --where filter picks it up
    let list = lib_api::run(&env, &["list", "milestones", "--where", "priority==high"]);
    let list_v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let items = list_v["milestones"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str().unwrap(), id);
}

#[test]
fn set_priority_all_valid_values() {
    let env = TestEnv::new();
    for priority in &["urgent", "high", "normal", "low"] {
        let id = create_milestone(&env);
        let out = lib_api::run(&env, &["milestone", "set-priority", &id, priority]);
        assert!(
            out.status.success(),
            "failed for {}: {}",
            priority,
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["priority"].as_str().unwrap(), *priority);
    }
}

#[test]
fn set_priority_invalid_rejected() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = lib_api::run(&env, &["milestone", "set-priority", &id, "critical"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid priority"), "stderr: {stderr}");
    assert!(stderr.contains("urgent"), "should list valid values");
}

#[test]
fn set_priority_roundtrip() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    for new_priority in &["urgent", "low", "high", "normal"] {
        let out = lib_api::run(&env, &["milestone", "set-priority", &id, new_priority]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // final value should be normal
    let show = lib_api::run(&env, &["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(m["milestone"]["priority"].as_str().unwrap(), "normal");
}

#[test]
fn set_priority_on_nonexistent_milestone_errors() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["milestone", "set-priority", "M99", "high"]);
    assert!(!out.status.success());
}
