use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn milestone_update_if_updated_conflict() {
    let env = TestEnv::new();

    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
            "title": "Conflict test",
            "intent": { "outcome": "Detect stale writes." },
            "problem": { "description": "Race." },
            "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
            "acceptance_criteria": [{ "description": "Works", "verification": "test" }]
        }"#,
            "--format",
            "json",
        ],
    );
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    let conflict = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            id,
            "--json",
            r#"{"title":"New title"}"#,
            "--if-updated",
            "1999-01-01",
        ],
    );
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("write conflict"));
}
