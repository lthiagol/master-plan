//! TW-07: mp validate flags verification/tests values that are neither a
//! single runnable command/path nor a `manual: accepted - <reason>` literal.

use crate::common::lib_api;
use crate::common::TestEnv;

fn validate_warnings(env: &TestEnv) -> serde_json::Value {
    let out = lib_api::run(env, &["validate", "--format", "json"]);
    serde_json::from_slice(&out.stdout).expect("validate JSON")
}

fn has_w20(report: &serde_json::Value) -> bool {
    report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "W20")
}

#[test]
fn single_command_passes() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "single-cmd",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "cargo test" }
        ]
    }"#;
    assert!(env
        .run(&["milestone", "create", "--json", json, "--format", "json"])
        .status
        .success());
    let report = validate_warnings(&env);
    assert!(!has_w20(&report), "single command should not produce W20");
}

#[test]
fn manual_accepted_passes() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "manual-ok",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "manual: accepted - code review" }
        ]
    }"#;
    assert!(env
        .run(&["milestone", "create", "--json", json, "--format", "json"])
        .status
        .success());
    let report = validate_warnings(&env);
    assert!(!has_w20(&report), "manual: accepted should not produce W20");
}

#[test]
fn comma_separated_flagged() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "comma-bad",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "cargo test, make check" }
        ]
    }"#;
    assert!(env
        .run(&["milestone", "create", "--json", json, "--format", "json"])
        .status
        .success());
    let report = validate_warnings(&env);
    assert!(has_w20(&report), "comma-separated should produce W20");
}

#[test]
fn single_path_passes() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "path-ok",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "crates/mp/tests/some_test.rs" }
        ]
    }"#;
    assert!(env
        .run(&["milestone", "create", "--json", json, "--format", "json"])
        .status
        .success());
    let report = validate_warnings(&env);
    assert!(!has_w20(&report), "single path should not produce W20");
}

/// BF-13 (M131): `mp validate` rejects invalid thread entries on
/// findings (W53), regardless of how the entry was added. Previously
/// this validation lived only in `add_finding_with_phase`, so a thread
/// entry with a non-RFC3339 `at` written any other way persisted
/// unvalidated. We write the bad entry directly to the milestone file
/// (the CLI doesn't expose thread editing) and confirm validate flags it.
#[test]
fn validate_rejects_invalid_thread_entry() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "bf13-thread",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "echo ok" }
        ]
    }"#;
    assert!(env
        .run(&["milestone", "create", "--json", json, "--format", "json"])
        .status
        .success());

    // Find the created milestone file and inject a finding with a bad
    // thread-entry timestamp, written directly to disk.
    let dir = env.tmp.path().join("master-plan/milestones");
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .expect("milestone file")
        .path();
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["findings"] = serde_json::json!([{
        "id": "F-01", "severity": "high", "category": "correctness",
        "description": "bad thread", "status": "open", "author": "test",
        "fixed_in": "", "created": "", "resolved": "",
        "thread": [{ "author": "test", "at": "not-a-timestamp", "body": "x" }]
    }]);
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).unwrap()),
    )
    .unwrap();

    let report = validate_warnings(&env);
    let has_w53 = report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "W53");
    assert!(
        has_w53,
        "invalid thread entry should produce W53; got: {}",
        report["warnings"]
    );
}
