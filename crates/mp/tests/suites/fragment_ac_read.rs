//! M93 AC-01: `mp milestone ac show` and `mp milestone ac list` return only
//! acceptance-criterion fragments — no full milestone document.
//!
//! These commands are the agent-friendly read path: when an agent only needs one
//! AC (e.g. "what's the verification for AC-03?"), it must not load the whole
//! milestone document. See docs/concepts/01 - Agent Integration/AGENT-READINESS.md.

use crate::common::{lib_api, TestEnv};

/// `mp milestone ac show <id> <AC-id>` returns only the requested AC object.
#[test]
fn ac_show_returns_only_requested_fragment() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", "AC-03", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "ac show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("ac show returns valid json");

    // Fragment-only contract: top-level keys are exactly the AC fields.
    let obj = value.as_object().expect("ac show returns an object");
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(|s| s.as_str()).collect();
    assert_eq!(
        keys,
        ["description", "evidence", "id", "status", "verification"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        "ac show returned unexpected keys: {keys:?}"
    );

    // Stable id selector: the returned AC is the one we asked for.
    assert_eq!(obj.get("id").and_then(|v| v.as_str()), Some("AC-03"));
    assert_eq!(
        obj.get("status").and_then(|v| v.as_str()),
        Some("pending"),
        "AC-03 should be pending in the fixture"
    );
    assert!(
        !obj.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty(),
        "AC-03 description should be populated"
    );

    // Negative: unknown AC id fails with a structured error.
    let bad = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", "AC-99", "--format", "json"],
    );
    assert!(!bad.status.success(), "ac show on unknown AC must fail");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("AC-99"),
        "error should mention the missing AC id, got: {stderr}"
    );
}

/// `mp milestone ac list <id>` returns a JSON array of AC fragments (not a wrapper).
#[test]
fn ac_list_returns_array_of_fragments() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(&env, &["milestone", "ac", "list", "03", "--format", "json"]);
    assert!(
        out.status.success(),
        "ac list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("ac list returns valid json");

    let arr = value.as_array().expect("ac list returns an array");
    assert!(arr.len() >= 3, "fixture M03 has 3+ ACs, got {}", arr.len());
    for ac in arr {
        let obj = ac.as_object().expect("each AC is an object");
        let keys: std::collections::BTreeSet<&str> = obj.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            keys,
            ["description", "evidence", "id", "status", "verification"]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            "list fragment has unexpected keys: {keys:?}"
        );
        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
        assert!(id.starts_with("AC-"), "id should be AC-NN, got {id}");
    }
    // Stable ids preserved in order
    let ids: Vec<&str> = arr
        .iter()
        .map(|v| v.get("id").and_then(|s| s.as_str()).unwrap_or(""))
        .collect();
    assert_eq!(ids[0], "AC-01");
    assert_eq!(ids[2], "AC-03");
}
