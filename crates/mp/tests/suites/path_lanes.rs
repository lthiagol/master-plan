//! M102 R3 (F-02 / F-06 / F-07): the `mp path --all / --lane` lane
//! engine integration test. Pins the wire-format contract that raul
//! M103 consumes: 5 lanes with item_type per lane, per-lane head,
//! --no-ideas strips ideas, --summary returns per-lane counts.

use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn path_lanes_default_returns_execution_lane() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["path", "--all", "--format", "json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lanes = v["lanes"].as_array().expect("lanes array");
    assert!(!lanes.is_empty(), "lanes array must not be empty");
    // Each lane has a name + item_type + count.
    for lane in lanes {
        assert!(lane["name"].is_string(), "lane.name must be a string");
        assert!(
            lane["item_type"].is_string(),
            "lane.item_type must be a string; got {}",
            lane
        );
    }
    // The default 5 lanes are blocked / execution / review / grooming / backlog.
    let names: Vec<&str> = lanes.iter().map(|l| l["name"].as_str().unwrap()).collect();
    for expected in ["blocked", "execution", "review", "grooming", "backlog"] {
        assert!(
            names.contains(&expected),
            "default 5-lane set must include {expected}; got {names:?}"
        );
    }
}

#[test]
fn path_lanes_by_name_resolves() {
    let env = TestEnv::new();
    for name in ["blocked", "execution", "review", "grooming", "backlog"] {
        let out = lib_api::run(&env, &["path", "--lane", name, "--format", "json"]);
        assert!(
            out.status.success(),
            "mp path --lane {name} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // An unknown lane errors with a clear message.
    let out = lib_api::run(&env, &["path", "--lane", "not-a-lane", "--format", "json"]);
    assert!(!out.status.success(), "mp path --lane not-a-lane must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown lane") || stderr.contains("not-a-lane"),
        "stderr should mention unknown lane; got {stderr}"
    );
}

#[test]
fn path_lanes_item_type_per_lane() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["path", "--all", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lanes = v["lanes"].as_array().unwrap();

    // Each lane must have an item_type that distinguishes milestones from
    // backlog items. The execution / review / grooming lanes carry
    // milestones; the backlog lane carries backlog items.
    for lane in lanes {
        let name = lane["name"].as_str().unwrap();
        let item_type = lane["item_type"].as_str().unwrap();
        match name {
            // awaiting-approval was added by M157 (path tree view).
            "blocked" | "execution" | "review" | "grooming" | "awaiting-approval" => {
                assert_eq!(
                    item_type, "milestone",
                    "{name} should carry milestones; got {item_type}"
                );
            }
            "backlog" => {
                assert_eq!(
                    item_type, "backlog_item",
                    "backlog should carry backlog_items; got {item_type}"
                );
            }
            _ => panic!("unexpected lane name: {name}"),
        }
    }
}

#[test]
fn path_lanes_summary_per_lane_counts() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["path", "--summary", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // --summary returns a lanes array (each lane carries item_count).
    let lanes = v["lanes"].as_array().expect("lanes array");
    assert!(!lanes.is_empty(), "--summary must return lanes");
    for lane in lanes {
        assert!(lane["name"].is_string(), "lane.name should be a string");
        assert!(
            lane["item_count"].is_number(),
            "lane.item_count should be a number"
        );
    }
}
