//! M111 S2: `mp milestone ac add` must auto-increment the AC id and append,
//! mirroring `mp milestone step add`. Pre-M111 the dogfood log flagged a
//! regression where two sequential `ac add` calls both returned `id: "AC-01"`
//! and only the second survived. Pinned by a regression test that adds two
//! ACs and asserts both are listed.

mod common;

use crate::common::TestEnv;

#[test]
fn ac_add_appends_two_acs_on_fresh_milestone() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // First add: fresh AC-04 on the fixture (which already has AC-01..AC-03).
    let add1 = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "first appended ac",
        "--verification",
        "manual: first",
    ]);
    assert!(
        add1.status.success(),
        "first add failed: {}",
        String::from_utf8_lossy(&add1.stderr)
    );
    let v1: serde_json::Value = serde_json::from_slice(&add1.stdout).unwrap();
    assert_eq!(
        v1["acceptance_criterion"]["id"].as_str(),
        Some("AC-04"),
        "first add should land at AC-04 (fixture has AC-01..AC-03)"
    );

    // Second add: should land at AC-05, NOT overwrite AC-04.
    let add2 = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "second appended ac",
        "--verification",
        "manual: second",
    ]);
    assert!(
        add2.status.success(),
        "second add failed: {}",
        String::from_utf8_lossy(&add2.stderr)
    );
    let v2: serde_json::Value = serde_json::from_slice(&add2.stdout).unwrap();
    assert_eq!(
        v2["acceptance_criterion"]["id"].as_str(),
        Some("AC-05"),
        "second add should land at AC-05; not overwrite AC-04"
    );

    // Both ACs are listed — regression gate.
    let list = env.run(&["milestone", "ac", "list", "03"]);
    assert!(list.status.success());
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = arr.as_array().expect("array");
    assert_eq!(arr.len(), 5, "list should contain AC-01..AC-05");
    assert_eq!(arr[3]["id"], "AC-04");
    assert_eq!(arr[3]["description"], "first appended ac");
    assert_eq!(arr[4]["id"], "AC-05");
    assert_eq!(arr[4]["description"], "second appended ac");
}

#[test]
fn ac_add_appends_on_milestone_with_zero_existing_acs() {
    // Harder edge case: start from a milestone that has zero ACs. The
    // auto-increment must still produce AC-01, AC-02 (not both AC-01).
    let env = TestEnv::blank();
    let init = env.run(&["init", "--profile", "full"]);
    assert!(
        init.status.success(),
        "mp init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"zero-ac","intent":{"outcome":"x"},"problem":{"description":"x"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
    ]);
    assert!(
        create.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap().to_string();

    let add1 = env.run(&[
        "milestone",
        "criterion",
        "add",
        &id,
        "--description",
        "first",
        "--verification",
        "manual: first",
    ]);
    assert!(
        add1.status.success(),
        "first add failed: {}",
        String::from_utf8_lossy(&add1.stderr)
    );
    let v1: serde_json::Value = serde_json::from_slice(&add1.stdout).unwrap();
    assert_eq!(v1["acceptance_criterion"]["id"].as_str(), Some("AC-01"));

    let add2 = env.run(&[
        "milestone",
        "criterion",
        "add",
        &id,
        "--description",
        "second",
        "--verification",
        "manual: second",
    ]);
    assert!(
        add2.status.success(),
        "second add failed: {}",
        String::from_utf8_lossy(&add2.stderr)
    );
    let v2: serde_json::Value = serde_json::from_slice(&add2.stdout).unwrap();
    assert_eq!(v2["acceptance_criterion"]["id"].as_str(), Some("AC-02"));

    let list = env.run(&["milestone", "ac", "list", &id]);
    assert!(list.status.success());
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = arr.as_array().expect("array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["id"], "AC-01");
    assert_eq!(arr[1]["id"], "AC-02");
}

#[test]
fn ac_add_after_remove_does_not_collide_with_surviving_ac() {
    // M111 external-review regression (2026-07-07): the original auto-increment
    // used `len() + 1`, which collides after a removal. Removing AC-02 from
    // [AC-01, AC-02, AC-03] leaves len()=2; the len-based formula produced
    // AC-03 again, duplicating the surviving AC-03. The fix derives the next
    // id from the max existing numeric suffix (parity with step::next_step_id).
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Fixture milestone 03 starts with AC-01, AC-02, AC-03.
    // Remove AC-02 (it is uncovered in the fixture, so removal succeeds).
    let rm = env.run(&["milestone", "ac", "remove", "03", "AC-02"]);
    assert!(
        rm.status.success(),
        "remove AC-02 failed: {}",
        String::from_utf8_lossy(&rm.stderr)
    );

    // After removal: [AC-01, AC-03] (len=2, but max suffix=3).
    let list_after_remove = env.run(&["milestone", "ac", "list", "03"]);
    let arr: serde_json::Value = serde_json::from_slice(&list_after_remove.stdout).unwrap();
    let ids: Vec<String> = arr
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec!["AC-01", "AC-03"]);

    // Adding a new AC must produce AC-04 (max suffix 3 + 1), NOT AC-03
    // (which the buggy len-based formula returned, duplicating the survivor).
    let add = env.run(&[
        "milestone",
        "ac",
        "add",
        "03",
        "--description",
        "post-remove add",
        "--verification",
        "manual: post-remove",
    ]);
    assert!(
        add.status.success(),
        "add after remove failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(
        v["acceptance_criterion"]["id"].as_str(),
        Some("AC-04"),
        "add after remove must derive from max suffix, not len"
    );

    // No duplicate ids survive on disk.
    let list = env.run(&["milestone", "ac", "list", "03"]);
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let ids: Vec<String> = arr
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        ids.len(),
        sorted.len(),
        "duplicate AC ids after add: {ids:?}"
    );
    assert_eq!(ids, vec!["AC-01", "AC-03", "AC-04"]);
}
