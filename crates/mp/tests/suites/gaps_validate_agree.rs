use crate::common::TestEnv;

/// Under strictness=full, validate flags steps with empty tests.
#[test]
fn validate_flags_empty_step_tests_under_full_strictness() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    // Set strictness to full
    env.run(&[
        "config",
        "set",
        "workflow.gates.strictness",
        "full",
        "--format",
        "json",
    ]);

    let create_json = r#"{
        "title": "Test milestone",
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

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    // Approve it
    let approve = env.run(&["milestone", "approve", id, "--format", "json"]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );

    // Decompose to scaffold WPs
    env.run(&["milestone", "decompose", id, "--format", "json"]);

    // Add a step without tests field (empty tests)
    let step = env.run(&[
        "milestone",
        "step",
        "add",
        id,
        "--wp",
        "WP1",
        "--action",
        "do something",
        "--done-when",
        "it works",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);
    assert!(
        step.status.success(),
        "{}",
        String::from_utf8_lossy(&step.stderr)
    );

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // Under full strictness, should be an error (G10)
    let errors = json["errors"].as_array().unwrap();
    let has_test_error = errors.iter().any(|e| {
        e["code"].as_str() == Some("G10") && e["message"].as_str().unwrap().contains("tests")
    });
    assert!(
        has_test_error,
        "validate should flag empty step tests under full strictness"
    );
}

/// plan gaps reports missing step tests independently.
#[test]
fn plan_gaps_reports_empty_step_tests() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "Gaps test",
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

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    assert!(create.status.success());
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    env.run(&["milestone", "approve", id, "--format", "json"]);
    env.run(&["milestone", "decompose", id, "--format", "json"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        id,
        "--wp",
        "WP1",
        "--action",
        "do something",
        "--done-when",
        "it works",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);

    let out = env.run(&["plan", "gaps", id, "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let missing = json["missing"].as_array().unwrap();
    let has_test_gap = missing
        .iter()
        .any(|g| g["field"].as_str().unwrap().contains("tests"));
    assert!(has_test_gap, "plan gaps should flag missing step tests");
}
