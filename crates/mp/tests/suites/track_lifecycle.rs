use crate::common::TestEnv;

/// M233 / AC-04: positive integration test for the TrackItem arm of
/// `mp track archive track-item`. Verifies that:
/// 1. `mp track archive track-item <kind> <id>` succeeds (the
///    TrackItem match in `crates/mp/src/commands/archive.rs:19-45`),
/// 2. the item's on-disk `status` flips to "archived",
/// 3. `mp track list --items` no longer surfaces it under the active
///    status names ("pending", "in-progress", "blocked").
#[test]
fn track_archive_marks_item_archived_and_hides_from_active_list() {
    let env = TestEnv::new();

    // Add a fresh track item.
    let add = env.run(&[
        "track",
        "add",
        "tweak",
        "--title",
        "Archive me",
        "--problem",
        "Need an archived item for AC-04",
        "--format",
        "json",
    ]);
    let add_stderr = String::from_utf8_lossy(&add.stderr);
    assert!(
        add.status.success(),
        "track add must succeed: {add_stderr}"
    );
    let item_id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Archive it via the TrackItem subcommand.
    let archive = env.run(&[
        "track",
        "archive",
        "track-item",
        "tweak",
        &item_id,
        "--format",
        "json",
    ]);
    let archive_stderr = String::from_utf8_lossy(&archive.stderr);
    assert!(
        archive.status.success(),
        "track archive track-item must succeed: {archive_stderr}"
    );
    let archive_payload: serde_json::Value =
        serde_json::from_slice(&archive.stdout).expect("archive JSON");
    assert_eq!(
        archive_payload["archived"].as_str(),
        Some("track-item"),
        "archive payload must report the archived kind"
    );
    assert_eq!(
        archive_payload["kind"].as_str(),
        Some("tweak"),
        "archive payload must echo the kind"
    );
    assert_eq!(
        archive_payload["id"].as_str(),
        Some(item_id.as_str()),
        "archive payload must echo the id"
    );

    // The on-disk track JSON must now show status=archived.
    let track_path = env
        .tmp
        .path()
        .join("master-plan/tracks/tweak.json");
    let track_text = std::fs::read_to_string(&track_path).expect("track json");
    let track: serde_json::Value =
        serde_json::from_str(&track_text).expect("track parse");
    let items = track["items"].as_array().expect("items array");
    let archived = items
        .iter()
        .find(|i| i["id"].as_str() == Some(item_id.as_str()))
        .expect("archived item must still exist in track file");
    assert_eq!(
        archived["status"].as_str(),
        Some("archived"),
        "archived item must have status='archived' on disk"
    );
    assert!(
        archived["archived_at"].as_str().is_some(),
        "archived_at must be recorded as an RFC3339 timestamp"
    );

    // `mp track list --items` must not surface the archived item
    // under any of the active status names. The active set is
    // exactly the set the production code considers "in flight";
    // archived items are filtered out before reaching the
    // operator's view.
    let list = env.run(&[
        "track",
        "list",
        "--items",
        "--format",
        "json",
    ]);
    assert!(
        list.status.success(),
        "track list must succeed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_payload: serde_json::Value =
        serde_json::from_slice(&list.stdout).expect("list JSON");
    // `tracks` is an array; find the "tweak" entry and inspect its
    // items list. The active set is exactly the set the production
    // code considers "in flight"; archived items are filtered out
    // before reaching the operator's view.
    let tracks = list_payload["tracks"].as_array().expect("tracks array");
    let tweak_items = tracks
        .iter()
        .find(|t| t["kind"].as_str() == Some("tweak"))
        .and_then(|t| t["items"].as_array())
        .expect("tweak items array");
    let still_active = tweak_items.iter().find(|i| {
        i["id"].as_str() == Some(item_id.as_str())
            && matches!(
                i["status"].as_str(),
                Some("pending") | Some("in-progress") | Some("blocked")
            )
    });
    assert!(
        still_active.is_none(),
        "archived item must NOT appear with an active status in track list"
    );
}

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
