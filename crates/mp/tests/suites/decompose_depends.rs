use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn infer_depends_on_steps_from_wp_order() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Test Deps",
        "intent": { "outcome": "Test depends_on_steps inference." },
        "problem": { "description": "Need dependency inference." },
        "scope": {
            "in_scope": ["Dep inference"],
            "out_of_scope": ["Other", "More"]
        },
        "acceptance_criteria": [
            { "description": "Deps inferred", "verification": "cargo test decompose_depends" }
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

    // Add two WPs with steps
    lib_api::run(
        &env,
        &[
            "milestone",
            "wp",
            "add",
            &id,
            "--id",
            "WP2",
            "--name",
            "Phase 2",
            "--goal",
            "Second phase",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Setup",
            "--id",
            "S1",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Build",
            "--id",
            "S2",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP2",
            "--action",
            "Deploy",
            "--id",
            "S3",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP2",
            "--action",
            "Verify",
            "--id",
            "S4",
            "--format",
            "json",
        ],
    );

    // Run infer-deps
    let infer = lib_api::run(&env, &["plan", "infer-deps", &id, "--format", "json"]);
    assert!(
        infer.status.success(),
        "infer-deps failed: {}",
        String::from_utf8_lossy(&infer.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&infer.stdout).expect("json");
    assert_eq!(result["ok"], true);
    assert!(
        result["steps_updated"].as_i64().unwrap_or(0) > 0,
        "expected some steps updated"
    );

    // Use list steps to verify depends_on_steps
    let list = lib_api::run(
        &env,
        &["list", "steps", "--milestone", &id, "--format", "json"],
    );
    assert!(list.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json");

    let extract = |step_id: &str| -> serde_json::Value {
        listed["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .find(|s| s["step"]["id"] == step_id)
            .expect(step_id)
            .clone()
    };

    // S1 (first in WP1): no deps
    let s1 = extract("S1");
    let s1_deps = s1["step"]["depends_on_steps"].as_array().expect("S1 deps");
    assert!(s1_deps.is_empty(), "S1 should have no deps (first in WP1)");

    // S2 (second in WP1): depends on S1
    let s2 = extract("S2");
    let s2_deps = s2["step"]["depends_on_steps"].as_array().expect("S2 deps");
    assert_eq!(s2_deps.len(), 1, "S2 should depend on S1");
    assert_eq!(s2_deps[0], "S1");

    // S3 (first in WP2): no deps
    let s3 = extract("S3");
    let s3_deps = s3["step"]["depends_on_steps"].as_array().expect("S3 deps");
    assert!(s3_deps.is_empty(), "S3 should have no deps (first in WP2)");

    // S4 (second in WP2): depends on S3
    let s4 = extract("S4");
    let s4_deps = s4["step"]["depends_on_steps"].as_array().expect("S4 deps");
    assert_eq!(s4_deps.len(), 1, "S4 should depend on S3");
    assert_eq!(s4_deps[0], "S3");
}

#[test]
fn infer_depends_on_steps_empty_milestone() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Empty Test",
        "intent": { "outcome": "Test empty milestone." },
        "problem": { "description": "No steps case." },
        "scope": {
            "in_scope": ["Empty"],
            "out_of_scope": ["Other", "More"]
        },
        "acceptance_criteria": [
            { "description": "No deps", "verification": "manual" }
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

    let result = lib_api::run(&env, &["plan", "infer-deps", &id, "--format", "json"]);
    assert!(
        result.status.success(),
        "should handle empty milestone gracefully"
    );
    let res: serde_json::Value = serde_json::from_slice(&result.stdout).expect("json");
    assert_eq!(res["steps_updated"], 0);
}
