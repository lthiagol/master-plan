use crate::common::TestEnv;

/// --checklist-type milestone works (canonical).
#[test]
fn interview_checklist_type_canonical() {
    let env = TestEnv::new();
    let out = env.run(&[
        "interview",
        "checklist",
        "--checklist-type",
        "milestone",
        "--id",
        "01",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["type"], "milestone",
        "canonical --checklist-type should work"
    );
}

/// --type milestone works (deprecated alias) and emits a deprecation warning.
#[test]
fn interview_checklist_type_deprecated_alias() {
    let env = TestEnv::new();
    let out = env.run(&[
        "interview",
        "checklist",
        "--type",
        "milestone",
        "--id",
        "01",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("deprecated"),
        "deprecation warning should appear on stderr, got: {stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["type"], "milestone", "deprecated --type should work");
}

/// --checklist-type brief works.
#[test]
fn interview_checklist_brief() {
    let env = TestEnv::new();
    let out = env.run(&[
        "interview",
        "checklist",
        "--checklist-type",
        "brief",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["type"], "brief");
}
