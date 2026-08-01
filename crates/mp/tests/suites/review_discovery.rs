use serde_json::Value;

use crate::common::review_queue_fixture::{write_done_milestone, write_spec_review_milestone};
use crate::common::TestEnv;

#[test]
fn reviews_status_merges_execution_and_spec_queues() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");
    write_spec_review_milestone(&env, "91", "spec-one", "Spec One");

    let out = env.run(&["reviews", "status"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(v["pending_review_count"].as_u64().unwrap(), 1);
    assert_eq!(v["spec_review_count"].as_u64().unwrap(), 1);
    assert_eq!(v["execution_review"]["count"].as_u64().unwrap(), 1);
    assert_eq!(v["spec_review"]["count"].as_u64().unwrap(), 1);

    let exec_pending = v["execution_review"]["pending"].as_array().unwrap();
    assert_eq!(exec_pending[0]["milestone_id"].as_str().unwrap(), "90");

    let spec_items = v["spec_review"]["milestones"].as_array().unwrap();
    assert_eq!(spec_items[0]["milestone_id"].as_str().unwrap(), "91");

    let next = &v["suggested_next"];
    assert_eq!(next["type"].as_str().unwrap(), "spec-review");
    assert_eq!(next["milestone_id"].as_str().unwrap(), "91");
}

#[test]
fn reviews_status_suggests_execution_review_when_no_spec_queue() {
    let env = TestEnv::new();
    write_done_milestone(&env, "88", "done-only", "Done Only");

    let out = env.run(&["reviews", "status"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(v["spec_review_count"].as_u64().unwrap(), 0);
    assert_eq!(v["pending_review_count"].as_u64().unwrap(), 1);
    let next = &v["suggested_next"];
    assert_eq!(next["type"].as_str().unwrap(), "execution-review");
    assert_eq!(next["milestone_id"].as_str().unwrap(), "88");
}

#[test]
fn inbox_filter_review_returns_only_review_items() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");
    write_spec_review_milestone(&env, "91", "spec-one", "Spec One");

    let out = env.run(&["inbox", "--filter", "review"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();

    assert_eq!(items.len(), 2);
    let kinds: Vec<&str> = items.iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"execution-review"));
    assert!(kinds.contains(&"spec-review"));
    for item in items {
        assert!(
            item["kind"].as_str().unwrap() == "execution-review"
                || item["kind"].as_str().unwrap() == "spec-review"
        );
    }
}

#[test]
fn inbox_filter_spec_review_only() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");
    write_spec_review_milestone(&env, "91", "spec-one", "Spec One");

    let out = env.run(&["inbox", "--filter", "spec-review"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"].as_str().unwrap(), "spec-review");
    assert_eq!(items[0]["id"].as_str().unwrap(), "91");
}

#[test]
fn inbox_filter_execution_review_only() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");
    write_spec_review_milestone(&env, "91", "spec-one", "Spec One");

    let out = env.run(&["inbox", "--filter", "execution-review"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"].as_str().unwrap(), "execution-review");
    assert_eq!(items[0]["id"].as_str().unwrap(), "90");
}

#[test]
fn reviews_pending_summary_includes_steps_and_findings() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");

    let out = env.run(&["reviews", "pending", "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let pending = v["pending"].as_array().unwrap();
    assert_eq!(pending.len(), 1);
    let summary = &pending[0]["summary"];
    assert!(summary.get("steps_done").is_some());
    assert!(summary.get("steps_total").is_some());
    assert!(summary.get("findings_open").is_some());
}

#[test]
fn reviews_status_fields_projection() {
    let env = TestEnv::new();
    write_done_milestone(&env, "90", "done-one", "Done One");

    let out = env.run(&[
        "reviews",
        "status",
        "--fields",
        "pending_review_count,spec_review_count,suggested_next.type",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["pending_review_count"].as_u64().unwrap(), 1);
    assert_eq!(v["spec_review_count"].as_u64().unwrap(), 0);
    assert_eq!(
        v["suggested_next"]["type"].as_str().unwrap(),
        "execution-review"
    );
    assert!(v.get("execution_review").is_none());
}

#[test]
fn inbox_unknown_filter_errors() {
    let env = TestEnv::new();
    let out = env.run(&["inbox", "--filter", "not-a-filter"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown inbox filter"));
}

#[test]
fn reviews_pending_group_by_and_summary_mutually_exclusive() {
    let env = TestEnv::new();
    let out = env.run(&[
        "reviews",
        "pending",
        "--group-by",
        "milestone_id",
        "--summary",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mutually exclusive"));
}
