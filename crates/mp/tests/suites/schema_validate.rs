use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn schema_rejects_invalid_milestone_create_json() {
    let env = TestEnv::new();

    let bad = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{"scope":{"in_scope":["x"],"out_of_scope":["a","b"]}}"#,
            "--format",
            "json",
        ],
    );
    assert!(!bad.status.success());
    let err = String::from_utf8_lossy(&bad.stderr);
    assert!(
        err.contains("title is required") || err.contains("schema validation failed"),
        "expected schema/title error, got: {err}"
    );
}
