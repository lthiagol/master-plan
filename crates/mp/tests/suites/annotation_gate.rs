use std::fs;

use crate::common::TestEnv;

/// Write a milestone file (with full ceremony) as JSON into the temp plan dir.
fn write_milestone(env: &TestEnv, id: &str, title: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "slug": "test",
            "title": title,
            "spec_status": "review",
            "execution_status": "planned",
            "priority": "normal",
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "depends_on": [],
            "created": "2026-06-28",
            "target_version": "",
            "updated": "",
            "block_reason": "",
            "blocked_at": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "Test outcome" },
        "problem": { "description": "Test problem" },
        "scope": { "in_scope": ["Item 1"], "out_of_scope": ["Not this", "Nor this"] },
        "acceptance_criteria": [
            {
                "id": "AC-01",
                "description": "AC one",
                "verification": "manual",
                "status": "",
                "evidence": "",
            },
            {
                "id": "AC-02",
                "description": "AC two",
                "verification": "manual",
                "status": "",
                "evidence": "",
            },
        ],
        "open_questions": [{
            "id": "Q-01",
            "question": "A question",
            "status": "resolved",
            "answer": "Done",
        }],
    });
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-test.json")), format!("{json}\n")).unwrap();
}

#[test]
fn g14_block_validate() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    // Create a milestone file
    write_milestone(&env, "01", "Test M01");

    // Sync plan
    env.run(&["sync", "--format", "json"]);

    // Without approval annotation, validate should pass (gates may fire but will be fine for review)
    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate should pass without G14 block: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Add an approval-request annotation targeting M01
    let out = env.run(&[
        "annotation",
        "create",
        "M01",
        "approval-request",
        "Block M01 until approved",
        "alice",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now validate should fail with G14
    let out = env.run(&["validate", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("G14"),
        "validate should contain G14: {stdout}"
    );

    // Resolve the annotation — find annotation ID via list
    let _json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let list_out = env.run(&["annotation", "list", "--format", "json"]);
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let ann_id = list_json["annotations"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    env.run(&["annotation", "resolve", &ann_id, "--format", "json"]);

    // Now validate should pass again
    let out = env.run(&["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate should pass after resolving approval: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn g14_block_set_spec_status_ready() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    write_milestone(&env, "02", "Test M02");
    env.run(&["sync", "--format", "json"]);

    // Add approval-request annotation
    env.run(&[
        "annotation",
        "create",
        "M02",
        "approval-request",
        "Block M02",
        "alice",
        "--format",
        "json",
    ]);

    // Set spec status to ready should be blocked
    let out = env.run(&[
        "milestone",
        "set-spec-status",
        "02",
        "ready",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("G14"),
        "set-spec-status ready should be G14 blocked: {stdout}"
    );

    // Get annotation id and resolve
    let list_out = env.run(&["annotation", "list", "--format", "json"]);
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let ann_id = list_json["annotations"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["annotation", "resolve", &ann_id, "--format", "json"]);

    // Now set spec status to ready should work
    let out = env.run(&[
        "milestone",
        "set-spec-status",
        "02",
        "ready",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "set-spec-status ready should work after resolving: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn g14_only_approval_request_blocks() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    write_milestone(&env, "03", "Test M03");
    env.run(&["sync", "--format", "json"]);

    // Add a review-request annotation (NOT approval-request)
    env.run(&[
        "annotation",
        "create",
        "M03",
        "review-request",
        "Please review M03",
        "alice",
        "--format",
        "json",
    ]);

    // Other kinds should NOT block ready
    let out = env.run(&[
        "milestone",
        "set-spec-status",
        "03",
        "ready",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "review-request should not trigger G14: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Validate should also pass
    assert!(
        env.run_validate(),
        "validate should pass with non-approval annotation"
    );
}

#[test]
fn g14_block_step_target() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    write_milestone(&env, "04", "Test M04");
    env.run(&["sync", "--format", "json"]);

    // Add approval-request targeting M04/S1 (step target)
    env.run(&[
        "annotation",
        "create",
        "M04/S1",
        "approval-request",
        "Block M04 step S1",
        "alice",
        "--format",
        "json",
    ]);

    // Should still block since target starts with M04/
    let out = env.run(&[
        "milestone",
        "set-spec-status",
        "04",
        "ready",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("G14"),
        "step-level approval should also trigger G14: {stdout}"
    );
}
