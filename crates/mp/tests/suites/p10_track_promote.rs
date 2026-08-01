use crate::common::TestEnv;

#[test]
fn track_promote_to_milestone_archives_item() {
    let env = TestEnv::new();

    let add = env.run(&[
        "track",
        "add",
        "bugfix",
        "--title",
        "Login redirect loop",
        "--problem",
        "Users bounce after OAuth",
        "--done-when",
        "Login completes without redirect",
        "--verification",
        "cargo test oauth",
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

    let promote = env.run(&[
        "track",
        "promote",
        "bugfix",
        &item_id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(
        promote.status.success(),
        "{}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&promote.stdout).unwrap();
    assert!(json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("milestone:"));

    let track = env.run(&["track", "show", "bugfix", "--format", "json"]);
    let track_json: serde_json::Value = serde_json::from_slice(&track.stdout).unwrap();
    let item = track_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == item_id)
        .unwrap();
    assert_eq!(item["status"], "archived");
    assert!(!item["archived_at"].as_str().unwrap().is_empty());

    let milestones = env.run(&["list", "milestones", "--format", "json"]);
    assert!(
        !serde_json::from_slice::<serde_json::Value>(&milestones.stdout).unwrap()["milestones"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn track_promote_archived_item_is_idempotent() {
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
    let item_id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = env.run(&[
        "track",
        "promote",
        "tweak",
        &item_id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(first.status.success());
    let first_json = serde_json::from_slice::<serde_json::Value>(&first.stdout).unwrap();
    let promoted_to = first_json["promoted_to"].as_str().unwrap().to_string();

    let again = env.run(&[
        "track",
        "promote",
        "tweak",
        &item_id,
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(
        again.status.success(),
        "M187: re-promote after successful promotion is idempotent"
    );
    let again_json = serde_json::from_slice::<serde_json::Value>(&again.stdout).unwrap();
    assert_eq!(again_json["ok"], true);
    assert_eq!(again_json["idempotent"], true);
    assert_eq!(again_json["promoted_to"].as_str().unwrap(), promoted_to);
}
