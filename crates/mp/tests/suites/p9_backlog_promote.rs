use crate::common::TestEnv;

#[test]
fn backlog_promote_to_milestone_and_track() {
    let env = TestEnv::new();

    let add_ms = env.run(&[
        "backlog",
        "add",
        "--desc",
        "Google OAuth provider",
        "--priority",
        "high",
        "--format",
        "json",
    ]);
    assert!(
        add_ms.status.success(),
        "{}",
        String::from_utf8_lossy(&add_ms.stderr)
    );
    let ms_id = crate::common::json_from_stdout(&add_ms.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let promote_ms = env.run(&[
        "backlog",
        "promote",
        &ms_id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(
        promote_ms.status.success(),
        "{}",
        String::from_utf8_lossy(&promote_ms.stderr)
    );
    let ms_json: serde_json::Value = serde_json::from_slice(&promote_ms.stdout).unwrap();
    assert!(ms_json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("milestone:"));

    let show_ms = env.run(&["backlog", "show", &ms_id, "--format", "json"]);
    let show_ms_json: serde_json::Value = serde_json::from_slice(&show_ms.stdout).unwrap();
    assert_eq!(show_ms_json["item"]["status"], "resolved");
    assert!(show_ms_json["item"]["resolution"]
        .as_str()
        .unwrap()
        .starts_with("milestone:"));

    let milestones = env.run(&["list", "milestones", "--format", "json"]);
    assert!(milestones.status.success());
    let milestone_list: serde_json::Value = serde_json::from_slice(&milestones.stdout).unwrap();
    assert!(!milestone_list["milestones"].as_array().unwrap().is_empty());

    let add_tr = env.run(&[
        "backlog",
        "add",
        "--desc",
        "Fix login redirect loop",
        "--format",
        "json",
    ]);
    let tr_id = crate::common::json_from_stdout(&add_tr.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let promote_tr = env.run(&[
        "backlog",
        "promote",
        &tr_id,
        "--to-track",
        "bugfix",
        "--format",
        "json",
    ]);
    assert!(
        promote_tr.status.success(),
        "{}",
        String::from_utf8_lossy(&promote_tr.stderr)
    );
    let tr_json: serde_json::Value = serde_json::from_slice(&promote_tr.stdout).unwrap();
    assert!(tr_json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("track:bugfix:"));

    let track = env.run(&["track", "show", "bugfix", "--format", "json"]);
    assert!(track.status.success());
    let track_json: serde_json::Value = serde_json::from_slice(&track.stdout).unwrap();
    let bugfix_items = track_json["items"].as_array().unwrap();
    assert!(!bugfix_items.is_empty());
}

#[test]
fn backlog_promote_resolved_item_is_idempotent() {
    let env = TestEnv::new();

    let add = env.run(&[
        "backlog",
        "add",
        "--desc",
        "Already handled",
        "--format",
        "json",
    ]);
    let id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = env.run(&[
        "backlog",
        "promote",
        &id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(first.status.success());
    let first_json = crate::common::json_from_stdout(&first.stdout);
    let promoted_to = first_json["promoted_to"].as_str().unwrap().to_string();

    let again = env.run(&[
        "backlog",
        "promote",
        &id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(
        again.status.success(),
        "M187: re-promote after successful promotion is idempotent"
    );
    let again_json = crate::common::json_from_stdout(&again.stdout);
    assert_eq!(again_json["ok"], true);
    assert_eq!(again_json["idempotent"], true);
    assert_eq!(again_json["promoted_to"].as_str().unwrap(), promoted_to);
}
