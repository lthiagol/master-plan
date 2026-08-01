use serde_json::Value;

use crate::common::review_queue_fixture::write_done_milestone;
use crate::common::TestEnv;

#[test]
fn reviews_pending_lists_done_without_signoff() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "review-pending", "Review Pending");

    let out = env.run(&["reviews", "pending", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["count"].as_u64().unwrap() >= 1);
    let ids: Vec<String> = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["milestone_id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"90".to_string()));
}

#[test]
fn reviews_pass_removes_from_pending_and_recomplete_requeues() {
    let env = TestEnv::new();
    write_done_milestone(&env, "89", "review-pass", "Review Pass");

    let pass = env.run(&[
        "reviews",
        "pass",
        "89",
        "--verdict",
        "ok",
        "--reviewer",
        "reviewer-a",
        "--format",
        "json",
    ]);
    assert!(
        pass.status.success(),
        "{}",
        String::from_utf8_lossy(&pass.stderr)
    );

    let pending = env.run(&["reviews", "pending", "--format", "json"]);
    let v: Value = serde_json::from_slice(&pending.stdout).unwrap();
    let ids: Vec<&str> = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["milestone_id"].as_str())
        .collect();
    assert!(!ids.contains(&"89"));

    let list = env.run(&["reviews", "list", "--format", "json"]);
    let rows: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(rows["reviews"].as_array().unwrap().len(), 1);

    // Re-complete with newer verification date → pending again
    let path = env
        .tmp
        .path()
        .join("master-plan/milestones/89-review-pass.json");
    let mut content = std::fs::read_to_string(&path).unwrap();
    content = content.replace("\"date\": \"2026-06-30\"", "\"date\": \"2026-07-01\"");
    std::fs::write(&path, content).unwrap();
    env.run(&["sync", "--format", "json"]);

    let pending2 = env.run(&["reviews", "pending", "--format", "json"]);
    let v2: Value = serde_json::from_slice(&pending2.stdout).unwrap();
    let ids2: Vec<&str> = v2["pending"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["milestone_id"].as_str())
        .collect();
    assert!(ids2.contains(&"89"));
}

#[test]
fn status_includes_pending_review_count() {
    let env = TestEnv::new();
    write_done_milestone(&env, "88", "status-review", "Status Review");

    let out = env.run(&["status", "--format", "json"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["pending_review_count"].as_u64().unwrap() >= 1);
}
