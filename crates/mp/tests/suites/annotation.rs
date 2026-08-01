use crate::common::TestEnv;

#[test]
fn annotation_create_list_show() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M03",
        "review-request",
        "Please review the approach",
        "alice",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["ok"].as_bool().unwrap());
    assert_eq!(json["annotation"]["target"].as_str().unwrap(), "M03");
    assert_eq!(
        json["annotation"]["kind"].as_str().unwrap(),
        "review-request"
    );
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "open");
    let id1 = json["annotation"]["id"].as_str().unwrap().to_string();

    let out = env.run(&[
        "annotation",
        "create",
        "M04",
        "approval-request",
        "Block M04 until reviewed",
        "bob",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id2 = json["annotation"]["id"].as_str().unwrap().to_string();

    assert_ne!(id1, id2);

    let out = env.run(&["annotation", "list", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = json["annotations"].as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let out = env.run(&["annotation", "list", "--open", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = json["annotations"].as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let out = env.run(&["annotation", "list", "--target", "M03", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = json["annotations"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"].as_str().unwrap(), id1);

    let out = env.run(&["annotation", "show", &id1, "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["id"].as_str().unwrap(), id1);
}

#[test]
fn annotation_create_rejects_invalid_kind() {
    let env = TestEnv::new();
    let out = env.run(&[
        "annotation",
        "create",
        "M01",
        "bogus-kind",
        "body",
        "alice",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
}

#[test]
fn annotation_create_rejects_empty_target() {
    let env = TestEnv::new();
    let out = env.run(&[
        "annotation",
        "create",
        "",
        "note",
        "body",
        "alice",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
}

#[test]
fn annotation_show_nonexistent() {
    let env = TestEnv::new();
    let out = env.run(&["annotation", "show", "AN-99", "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn annotation_update() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M05",
        "note",
        "Initial body",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    let out = env.run(&[
        "annotation",
        "update",
        &id,
        "--body",
        "Updated body",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["body"].as_str().unwrap(), "Updated body");
}

#[test]
fn annotation_update_rejects_if_not_open() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M05",
        "note",
        "body",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    env.run(&["annotation", "resolve", &id, "--format", "json"]);

    let out = env.run(&[
        "annotation",
        "update",
        &id,
        "--body",
        "New body",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
}

#[test]
fn annotation_lifecycle_open_addressed_resolved() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M06",
        "review-request",
        "Please review",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "open");

    let out = env.run(&["annotation", "addressed", &id, "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "addressed");

    let out = env.run(&["annotation", "resolve", &id, "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "resolved");
    assert!(!json["annotation"]["resolved_at"]
        .as_str()
        .unwrap()
        .is_empty());
}

#[test]
fn annotation_lifecycle_open_resolved_direct() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M07",
        "note",
        "Direct resolve",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    let out = env.run(&["annotation", "resolve", &id, "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "resolved");
}

#[test]
fn annotation_reopen() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M08",
        "note",
        "Reopen test",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    env.run(&["annotation", "resolve", &id, "--format", "json"]);

    let out = env.run(&["annotation", "reopen", &id, "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotation"]["status"].as_str().unwrap(), "open");
    assert_eq!(json["annotation"]["resolved_at"].as_str().unwrap(), "");
}

#[test]
fn annotation_reopen_from_addressed_rejected() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M09",
        "note",
        "test",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    env.run(&["annotation", "addressed", &id, "--format", "json"]);

    let out = env.run(&["annotation", "reopen", &id, "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn annotation_resolve_from_resolved_rejected() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M10",
        "note",
        "test",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    env.run(&["annotation", "resolve", &id, "--format", "json"]);

    let out = env.run(&["annotation", "resolve", &id, "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn annotation_addressed_from_resolved_rejected() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M11",
        "note",
        "test",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    env.run(&["annotation", "resolve", &id, "--format", "json"]);

    let out = env.run(&["annotation", "addressed", &id, "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn annotation_remove() {
    let env = TestEnv::new();

    let out = env.run(&[
        "annotation",
        "create",
        "M12",
        "note",
        "To be removed",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json["annotation"]["id"].as_str().unwrap().to_string();

    let out = env.run(&["annotation", "remove", &id, "--format", "json"]);
    assert!(out.status.success());

    let out = env.run(&["annotation", "show", &id, "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn annotation_list_filters() {
    let env = TestEnv::new();

    env.run(&[
        "annotation",
        "create",
        "M01",
        "note",
        "Note by alice",
        "alice",
        "--format",
        "json",
    ]);
    env.run(&[
        "annotation",
        "create",
        "M02",
        "approval-request",
        "Request by bob",
        "bob",
        "--format",
        "json",
    ]);

    let out = env.run(&[
        "annotation",
        "list",
        "--kind",
        "approval-request",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations"].as_array().unwrap().len(), 1);

    let out = env.run(&[
        "annotation",
        "list",
        "--author",
        "alice",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations"].as_array().unwrap().len(), 1);

    let out = env.run(&["annotation", "list", "--kind", "bogus", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations"].as_array().unwrap().len(), 0);
}
