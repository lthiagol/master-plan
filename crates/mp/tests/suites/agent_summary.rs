use crate::common::TestEnv;

fn create_test_milestones(env: &TestEnv) {
    for i in 0..2 {
        let title = format!("Summary Test {i}");
        let json = serde_json::json!({
            "title": title,
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "intent": {"outcome": "Testing summary"},
            "problem": {"description": "Need summary tests."},
            "scope": {"in_scope": ["summary"], "out_of_scope": ["x", "y"]},
            "acceptance_criteria": [
                {"description": "AC1", "verification": "cargo test"}
            ]
        });
        let out = env.run(&[
            "milestone",
            "create",
            "--json",
            &json.to_string(),
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "create {i} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn validate_summary_has_counts_and_buckets() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&["validate", "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v["error_count"].as_u64().is_some());
    assert!(v["warning_count"].as_u64().is_some());
    assert!(v.get("warnings_by_code").is_some());
    assert!(v.get("errors_by_code").is_some());
}

#[test]
fn validate_summary_with_fields() {
    let env = TestEnv::new();

    let out = env.run(&[
        "validate",
        "--summary",
        "--fields",
        "ok,error_count,warning_count",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v.get("error_count").is_some());
    assert!(v.get("warning_count").is_some());
    assert!(v.get("warnings_by_code").is_none());
}

#[test]
fn reviews_pending_group_by_completed_at() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "pending", "--group-by", "completed_at"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("groups").is_some());
    assert!(v.get("total").is_some());
}

#[test]
fn reviews_pending_group_by_milestone_id_empty_lists() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "pending", "--group-by", "milestone_id"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("groups").is_some());
    assert_eq!(v["total"].as_u64().unwrap(), 0);
}

#[test]
fn reviews_pending_no_group_by_has_full_list() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "pending", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("pending").is_some());
    assert!(v.get("count").is_some());
}

#[test]
fn show_milestone_summary_rollup() {
    let env = TestEnv::new();
    let json = serde_json::json!({
        "title": "Summary Rollup Test",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {"outcome": "Test health summary"},
        "problem": {"description": "Testing summary rollup."},
        "scope": {"in_scope": ["summary"], "out_of_scope": ["x", "y"]},
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ]
    });
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &json.to_string(),
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap();

    env.run(&["milestone", "approve", id, "--format", "json"]);
    env.run(&["milestone", "wp", "add", id, "--name", "WP1"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        id,
        "--wp",
        "WP1",
        "--action",
        "task",
        "--tests",
        "manual: ok",
    ]);
    env.run(&["milestone", "set-status", id, "in-progress"]);
    env.run(&["milestone", "step", "set-status", id, "S1", "done"]);
    env.run(&["milestone", "complete", id, "--evidence", "manual ok"]);

    let out = env.run(&["show", "milestone", id, "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(s["steps"]["total"].as_u64().unwrap(), 1);
    assert_eq!(s["steps"]["done"].as_u64().unwrap(), 1);
    assert_eq!(s["acceptance_criteria"]["passed"].as_u64().unwrap(), 1);
    assert_eq!(s["review_state"].as_str().unwrap(), "pending-review");
    assert!(!s["verification"]["force_bypassed"].as_bool().unwrap());
}

#[test]
fn finding_resolve_all_open() {
    let env = TestEnv::new();

    let json = serde_json::json!({
        "title": "Resolve All Test",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {"outcome": "Test resolve all"},
        "problem": {"description": "Testing resolve all."},
        "scope": {"in_scope": ["resolve"], "out_of_scope": ["x", "y"]},
        "acceptance_criteria": [{"description": "AC1", "verification": "manual: ok"}]
    });
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &json.to_string(),
        "--format",
        "json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mid = v["milestone"]["id"].as_str().unwrap();
    env.run(&["milestone", "approve", mid, "--format", "json"]);
    env.run(&["milestone", "wp", "add", mid, "--name", "WP1"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        mid,
        "--wp",
        "WP1",
        "--action",
        "t",
        "--tests",
        "manual: ok",
    ]);
    env.run(&["milestone", "set-status", mid, "in-progress"]);
    env.run(&["milestone", "step", "set-status", mid, "S1", "done"]);
    env.run(&["milestone", "complete", mid]);

    env.run(&[
        "reviews",
        "finding",
        "add",
        mid,
        "--severity",
        "low",
        "--category",
        "bug",
        "--desc",
        "a",
    ]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        mid,
        "--severity",
        "low",
        "--category",
        "bug",
        "--desc",
        "b",
    ]);

    let out = env.run(&["reviews", "finding", "resolve", mid, "--all"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["resolved_count"].as_u64().unwrap(), 2);

    let list = env.run(&["reviews", "finding", "list", mid, "--open"]);
    let l: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(l["count"].as_u64().unwrap(), 0);
    assert_eq!(l["summary"]["open"].as_u64().unwrap(), 0);
    assert_eq!(l["summary"]["fixed"].as_u64().unwrap(), 2);
}
