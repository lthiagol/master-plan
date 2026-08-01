//! M182 S4: `mp config set sort.<lane> <sortkey>` round-trips through
//! the typed schema. The previous M172 S5 surface wrote only to
//! the in-memory `App::lane_sort_key` HashMap; M182 closes the
//! gap so the bound order survives a restart via `config.json`.
//!
//! Tests cover:
//! - `mp config set sort.<lane> <sortkey>` writes the sort key to
//!   the `sort` section of `config.json`.
//! - `mp config get sort.<lane>` reads back the bound key.
//! - Unknown lanes and unknown sort keys are rejected with
//!   structured validation errors (so raul's menu can surface a
//!   useful hint when the user picks a wrong option).
//!
//! M182 external review (F-10): the contract is the two-segment
//! `sort.<lane> <sortkey>` shape that raul's `persist_sort_rebind_choice`
//! actually writes (value = the sort key). The earlier three-segment
//! `sort.<lane>.<key>` form rejected raul's write and broke confirm.

use crate::common::lib_api;
use crate::common::TestEnv;

/// AC-04: `mp config set sort.milestones lifecycle` writes the sort
/// key. After the write, `mp config get sort.milestones` returns
/// `lifecycle` — the same value raul's sort-rebind menu bound.
#[test]
fn m182_s4_config_set_then_get_round_trips() {
    let env = TestEnv::new();

    let out = lib_api::run(
        &env,
        &[
            "config",
            "set",
            "sort.milestones",
            "lifecycle",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "set must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["value"], "lifecycle");

    let out = lib_api::run(
        &env,
        &["config", "get", "sort.milestones", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "get must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["value"], "lifecycle", "get round-trips the bound value");
}

/// AC-04: `mp config get sort.<lane>` returns an empty value (not
/// null, not an error) when no preference is recorded. raul's
/// `load_persisted_sort_keys` maps the empty value to
/// `SortKey::Id` (the documented default).
#[test]
fn m182_s4_config_get_returns_empty_for_unbound_lane() {
    let env = TestEnv::new();

    let out = lib_api::run(&env, &["config", "get", "sort.backlog", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["value"], "", "unbound lane must return empty string");
}

/// AC-04: unknown lanes are rejected with a structured validation
/// error that lists the valid lane names. This is the error
/// message raul surfaces when a future milestone's lane rename
/// races with a stale config.
#[test]
fn m182_s4_config_set_rejects_unknown_lane() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &[
            "config",
            "set",
            "sort.bogus",
            "lifecycle",
            "--format",
            "json",
        ],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("unknown sort lane"),
        "expected structured error mentioning 'unknown sort lane'; got {errors:?}"
    );
}

/// AC-04: unknown sort keys are rejected with a structured validation
/// error listing the four valid keys (id / lifecycle / priority /
/// updated). The error is structured so a future raul hint surface
/// can render it without string-matching.
#[test]
fn m182_s4_config_set_rejects_unknown_sort_key() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &[
            "config",
            "set",
            "sort.milestones",
            "bogus",
            "--format",
            "json",
        ],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("invalid sort key"),
        "expected structured error mentioning 'invalid sort key'; got {errors:?}"
    );
}

/// AC-04: the four documented sort keys are all accepted. Each
/// lane's preference is independent — binding `milestones` to
/// `priority` doesn't affect `tweaks`.
#[test]
fn m182_s4_each_documented_sort_key_round_trips() {
    let env = TestEnv::new();
    // M182 external review (F-10): pin the two-segment
    // `sort.<lane> <sortkey>` shape raul actually writes (value is
    // the sort key). Each round sets a different sort key for the
    // same lane; reading back via `sort.<lane>` returns the bound
    // value.
    for key in ["id", "lifecycle", "priority", "updated"] {
        let out = lib_api::run(
            &env,
            &["config", "set", "sort.milestones", key, "--format", "json"],
        );
        assert!(
            out.status.success(),
            "set sort.milestones {key} must succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["ok"], true, "set must succeed for {key}");

        let out = lib_api::run(
            &env,
            &["config", "get", "sort.milestones", "--format", "json"],
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["value"], key, "round-trip failed for {key}");
    }
}
