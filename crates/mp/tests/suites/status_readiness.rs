use crate::common::TestEnv;

/// status json includes can_handoff.
#[test]
fn status_includes_can_handoff() {
    let env = TestEnv::new();

    let out = env.run(&["status", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["execution"]["can_handoff"].is_boolean(),
        "status should include can_handoff"
    );
}

/// execution check reports can_handoff with decomposed milestones.
#[test]
fn execution_check_can_handoff_with_decomposed() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "Ready milestone",
        "intent": { "outcome": "Do something." },
        "problem": { "description": "Need to do something." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Must work", "verification": "cargo test" }
        ]
    }"#;

    env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    env.run(&["milestone", "approve", "01", "--format", "json"]);
    env.run(&["milestone", "decompose", "01", "--format", "json"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "do it",
        "--done-when",
        "done",
        "--tests",
        "test_foo",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);

    let out = env.run(&["execution", "check", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["can_handoff"].as_bool().unwrap_or(false),
        "decomposed milestone should enable can_handoff"
    );
}
