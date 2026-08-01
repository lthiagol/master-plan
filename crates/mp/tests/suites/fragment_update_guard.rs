//! M93 AC-08: `mp milestone update` rejects `acceptance_criteria` and `steps`
//! arrays in --json unless `--replace-arrays` is set. Fragment commands
//! (`mp milestone ac …`, `mp milestone step …`) remain the agent path.

use crate::common::{lib_api, TestEnv};

#[test]
fn update_rejects_acceptance_criteria_by_default() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let payload = r#"{
        "acceptance_criteria": [
            {"description": "x", "verification": "manual: y"}
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            "03",
            "--json",
            payload,
            "--format",
            "json",
        ],
    );
    assert!(
        !out.status.success(),
        "update with acceptance_criteria array must fail by default"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("acceptance_criteria"),
        "error must mention acceptance_criteria; got: {stderr}"
    );
    assert!(
        stderr.contains("guarded") || stderr.contains("replace-arrays"),
        "error must point to the guard / --replace-arrays; got: {stderr}"
    );
}

#[test]
fn update_rejects_steps_by_default() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let payload = r#"{
        "steps": [
            {"id": "S99", "action": "x", "tests": "manual", "done_when": "y"}
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            "03",
            "--json",
            payload,
            "--format",
            "json",
        ],
    );
    assert!(
        !out.status.success(),
        "update with steps array must fail by default"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("steps"),
        "error must mention steps; got: {stderr}"
    );
    assert!(
        stderr.contains("guarded") || stderr.contains("replace-arrays"),
        "error must point to the guard / --replace-arrays; got: {stderr}"
    );
}

#[test]
fn update_with_replace_arrays_allows_acceptance_criteria() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // With --replace-arrays, the migration escape hatch works.
    let payload = r#"{
        "acceptance_criteria": [
            {"description": "Replacement AC", "verification": "manual: post-merge"}
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            "03",
            "--json",
            payload,
            "--replace-arrays",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "update with --replace-arrays failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);

    // After replace, only the supplied AC remains.
    let list = lib_api::run(&env, &["milestone", "ac", "list", "03", "--format", "json"]);
    assert!(list.status.success());
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = arr.as_array().expect("array");
    assert_eq!(arr.len(), 1, "replace-arrays must wipe and replace");
    assert_eq!(arr[0]["description"], "Replacement AC");
}

#[test]
fn update_without_guarded_keys_unaffected() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Plain scalar update: still works as before.
    let payload = r#"{"intent": {"outcome": "Updated outcome."}}"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            "03",
            "--json",
            payload,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "scalar update regressed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
