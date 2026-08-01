//! M93 AC-02: `mp milestone step show <id> <step-id>` returns only the requested
//! step object — not the full milestone document.

use crate::common::{lib_api, TestEnv};

#[test]
fn step_show_returns_only_requested_fragment() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // M03 (oauth-login) is approved → spec_status=ready → has steps.
    let out = lib_api::run(
        &env,
        &["milestone", "step", "show", "03", "S1", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "step show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("step show returns valid json");

    // Fragment-only contract: must be a step object, not the milestone.
    let obj = value.as_object().expect("step show returns an object");
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(|s| s.as_str()).collect();
    let expected: std::collections::BTreeSet<&str> = [
        "action",
        "claimed_at",
        "claimed_by",
        "covers_ac",
        "depends_on_steps",
        "done_when",
        "evidence",
        "files",
        "id",
        "lease_expires_at",
        "order",
        "status",
        "tests",
        "work_package",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        keys,
        expected,
        "step fragment keys mismatch: extra={:?} missing={:?}",
        keys.difference(&expected).collect::<Vec<_>>(),
        expected.difference(&keys).collect::<Vec<_>>(),
    );

    // Stable id selector: the returned step is the one we asked for.
    assert_eq!(obj.get("id").and_then(|v| v.as_str()), Some("S1"));
    assert!(
        !obj.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty(),
        "step action should be populated"
    );

    // Negative: unknown step id fails with a structured error.
    let bad = lib_api::run(
        &env,
        &["milestone", "step", "show", "03", "S99", "--format", "json"],
    );
    assert!(!bad.status.success(), "step show on unknown step must fail");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("S99"),
        "error should mention the missing step id, got: {stderr}"
    );
}

/// Outline ids like `S1.2` should also resolve cleanly.
#[test]
fn step_show_handles_outline_substeps_when_present() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Walkthrough fixture may not have substeps; split S1 to create one, then show.
    // Skip the split if S1.1 already exists (idempotent across reruns is not
    // required — the test reads whichever step currently exists).
    let split = lib_api::run(
        &env,
        &["milestone", "step", "split", "03", "S1", "--format", "json"],
    );
    // split may succeed or fail (e.g. S1 done already). Either way we try to show S1.1.
    let _ = split;

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "show",
            "03",
            "S1.1",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step show S1.1 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["id"], "S1.1");
}
