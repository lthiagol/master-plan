//! M101 R3: schema enum acceptance — phase and confidence enums
//! accept `""` as the legacy default. Pinned by feeding the schema
//! validator a finding fixture with phase="" + confidence="" and
//! asserting mp validate passes.

use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn schema_enums_accept_empty_string() {
    let env = TestEnv::new();

    // Stand up a milestone + a finding with phase='' + confidence=''
    // (the legacy M125 / pre-M105 default shapes). The schema
    // validator must accept both as valid enum values.
    let payload = serde_json::json!({
        "title": "schema-enum-empty",
        "intent": { "outcome": "schema enum acceptance regression" },
        "problem": { "description": "phase='' / confidence='' are legacy defaults" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "ac", "verification": "echo ok" }],
        "spec_status": "ready",
    })
    .to_string();
    let out = lib_api::run(&env, &["milestone", "create", "--json", &payload]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap().to_string();

    // Legacy CLI path: phase defaults to '' via add_finding (legacy).
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "low",
            "--category",
            "correctness",
            "--desc",
            "phase-empty + confidence-empty legacy",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // mp validate must accept the milestone (no schema enum error).
    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp validate must pass for a milestone with legacy phase='' finding; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
