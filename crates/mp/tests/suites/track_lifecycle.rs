use crate::common::TestEnv;

#[test]
fn track_start_and_cancel_lifecycle() {
    let env = TestEnv::new();

    let add = env.run(&[
        "track",
        "add",
        "tweak",
        "--title",
        "Button padding",
        "--problem",
        "Too tight",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let item_id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let start = env.run(&["track", "start", "tweak", &item_id, "--format", "json"]);
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let start_json: serde_json::Value = serde_json::from_slice(&start.stdout).unwrap();
    assert_eq!(start_json["status"], "in-progress");

    let cancel = env.run(&["track", "cancel", "tweak", &item_id, "--format", "json"]);
    assert!(
        cancel.status.success(),
        "{}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    let cancel_json: serde_json::Value = serde_json::from_slice(&cancel.stdout).unwrap();
    assert!(
        cancel_json["status"] == "cancelled" || cancel_json["status"] == "archived",
        "cancel should archive or cancel item, got: {}",
        cancel_json["status"]
    );
}

#[test]
fn backlog_resolve_marks_item_resolved() {
    let env = TestEnv::new();

    let add = env.run(&[
        "backlog",
        "add",
        "--desc",
        "Defer OAuth provider",
        "--format",
        "json",
    ]);
    assert!(add.status.success());
    let id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resolve = env.run(&[
        "backlog",
        "resolve",
        &id,
        "--reason",
        "handled in M03",
        "--format",
        "json",
    ]);
    assert!(
        resolve.status.success(),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let resolved: serde_json::Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(resolved["item"]["status"], "resolved");
    assert_eq!(resolved["item"]["resolution"], "handled in M03");
}
