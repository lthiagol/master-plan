use crate::common::TestEnv;

#[test]
fn status_includes_annotations_open() {
    let env = TestEnv::new();

    let out = env.run(&["status", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations_open"].as_u64().unwrap(), 0);

    env.run(&[
        "annotation",
        "create",
        "M01",
        "note",
        "Test note",
        "alice",
        "--format",
        "json",
    ]);

    let out = env.run(&["status", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations_open"].as_u64().unwrap(), 1);
}

#[test]
fn inbox_includes_open_annotations() {
    let env = TestEnv::new();

    let out = env.run(&["inbox", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let has_annotation = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "annotation");
    assert!(!has_annotation, "no annotations yet");

    env.run(&[
        "annotation",
        "create",
        "M01",
        "review-request",
        "Please review",
        "agent",
        "--format",
        "json",
    ]);

    let out = env.run(&["inbox", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let annotation_items: Vec<_> = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "annotation")
        .collect();
    assert!(
        !annotation_items.is_empty(),
        "inbox should contain annotation items"
    );
    let item = &annotation_items[0];
    assert!(item["action"]
        .as_str()
        .unwrap()
        .contains("mp annotation show"));
}

#[test]
fn resolved_annotations_not_in_inbox() {
    let env = TestEnv::new();

    env.run(&[
        "annotation",
        "create",
        "M02",
        "note",
        "Test note",
        "alice",
        "--format",
        "json",
    ]);

    let out = env.run(&["inbox", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let count = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "annotation")
        .count();
    assert_eq!(count, 1);

    // Get id and resolve
    let list_out = env.run(&["annotation", "list", "--format", "json"]);
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let ann_id = list_json["annotations"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["annotation", "resolve", &ann_id, "--format", "json"]);

    let out = env.run(&["inbox", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let count = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "annotation")
        .count();
    assert_eq!(count, 0, "resolved annotations should not appear in inbox");
}

#[test]
fn annotations_open_decrements_on_resolve() {
    let env = TestEnv::new();

    env.run(&[
        "annotation",
        "create",
        "M03",
        "note",
        "Test",
        "alice",
        "--format",
        "json",
    ]);
    env.run(&[
        "annotation",
        "create",
        "M04",
        "note",
        "Test 2",
        "alice",
        "--format",
        "json",
    ]);

    let out = env.run(&["status", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations_open"].as_u64().unwrap(), 2);

    // Resolve first
    let list_out = env.run(&["annotation", "list", "--format", "json"]);
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let ann_id = list_json["annotations"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["annotation", "resolve", &ann_id, "--format", "json"]);

    let out = env.run(&["status", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["annotations_open"].as_u64().unwrap(), 1);
}
