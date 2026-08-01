use crate::common::TestEnv;

#[test]
fn decision_remove_removes_record() {
    let env = TestEnv::new();

    // Add a decision
    let add = env.run(&[
        "decision",
        "add",
        "--summary",
        "Use Rust",
        "--context",
        "Decision context",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "add decision failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let id = json["decision"]["id"].as_str().unwrap().to_string();

    // Confirm it's in the list
    let list = env.run(&["decision", "list", "--format", "json"]);
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list_json["decisions"].as_array().unwrap().len(), 1);

    // Remove it
    let remove = env.run(&["decision", "remove", &id, "--format", "json"]);
    assert!(
        remove.status.success(),
        "remove decision failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );

    // List should be empty
    let list = env.run(&["decision", "list", "--format", "json"]);
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(
        list_json["decisions"].as_array().unwrap().len(),
        0,
        "decision list should be empty after remove"
    );

    // Validate should still be green
    assert!(
        validate_ok(&env),
        "validate should pass after decision remove"
    );
}

#[test]
fn decision_remove_not_found_fails() {
    let env = TestEnv::new();
    let remove = env.run(&["decision", "remove", "D-999", "--format", "json"]);
    assert!(
        !remove.status.success(),
        "removing non-existent decision should fail"
    );
}

#[test]
fn idea_remove_removes_record() {
    let env = TestEnv::new();

    // Add an idea
    let add = env.run(&[
        "idea",
        "create",
        "--title",
        "Refactor auth",
        "--body",
        "Make it simpler",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "add idea failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let id = json["idea"]["id"].as_str().unwrap().to_string();

    // Confirm it's in the list
    let list = env.run(&["idea", "list", "--format", "json"]);
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list_json["ideas"].as_array().unwrap().len(), 1);

    // Remove it
    let remove = env.run(&["idea", "remove", &id, "--format", "json"]);
    assert!(
        remove.status.success(),
        "remove idea failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );

    // List should be empty
    let list = env.run(&["idea", "list", "--format", "json"]);
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(
        list_json["ideas"].as_array().unwrap().len(),
        0,
        "idea list should be empty after remove"
    );

    // Validate should still be green
    assert!(validate_ok(&env), "validate should pass after idea remove");
}

#[test]
fn idea_remove_not_found_fails() {
    let env = TestEnv::new();
    let remove = env.run(&["idea", "remove", "ID-99", "--format", "json"]);
    assert!(
        !remove.status.success(),
        "removing non-existent idea should fail"
    );
}

fn validate_ok(env: &TestEnv) -> bool {
    let out = env.run(&["validate", "--format", "json"]);
    out.status.success()
}
