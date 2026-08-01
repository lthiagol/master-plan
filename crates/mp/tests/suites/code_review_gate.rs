use crate::common::TestEnv;

fn create_ms_with_ac(env: &TestEnv, title: &str) -> String {
    let json = format!(
        r#"{{
        "title": "{title}",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "manual: accepted — tested" }}
        ]
    }}"#
    );
    let out = env.run(&["milestone", "create", "--json", &json, "--format", "json"]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn approve_and_add_step(env: &TestEnv, id: &str, step_action: &str) {
    assert!(env
        .run(&["milestone", "approve", id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "wp",
            "add",
            id,
            "--name",
            "WP1",
            "--id",
            "WP1",
            "--goal",
            "Implement",
            "--format",
            "json"
        ])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            id,
            "--wp",
            "WP1",
            "--action",
            step_action,
            "--tests",
            "echo ok",
            "--format",
            "json"
        ])
        .status
        .success());
}

fn set_implemented(env: &TestEnv, id: &str) {
    let out = env.run(&[
        "milestone",
        "set-spec-status",
        id,
        "implemented",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "set implemented failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn set_step_done(env: &TestEnv, id: &str, step_id: &str) {
    assert!(env
        .run(&["milestone", "step", "done", id, step_id, "--format", "json"])
        .status
        .success());
}

// AC-03: With code_review = false (default), milestone completion is unchanged
#[test]
fn code_review_default_off_allows_complete_without_steps() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "default-off-no-steps");
    approve_and_add_step(&env, &id, "implement");
    set_step_done(&env, &id, "S1");
    // code_review defaults to false — should complete fine
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "done",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "complete should work with default config: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// AC-03: code_review = false, even with incomplete steps, should complete
#[test]
fn code_review_default_off_allows_complete_with_pending_steps() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "default-off-pending");
    approve_and_add_step(&env, &id, "implement");
    // Don't mark step done — config defaults to false so complete should still work
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "done",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "complete should work with pending steps when code_review=false: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// AC-01: code_review = true blocks complete when steps are not all done
#[test]
fn code_review_gate_blocks_incomplete() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "gate-blocks");
    approve_and_add_step(&env, &id, "implement");
    set_step_done(&env, &id, "S1");

    // Add a second step and leave it pending
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "review",
            "--tests",
            "manual: accepted — reviewed",
            "--format",
            "json"
        ])
        .status
        .success());

    // Enable code_review
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    // Should block: step S2 is still pending
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "done",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("G15"),
        "complete should be blocked by G15: {stdout}"
    );
}

// AC-01: code_review = true allows complete when all steps are done
#[test]
fn code_review_gate_allows_done() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "gate-allows");
    approve_and_add_step(&env, &id, "implement");
    set_step_done(&env, &id, "S1");

    // Enable code_review
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    // All steps done — should complete
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "done",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "complete should work when all steps done: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// AC-01: code_review = true with no steps — no gate error
#[test]
fn code_review_gate_no_steps_passes() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "gate-no-steps");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());

    // Enable code_review
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    // No steps — gate passes trivially
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "done",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "complete should work when no steps: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// AC-02: list-pending-review shows milestones with spec_status=implemented and code_review=true
#[test]
fn list_pending_review_shows_implemented() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "pending-review");
    approve_and_add_step(&env, &id, "implement");
    set_step_done(&env, &id, "S1");

    // Enable code_review
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    // Set spec_status to implemented
    set_implemented(&env, &id);

    // list-pending-review should show it
    let out = env.run(&["milestone", "list-pending-review", "--format", "json"]);
    assert!(
        out.status.success(),
        "list-pending-review failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<&str> = json["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(
        ids.contains(&id.as_str()),
        "pending review should include {id}: {:?}",
        ids
    );
}

// AC-02: list-pending-review is empty when code_review is false
#[test]
fn list_pending_review_empty_when_disabled() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "pending-review-disabled");
    approve_and_add_step(&env, &id, "implement");
    set_step_done(&env, &id, "S1");
    set_implemented(&env, &id);

    // code_review defaults to false — list should be empty
    let out = env.run(&["milestone", "list-pending-review", "--format", "json"]);
    assert!(
        out.status.success(),
        "list-pending-review failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["milestones"].as_array().unwrap().is_empty(),
        "should be empty when code_review disabled"
    );
}

// AC-02: list-pending-review excludes milestones that are not implemented
#[test]
fn list_pending_review_excludes_non_implemented() {
    let env = TestEnv::new();
    let id = create_ms_with_ac(&env, "not-implemented");
    approve_and_add_step(&env, &id, "work");
    set_step_done(&env, &id, "S1");

    // Enable code_review but DON'T set spec_status to implemented
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    let out = env.run(&["milestone", "list-pending-review", "--format", "json"]);
    assert!(
        out.status.success(),
        "list-pending-review failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<&str> = json["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(
        !ids.contains(&id.as_str()),
        "non-implemented milestone should not appear: {:?}",
        ids
    );
}

// Config roundtrip for workflow.steps.code_review
#[test]
fn config_roundtrip_code_review() {
    let env = TestEnv::new();

    // Default is false
    let out = env.run(&[
        "config",
        "get",
        "workflow.steps.code_review",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "config get failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["value"], false, "default should be false");

    // Set to true
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "true",
            "--format",
            "json"
        ])
        .status
        .success());

    // Verify
    let out = env.run(&[
        "config",
        "get",
        "workflow.steps.code_review",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["value"], true, "should be true after set");

    // Set back to false
    assert!(env
        .run(&[
            "config",
            "set",
            "workflow.steps.code_review",
            "false",
            "--format",
            "json"
        ])
        .status
        .success());

    // Verify
    let out = env.run(&[
        "config",
        "get",
        "workflow.steps.code_review",
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["value"], false, "should be false after reset");
}
