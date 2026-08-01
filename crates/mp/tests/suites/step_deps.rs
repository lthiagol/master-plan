use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn step_update_depends_on_steps_persists() {
    let env = TestEnv::blank();
    assert!(lib_api::run(&env, &["init", "--format", "json"])
        .status
        .success());

    let create_json = r#"{
        "title": "Dep test",
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
    lib_api::run(
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
    lib_api::run(&env, &["milestone", "approve", "01", "--format", "json"]);
    lib_api::run(&env, &["milestone", "decompose", "01", "--format", "json"]);
    lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            "01",
            "--wp",
            "WP1",
            "--action",
            "step one",
            "--done-when",
            "done",
            "--tests",
            "test_one",
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
            "01",
            "--wp",
            "WP1",
            "--action",
            "step two",
            "--done-when",
            "done",
            "--tests",
            "test_two",
            "--format",
            "json",
        ],
    );

    // Set S2 to depend on S1
    let update = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            "01",
            "S2",
            "--depends-on-steps",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );

    // Verify via show
    let show = lib_api::run(&env, &["show", "milestone", "01", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let steps = json["steps"].as_array().unwrap();
    let s2 = steps
        .iter()
        .find(|s| s["id"].as_str() == Some("S2"))
        .unwrap();
    let deps: Vec<String> = s2["depends_on_steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(deps.contains(&"S1".to_string()), "S2 should depend on S1");
}
