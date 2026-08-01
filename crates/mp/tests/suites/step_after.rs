use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn step_add_after_inserts_in_correct_position() {
    let env = TestEnv::new();

    // Create a milestone and add three steps
    let create_json = r#"{
        "title": "Test Milestone",
        "intent": { "outcome": "Test --after flag." },
        "problem": { "description": "Need step ordering." },
        "scope": {
            "in_scope": ["Step ordering"],
            "out_of_scope": ["Other", "More"]
        },
        "acceptance_criteria": [
            { "description": "Step ordering works", "verification": "cargo test step_after" }
        ]
    }"#;
    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ],
    );
    assert!(create.status.success());
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).expect("json");
    let id = created["milestone"]["id"].as_str().expect("id").to_string();

    // Approve and decompose to add work packages
    let approve = lib_api::run(&env, &["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());
    let decompose = lib_api::run(&env, &["milestone", "decompose", &id, "--format", "json"]);
    assert!(decompose.status.success());

    // Add three steps
    let step1 = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "First step",
            "--id",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(step1.status.success());

    let step2 = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Second step",
            "--id",
            "S2",
            "--format",
            "json",
        ],
    );
    assert!(step2.status.success());

    let step3 = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Third step",
            "--id",
            "S3",
            "--format",
            "json",
        ],
    );
    assert!(step3.status.success());

    // Add a step after S1
    let after = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "After S1",
            "--after",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(
        after.status.success(),
        "add after failed: {}",
        String::from_utf8_lossy(&after.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&after.stdout).expect("json");
    let step = &result["step"];
    assert_eq!(step["action"], "After S1");
    assert_eq!(step["id"], "S1.1", "should create S1.1 as child of S1");

    // List steps and verify order
    let list = lib_api::run(
        &env,
        &["list", "steps", "--milestone", &id, "--format", "json"],
    );
    assert!(list.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json");
    let steps = listed["steps"].as_array().expect("steps");

    // Steps should be sorted by ID: S1, S1.1, S2, S3
    assert_eq!(steps.len(), 4, "expected 4 steps");
    assert_eq!(steps[0]["step"]["action"], "First step");
    assert_eq!(steps[1]["step"]["action"], "After S1");
    assert_eq!(steps[1]["step"]["id"], "S1.1");
    assert_eq!(steps[2]["step"]["action"], "Second step");
    assert_eq!(steps[3]["step"]["action"], "Third step");
}

#[test]
fn step_add_after_unknown_step_fails() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Test Fail",
        "intent": { "outcome": "Test --after failure." },
        "problem": { "description": "Need error case." },
        "scope": {
            "in_scope": ["Error case"],
            "out_of_scope": ["Other", "More"]
        },
        "acceptance_criteria": [
            { "description": "Error case", "verification": "cargo test step_after" }
        ]
    }"#;
    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ],
    );
    assert!(create.status.success());
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).expect("json");
    let id = created["milestone"]["id"].as_str().expect("id").to_string();

    let approve = lib_api::run(&env, &["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());
    let decompose = lib_api::run(&env, &["milestone", "decompose", &id, "--format", "json"]);
    assert!(decompose.status.success());

    let result = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Test",
            "--after",
            "S99",
            "--format",
            "json",
        ],
    );
    assert!(!result.status.success(), "should fail on unknown step");
}
