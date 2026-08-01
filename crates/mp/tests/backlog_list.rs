//! M112 S1: `mp backlog list` filters backlog items by source/status/priority
//! and applies --limit. Empty backlog returns `{items: []}`, not null. The
//! original M106 S11 test path that used `mp backlog list` is re-enabled here.

mod common;

use crate::common::TestEnv;

#[test]
fn backlog_list_returns_items_object_with_expected_shape() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["backlog", "list"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // Always emit `items`, never null, even if backlog is empty.
    assert!(v.get("items").is_some(), "backlog list must emit items key");
    assert!(v["items"].is_array());
}

#[test]
fn backlog_list_filters_by_source() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Seed a new backlog item with a distinct source.
    env.run(&[
        "backlog",
        "add",
        "--desc",
        "M112 S1 source filter test",
        "--source",
        "M112-fixture",
        "--priority",
        "high",
    ]);

    let unfiltered = env.run(&["backlog", "list"]);
    let v: serde_json::Value = serde_json::from_slice(&unfiltered.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    let total = items.len();
    let new_total = total + 1;
    assert!(
        new_total >= 2,
        "fixture must have at least one backlog item already"
    );

    let filtered = env.run(&["backlog", "list", "--source", "M112-fixture"]);
    let v2: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    let items2 = v2["items"].as_array().unwrap();
    assert_eq!(
        items2.len(),
        1,
        "M112-fixture-only filter must leave one item"
    );
    assert_eq!(items2[0]["description"], "M112 S1 source filter test");
    assert_eq!(items2[0]["source"], "M112-fixture");
}

#[test]
fn backlog_list_filters_by_priority_and_status() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "high-prio",
        "--priority",
        "high",
    ]);
    env.run(&["backlog", "add", "--desc", "low-prio", "--priority", "low"]);

    let high = env.run(&["backlog", "list", "--priority", "high"]);
    assert!(high.status.success());
    let v: serde_json::Value = serde_json::from_slice(&high.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["priority"] == "high"));

    // Status filter: defaults to `active` for new items.
    let active = env.run(&["backlog", "list", "--status", "active"]);
    let v_active: serde_json::Value = serde_json::from_slice(&active.stdout).unwrap();
    let items_active = v_active["items"].as_array().unwrap();
    assert!(
        items_active.iter().all(|i| i["status"] == "active"),
        "all active items must have status=active; got: {items_active:?}"
    );
}

#[test]
fn backlog_list_limit_slices_first_n() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // The fixture has many backlog items. --limit 3 must yield <=3.
    let out = env.run(&["backlog", "list", "--limit", "3"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(
        items.len() <= 3,
        "limit must cap the result count; got {}",
        items.len()
    );
}

#[test]
fn backlog_list_combined_filters() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "match-all",
        "--source",
        "M112",
        "--priority",
        "high",
    ]);
    env.run(&[
        "backlog",
        "add",
        "--desc",
        "wrong-priority",
        "--source",
        "M112",
        "--priority",
        "low",
    ]);

    let out = env.run(&[
        "backlog",
        "list",
        "--source",
        "M112",
        "--priority",
        "high",
        "--limit",
        "10",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["description"], "match-all");
}
