//! mp milestone trace (M70).

use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn trace_reports_ac_gaps_and_test_classification() {
    let env = TestEnv::new();
    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--title",
            "Trace fixture",
            "--json",
            r#"{
            "title":"Trace fixture",
            "intent":{"outcome":"x"},
            "problem":{"description":"y"},
            "scope":{"in_scope":["a"],"out_of_scope":["b","c"]},
            "acceptance_criteria":[
                {"id":"AC-01","description":"covered manual","verification":"manual: ok"},
                {"id":"AC-02","description":"uncovered runnable","verification":"manual: trace fixture sanity check"}
            ]
        }"#,
        ],
    );
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    lib_api::run(&env, &["milestone", "approve", &id]);
    lib_api::run(
        &env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
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
            "Runnable step",
            "--tests",
            "echo ok",
            "--done-when",
            "green",
            "--covers-ac",
            "AC-01",
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
            "Missing tests",
            "--tests",
            "",
            "--done-when",
            "n/a",
        ],
    );

    let trace = lib_api::run(&env, &["milestone", "trace", &id, "--format", "json"]);
    assert!(
        trace.status.success(),
        "{}",
        String::from_utf8_lossy(&trace.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&trace.stdout).unwrap();
    let gaps = json["gaps"].as_array().unwrap();
    assert!(
        gaps.iter().any(|g| g["kind"] == "uncovered_ac"),
        "expected uncovered AC gap: {json}"
    );
    assert!(
        gaps.iter().any(|g| g["kind"] == "missing_step_tests"),
        "expected missing tests gap: {json}"
    );
    assert!(
        gaps.iter().any(|g| g["kind"] == "manual_ac_runnable_step"),
        "expected manual AC with runnable step gap: {json}"
    );
    let steps = json["steps"].as_array().unwrap();
    assert!(
        steps.iter().any(|s| s["tests_kind"] == "runnable"),
        "expected runnable classification"
    );
}
