use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn path_include_grooming_flag() {
    let env = TestEnv::new();

    let path = lib_api::run(&env, &["path", "--include-grooming", "--format", "json"]);
    assert!(
        path.status.success(),
        "path with --include-grooming failed: {}",
        String::from_utf8_lossy(&path.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&path.stdout).expect("json");
    let _actions = result["actions"].as_array().expect("actions");
    assert!(result.get("actions").is_some(), "actions should be present");
}

#[test]
fn path_include_coverage_gaps_flag() {
    let env = TestEnv::new();

    let path = lib_api::run(
        &env,
        &["path", "--include-coverage-gaps", "--format", "json"],
    );
    assert!(
        path.status.success(),
        "path with --include-coverage-gaps failed: {}",
        String::from_utf8_lossy(&path.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&path.stdout).expect("json");
    assert!(result.get("actions").is_some(), "actions should be present");
}

#[test]
fn path_prioritize_coverage_flag() {
    let env = TestEnv::new();

    let path = lib_api::run(&env, &["path", "--prioritize-coverage", "--format", "json"]);
    assert!(
        path.status.success(),
        "path with --prioritize-coverage failed: {}",
        String::from_utf8_lossy(&path.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&path.stdout).expect("json");
    assert!(result.get("actions").is_some(), "actions should be present");
}

#[test]
fn path_all_flags_together() {
    let env = TestEnv::new();

    let path = lib_api::run(
        &env,
        &[
            "path",
            "--include-grooming",
            "--include-coverage-gaps",
            "--prioritize-coverage",
            "--format",
            "json",
        ],
    );
    assert!(
        path.status.success(),
        "path with all flags failed: {}",
        String::from_utf8_lossy(&path.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&path.stdout).expect("json");
    assert!(result.get("actions").is_some(), "actions should be present");
}

#[test]
fn path_list_pins_milestone_filter() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &["path", "list-pins", "--milestone", "01", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "path list-pins --milestone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(result.get("pins").is_some(), "pins should be present");
}

#[test]
fn path_suggest_returns_actionable_hints() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Blocked feature",
        "depends_on": ["99"],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "Ship" },
        "problem": { "description": "Needs foundation" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "works", "verification": "test" }]
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

    let suggest = lib_api::run(&env, &["path", "suggest", "--format", "json"]);
    assert!(
        suggest.status.success(),
        "{}",
        String::from_utf8_lossy(&suggest.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&suggest.stdout).unwrap();
    assert!(json["ok"].as_bool().unwrap());
    assert!(json["suggestions"].is_array());
}
