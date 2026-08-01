//! M111 S3: `mp milestone update --json --replace-arrays` must persist the
//! `steps` array when present in the input. Pre-M111 the flag returned
//! `{ ok: true }` but silently dropped the steps array. The same migration
//! escape hatch is also used to replace `acceptance_criteria`.

mod common;

use crate::common::TestEnv;

#[test]
fn replace_arrays_persists_steps_array() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Replace steps + acceptance_criteria in one shot. The fixture has 3 ACs
    // and a non-empty steps array; we wipe both and supply fresh values.
    let payload = r#"{
        "acceptance_criteria": [
            {"description": "Replacement AC", "verification": "manual: post-merge"}
        ],
        "steps": [
            {"id": "S99", "work_package": "WP1", "order": 1,
             "action": "replacement step", "files": [],
             "tests": "manual: replaced", "done_when": "replaced",
             "status": "pending", "covers_ac": [], "depends_on_steps": []}
        ]
    }"#;
    let out = env.run(&[
        "milestone",
        "update",
        "03",
        "--json",
        payload,
        "--replace-arrays",
    ]);
    assert!(
        out.status.success(),
        "update --replace-arrays failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Both arrays must be reflected on disk.
    let list = env.run(&["milestone", "ac", "list", "03"]);
    assert!(list.status.success());
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = arr.as_array().expect("array");
    assert_eq!(arr.len(), 1, "replace-arrays must wipe + replace ACs");
    assert_eq!(arr[0]["description"], "Replacement AC");

    let show = env.run(&["milestone", "step", "show", "03", "S99"]);
    assert!(
        show.status.success(),
        "replacement step S99 missing after replace-arrays: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(value["id"], "S99");
    assert_eq!(value["action"], "replacement step");
    assert_eq!(value["work_package"], "WP1");
}

#[test]
fn replace_arrays_without_steps_does_not_drop_existing_steps() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Snapshot the original step ids.
    let before = env.run(&["list", "steps", "--milestone", "03"]);
    assert!(before.status.success());
    let before_value: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    let before_ids: Vec<String> = before_value["steps"]
        .as_array()
        .expect("array")
        .iter()
        .map(|s| s["step"]["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!before_ids.is_empty(), "fixture must have steps");

    // Update only acceptance_criteria via replace-arrays, with no steps field.
    let payload = r#"{
        "acceptance_criteria": [
            {"description": "AC-only", "verification": "manual: ac-only"}
        ]
    }"#;
    let out = env.run(&[
        "milestone",
        "update",
        "03",
        "--json",
        payload,
        "--replace-arrays",
    ]);
    assert!(
        out.status.success(),
        "update with only acceptance_criteria failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Steps must be untouched.
    let after = env.run(&["list", "steps", "--milestone", "03"]);
    assert!(after.status.success());
    let after_value: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    let after_ids: Vec<String> = after_value["steps"]
        .as_array()
        .expect("array")
        .iter()
        .map(|s| s["step"]["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        after_ids, before_ids,
        "replace-arrays on ACs must leave steps alone"
    );
}
