use crate::common::review_queue_fixture::create_and_complete_milestone as create_and_complete;
use crate::common::TestEnv;

// S1: Executor attribution

#[test]
fn executor_persisted_on_complete() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("opencode-2026-07-01"));

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        m["milestone"]["executed_by"].as_str().unwrap(),
        "opencode-2026-07-01"
    );
}

#[test]
fn executor_empty_when_not_provided() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, None);

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(m["milestone"]["executed_by"].as_str().unwrap(), "");
}

#[test]
fn executor_persisted_with_force() {
    let env = TestEnv::new();
    // Create a milestone with a runnable AC that will fail → needs --force
    let json = serde_json::json!({
        "title": "Force Complete Test",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {"outcome": "Test force + executor"},
        "problem": {"description": "Testing force complete."},
        "scope": {"in_scope": ["force"], "out_of_scope": ["x", "y"]},
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: approve before installing failing verifier"}
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
    let id = v["milestone"]["id"].as_str().unwrap().to_string();

    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());
    let start = env.run(&["milestone", "set-status", &id, "in-progress"]);
    assert!(start.status.success());
    let update = env.run(&[
        "milestone",
        "ac",
        "update",
        &id,
        "AC-01",
        "--verification",
        "sh -c 'exit 1'",
    ]);
    assert!(update.status.success());

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--force",
        "--executor",
        "force-bot",
    ]);
    assert!(
        out.status.success(),
        "force complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(m["milestone"]["executed_by"].as_str().unwrap(), "force-bot");
    assert_eq!(m["milestone"]["execution_status"].as_str().unwrap(), "done");
    assert!(m["verification"]["evidence"]
        .as_str()
        .unwrap()
        .contains("force-bypassed"));
}

#[test]
fn remediation_restores_exact_complete_pre_state() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));
    let add = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "medium",
        "--category",
        "correctness",
        "--desc",
        "exercise exact restore",
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let in_remediation = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&in_remediation.stdout).unwrap();
    assert_eq!(m["milestone"]["lifecycle"], "remediation");
    assert_eq!(m["milestone"]["remediation_pre_state"], "complete");

    let resolve = env.run(&["reviews", "finding", "resolve", &id, "F-01"]);
    assert!(
        resolve.status.success(),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let restored = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(m["milestone"]["lifecycle"], "complete");
    assert!(m["milestone"].get("remediation_pre_state").is_none());
}

// S2-S4: Findings

#[test]
fn finding_add_creates_open_finding() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "high",
        "--category",
        "bug",
        "--desc",
        "A critical issue",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    let f = &v["finding"];
    assert_eq!(f["id"].as_str().unwrap(), "F-01");
    assert_eq!(f["status"].as_str().unwrap(), "open");
    assert_eq!(f["severity"].as_str().unwrap(), "high");
}

#[test]
fn finding_add_with_author() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "medium",
        "--category",
        "review",
        "--desc",
        "needs attention",
        "--author",
        "alice",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let f = &v["finding"];
    assert_eq!(f["author"].as_str().unwrap(), "alice");
    assert_eq!(f["severity"].as_str().unwrap(), "medium");
}

#[test]
fn finding_add_invalid_severity_rejected() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "critical",
        "--category",
        "bug",
        "--desc",
        "bad",
    ]);
    assert!(!out.status.success());
}

#[test]
fn milestone_update_rejects_findings_field() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    let json = r#"{"findings": [{"id": "F-01", "status": "fixed"}]}"#;
    let out = env.run(&[
        "milestone",
        "update",
        &id,
        "--json",
        json,
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unsupported field") && err.contains("findings"),
        "expected findings rejection, got: {err}"
    );
    assert!(
        err.contains("mp reviews finding"),
        "expected redirect hint, got: {err}"
    );
}

#[test]
fn finding_resolve_marks_fixed() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "medium",
        "--category",
        "bug",
        "--desc",
        "x",
    ]);
    let out = env.run(&[
        "reviews", "finding", "resolve", &id, "F-01", "--commit", "abc123",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["finding"]["status"].as_str().unwrap(), "fixed");
    assert_eq!(v["finding"]["fixed_in"].as_str().unwrap(), "abc123");
}

#[test]
fn finding_list_filters_open() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "high",
        "--category",
        "bug",
        "--desc",
        "f1",
    ]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "low",
        "--category",
        "style",
        "--desc",
        "f2",
    ]);
    env.run(&[
        "reviews", "finding", "resolve", &id, "F-01", "--commit", "x",
    ]);

    let out = env.run(&["reviews", "finding", "list", &id, "--open"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"].as_u64().unwrap(), 1);
    assert_eq!(
        v["findings"].as_array().unwrap()[0]["id"].as_str().unwrap(),
        "F-02"
    );
}

#[test]
fn finding_list_all() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "high",
        "--category",
        "bug",
        "--desc",
        "f1",
    ]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "low",
        "--category",
        "style",
        "--desc",
        "f2",
    ]);

    let out = env.run(&["reviews", "finding", "list", &id]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"].as_u64().unwrap(), 2);
}

#[test]
fn findings_surfaced_in_show() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "high",
        "--category",
        "bug",
        "--desc",
        "shown",
    ]);

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let findings = m["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["status"].as_str().unwrap(), "open");
}

// S5: Review state

#[test]
fn review_state_pending_before_review() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    // No review yet → pending-review
    let lifecycle = env.run(&["reviews", "lifecycle", "--format", "json"]);
    let l: serde_json::Value = serde_json::from_slice(&lifecycle.stdout).unwrap();
    let items = l["lifecycle"].as_array().unwrap();
    let pending = items
        .iter()
        .find(|i| i["review_state"].as_str() == Some("pending-review"))
        .unwrap();
    let ids: Vec<&str> = pending["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id.as_str()));
}

#[test]
fn review_state_after_pass_with_findings() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "alice",
    ]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "medium",
        "--category",
        "bug",
        "--desc",
        "x",
    ]);

    let lifecycle = env.run(&["reviews", "lifecycle", "--format", "json"]);
    let l: serde_json::Value = serde_json::from_slice(&lifecycle.stdout).unwrap();
    let items = l["lifecycle"].as_array().unwrap();
    let open = items
        .iter()
        .find(|i| i["review_state"].as_str() == Some("open-findings"))
        .unwrap();
    let ids: Vec<&str> = open["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id.as_str()));
}

#[test]
fn review_state_remediated_after_fix() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "alice",
    ]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "low",
        "--category",
        "bug",
        "--desc",
        "x",
    ]);
    env.run(&[
        "reviews", "finding", "resolve", &id, "F-01", "--commit", "fix123",
    ]);

    let lifecycle = env.run(&["reviews", "lifecycle", "--format", "json"]);
    let l: serde_json::Value = serde_json::from_slice(&lifecycle.stdout).unwrap();
    let items = l["lifecycle"].as_array().unwrap();
    let remediated = items
        .iter()
        .find(|i| i["review_state"].as_str() == Some("remediated"))
        .unwrap();
    let ids: Vec<&str> = remediated["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id.as_str()));
}

#[test]
fn review_state_clean() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "alice",
    ]);

    let lifecycle = env.run(&["reviews", "lifecycle", "--format", "json"]);
    let l: serde_json::Value = serde_json::from_slice(&lifecycle.stdout).unwrap();
    let items = l["lifecycle"].as_array().unwrap();
    let clean = items
        .iter()
        .find(|i| i["review_state"].as_str() == Some("reviewed-clean"))
        .unwrap();
    let ids: Vec<&str> = clean["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id.as_str()));
}

// S7: Validate hygiene

#[test]
fn validate_warns_on_done_but_unreviewed() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    let out = env.run(&["validate", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = v["warnings"].as_array().unwrap();
    let w44 = warnings.iter().find(|w| w["code"].as_str() == Some("W44"));
    assert!(
        w44.is_some(),
        "should warn W44 for done-but-unreviewed milestone"
    );
    assert_eq!(w44.unwrap()["milestone"].as_str().unwrap(), id);
}

#[test]
fn validate_no_w44_after_review() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));

    env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "alice",
    ]);

    let out = env.run(&["validate", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = v["warnings"].as_array().unwrap();
    let w44 = warnings.iter().find(|w| w["code"].as_str() == Some("W44"));
    assert!(w44.is_none(), "no W44 after review is recorded");
}

// S6: Lifecycle rollup

#[test]
fn lifecycle_has_total_done_count() {
    let env = TestEnv::new();
    let _id = create_and_complete(&env, Some("test"));

    let out = env.run(&["reviews", "lifecycle", "--format", "json"]);
    let l: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(l["total_done"].as_u64().unwrap() >= 1);
}

#[test]
fn lifecycle_with_fields_projection() {
    let env = TestEnv::new();
    let _id = create_and_complete(&env, Some("test"));

    let out = env.run(&[
        "reviews",
        "lifecycle",
        "--fields",
        "lifecycle[].review_state,lifecycle[].count",
    ]);
    let l: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = l["lifecycle"].as_array().unwrap();
    for item in items {
        assert!(item.get("review_state").is_some());
        assert!(item.get("count").is_some());
        assert!(item.get("milestones").is_none());
    }
}

#[test]
fn external_finding_on_complete_enters_remediation() {
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));
    let add = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "high",
        "--category",
        "correctness",
        "--phase",
        "external",
        "--desc",
        "M189 F-04 external on complete must enter remediation",
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        m["milestone"]["lifecycle"], "remediation",
        "external finding on complete must EnterRemediation; got {}",
        m["milestone"]["lifecycle"]
    );
    assert_eq!(m["milestone"]["remediation_pre_state"], "complete");
}

#[test]
fn external_finding_on_executed_enters_remediation() {
    // M196: the executor's end-state was renamed from `done` to
    // `executed`. The review-flow behavior is unchanged: an external
    // finding filed on a milestone in the executor's end-state enters
    // `remediation` and captures the pre-state so the exit restores
    // exactly.
    let env = TestEnv::new();
    let id = create_and_complete(&env, Some("test"));
    // Hand-write lifecycle=executed (post-rename canonical name).
    let milestone_dir = env.tmp.path().join("master-plan/milestones");
    let file = std::fs::read_dir(&milestone_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let body = std::fs::read_to_string(p).unwrap_or_default();
            body.contains(&format!("\"id\": \"{id}\""))
                || body.contains(&format!("\"id\":\"{id}\""))
        })
        .expect("milestone file");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    doc["milestone"]["lifecycle"] = serde_json::json!("executed");
    doc["milestone"]["spec_status"] = serde_json::json!("implemented");
    doc["milestone"]["execution_status"] = serde_json::json!("done");
    std::fs::write(&file, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let add = env.run(&[
        "reviews",
        "finding",
        "add",
        &id,
        "--severity",
        "medium",
        "--category",
        "correctness",
        "--phase",
        "external",
        "--desc",
        "M189 F-04 external on executed must enter remediation",
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let m: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(m["milestone"]["lifecycle"], "remediation");
    assert_eq!(m["milestone"]["remediation_pre_state"], "executed");
}
