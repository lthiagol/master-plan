use crate::common::TestEnv;

#[test]
fn validate_passes_for_valid_annotations() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M01",
        "review-request",
        "Please review",
        "alice",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    assert!(
        env.run_validate(),
        "validate should pass with valid annotations"
    );
}

#[test]
fn validate_fails_for_invalid_annotation() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");

    let json = serde_json::json!({
        "annotations": [{
            "id": "AN-01",
            "target": "",
            "kind": "bogus-kind",
            "body": "",
            "author": "",
            "status": "invalid",
            "created_at": "2026-06-28",
            "resolved_at": "",
        }]
    });
    std::fs::write(
        &annotations_path,
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        !out.status.success(),
        "validate should fail for invalid annotation"
    );
    let stderr = String::from_utf8_lossy(&out.stdout);
    assert!(stderr.contains("R1"), "should contain R1 error: {stderr}");
}

#[test]
fn validate_emits_r1_for_empty_target() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");

    let json = serde_json::json!({
        "annotations": [{
            "id": "AN-01",
            "target": "",
            "kind": "note",
            "body": "Some body",
            "author": "alice",
            "status": "open",
            "created_at": "2026-06-28",
            "resolved_at": "",
        }]
    });
    std::fs::write(
        &annotations_path,
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        !out.status.success(),
        "validate should fail for empty target"
    );
}

#[test]
fn validate_emits_r1_for_invalid_kind() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");

    let json = serde_json::json!({
        "annotations": [{
            "id": "AN-01",
            "target": "M01",
            "kind": "bogus",
            "body": "body",
            "author": "alice",
            "status": "open",
            "created_at": "2026-06-28",
            "resolved_at": "",
        }]
    });
    std::fs::write(
        &annotations_path,
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        !out.status.success(),
        "validate should fail for invalid kind"
    );
}

#[test]
fn validate_emits_r1_for_invalid_status() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let annotations_path = env.tmp.path().join("master-plan/annotations.json");

    let json = serde_json::json!({
        "annotations": [{
            "id": "AN-01",
            "target": "M01",
            "kind": "note",
            "body": "body",
            "author": "alice",
            "status": "bogus-status",
            "created_at": "2026-06-28",
            "resolved_at": "",
        }]
    });
    std::fs::write(
        &annotations_path,
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();

    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        !out.status.success(),
        "validate should fail for invalid status"
    );
}
