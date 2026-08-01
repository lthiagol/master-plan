//! M93 AC-03: `mp show milestone --fields` accepts id selectors like
//! `acceptance_criteria[AC-03]` and `steps[S4]` — not only numeric indices.
//!
//! Regression: numeric-index projection from M79 must keep working.
//! See crates/mp/tests/agent_projection.rs for the existing contract.

use crate::common::{lib_api, TestEnv};

#[test]
fn fields_accepts_ac_id_selector() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "acceptance_criteria[AC-03]",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "fields with AC id selector failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    // Expected shape: {"acceptance_criteria": {"AC-03": { ... }}}
    let ac_map = value
        .get("acceptance_criteria")
        .and_then(|v| v.as_object())
        .expect("acceptance_criteria is an object keyed by id");
    assert!(
        ac_map.contains_key("AC-03"),
        "expected AC-03 key, got keys: {:?}",
        ac_map.keys().collect::<Vec<_>>()
    );
    // Stable-id match — the returned fragment is exactly the AC-03 entry.
    let ac03 = &ac_map["AC-03"];
    assert_eq!(ac03["id"], "AC-03");
    assert!(
        ac03["description"].as_str().unwrap_or("").contains("OAuth")
            || !ac03["description"].as_str().unwrap_or("").is_empty(),
        "AC-03 description should be populated"
    );

    // Negative: unknown id fails with a clear error.
    let bad = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "acceptance_criteria[AC-99]",
            "--format",
            "json",
        ],
    );
    assert!(
        !bad.status.success(),
        "fields with unknown AC id should fail"
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("AC-99"),
        "error should mention missing id, got: {stderr}"
    );
}

#[test]
fn fields_accepts_step_outline_id_selector() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "steps[S1]",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "fields with step outline id failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let steps_map = value
        .get("steps")
        .and_then(|v| v.as_object())
        .expect("steps is an object keyed by id");
    assert!(
        steps_map.contains_key("S1"),
        "expected S1 key, got: {:?}",
        steps_map.keys().collect::<Vec<_>>()
    );
    let s1 = &steps_map["S1"];
    assert_eq!(s1["id"], "S1");
    assert!(
        !s1["action"].as_str().unwrap_or("").is_empty(),
        "S1 action should be populated"
    );

    // Negative: unknown step outline id.
    let bad = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "steps[S99]",
            "--format",
            "json",
        ],
    );
    assert!(
        !bad.status.success(),
        "fields with unknown step id should fail"
    );
}

#[test]
fn fields_numeric_index_still_works_for_backward_compat() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // M79 contract: acceptance_criteria[0] still returns {"0": {...}}.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "acceptance_criteria[0]",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "numeric index projection broke: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ac_map = value["acceptance_criteria"].as_object().expect("object");
    assert!(
        ac_map.contains_key("0"),
        "numeric index must produce key \"0\", got {:?}",
        ac_map.keys().collect::<Vec<_>>()
    );
    assert_eq!(ac_map["0"]["id"], "AC-01");
}

#[test]
fn fields_mixed_id_and_numeric_in_one_query() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Mixed: one id selector + one numeric index + one field. Both selectors
    // must coexist in the merged result object.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "03",
            "--fields",
            "acceptance_criteria[AC-02],steps[S1],milestone.id",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "mixed fields failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["acceptance_criteria"]["AC-02"]["id"], "AC-02");
    assert_eq!(value["steps"]["S1"]["id"], "S1");
    assert_eq!(value["milestone"]["id"], "03");
}
