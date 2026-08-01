use crate::common::TestEnv;

/// Milestone create with valid minimal input succeeds.
#[test]
fn milestone_create_valid_minimal_succeeds() {
    let env = TestEnv::new();

    let json = r#"{"title": "Valid milestone", "intent": {"outcome": "x"}, "problem": {"description": "y"}, "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]}}"#;
    let out = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    assert!(
        out.status.success(),
        "minimal valid create should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(created["milestone"]["id"].as_str().is_some());
}

/// Completely invalid JSON fails with schema error.
#[test]
fn milestone_create_invalid_fails() {
    let env = TestEnv::new();

    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        "not json",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "invalid json should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty());
}
