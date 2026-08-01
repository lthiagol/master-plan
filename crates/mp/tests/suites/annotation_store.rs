use crate::common::TestEnv;

/// Annotation storage: init creates annotations.json, validate passes.
#[test]
fn annotation_init_creates_file() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");
    assert!(
        annotations_path.exists(),
        "annotations.json should exist after init full"
    );
}

/// Annotation storage: save valid annotation and load back.
#[test]
fn annotation_save_load_round_trip() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");

    let json = serde_json::json!({
        "annotations": [{
            "id": "AN-01",
            "target": "M03",
            "kind": "approval-request",
            "body": "Please approve this milestone before executing",
            "author": "human",
            "status": "open",
            "created_at": "2026-06-28",
            "resolved_at": "",
        }]
    });
    std::fs::write(
        &annotations_path,
        format!("{}\n", serde_json::to_string_pretty(&json).unwrap()),
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["ok"].as_bool().unwrap(),
        "validate should pass for valid annotation"
    );
}
