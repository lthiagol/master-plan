use crate::common::{repo_root, run_validate_fixture};

#[test]
fn validate_minimal_ready_fixture() {
    let (code, stdout) = run_validate_fixture("minimal-ready", None);
    assert_eq!(code, 0, "stdout: {stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], true);
    assert!(report["errors"].as_array().unwrap().is_empty());
}

#[test]
fn validate_walkthrough_oauth_fixture() {
    let (code, stdout) = run_validate_fixture("walkthrough-oauth", None);
    assert_eq!(code, 0, "stdout: {stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], true);
}

#[test]
fn validate_hybrid_work_fixture() {
    let (code, stdout) = run_validate_fixture("hybrid-work", Some(".mp"));
    assert_eq!(code, 0, "stdout: {stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], true);
}

#[test]
fn validate_gate_g1_fail_fixture() {
    let (code, stdout) = run_validate_fixture("gate-g1-fail", None);
    assert_ne!(code, 0, "expected validation failure");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], false);
    let codes: Vec<String> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.get("code").and_then(|c| c.as_str()).map(str::to_string))
        .collect();
    assert!(codes.contains(&"G1".to_string()));
}

#[test]
fn validation_never_writes_into_source_fixtures() {
    for fixture in [
        "minimal-ready/master-plan",
        "walkthrough-oauth/master-plan",
        "gate-g1-fail/master-plan",
        "hybrid-work/.mp",
    ] {
        let source = repo_root().join("tests/fixtures/projects").join(fixture);
        for artifact in [".mp-write.lock", "activity.json", ".mp-txn"] {
            assert!(
                !source.join(artifact).exists(),
                "source fixture contains generated artifact: {fixture}/{artifact}"
            );
        }
    }
}
