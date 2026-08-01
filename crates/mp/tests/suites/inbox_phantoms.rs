use crate::common::TestEnv;

/// Fresh init yields an empty inbox.
#[test]
fn fresh_init_inbox_is_empty() {
    let env = TestEnv::new();

    let out = env.run(&["inbox", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["count"], 0, "fresh init should have empty inbox");
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
}

/// Fresh init status shows inbox_count 0.
#[test]
fn fresh_init_status_inbox_count_zero() {
    let env = TestEnv::new();

    let out = env.run(&["status", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["inbox_count"], 0, "fresh init inbox_count should be 0");
}
