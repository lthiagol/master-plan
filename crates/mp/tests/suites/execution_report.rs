use serde_json::Value;

use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn execution_report_compiles_milestone_handoff() {
    let env = TestEnv::new();
    let create = lib_api::run_json(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
          "title": "Report fixture",
          "intent": {"outcome": "x"},
          "problem": {"description": "x"},
          "scope": {"in_scope": ["x"], "out_of_scope": ["a", "b"]},
          "acceptance_criteria": [{"id": "AC-01", "description": "ac", "verification": "manual: x"}]
        }"#,
            "--format",
            "json",
        ],
    );
    let id = create["milestone"]["id"].as_str().unwrap();

    lib_api::run_json(&env, &["milestone", "approve", id, "--format", "json"]);
    lib_api::run_json(
        &env,
        &[
            "milestone",
            "wp",
            "add",
            id,
            "--id",
            "WP1",
            "--name",
            "fixture",
            "--format",
            "json",
        ],
    );
    lib_api::run_json(
        &env,
        &[
            "milestone",
            "step",
            "add",
            id,
            "--wp",
            "WP1",
            "--action",
            "do thing",
            "--tests",
            "manual: accepted — done",
            "--format",
            "json",
        ],
    );

    let out = lib_api::run(&env, &["execution", "report", id, "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone_id"], id);
    assert!(!v["steps"].as_array().unwrap().is_empty());
    assert!(!v["acceptance_criteria"].as_array().unwrap().is_empty());
}
