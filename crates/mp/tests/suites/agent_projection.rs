use crate::common::TestEnv;

fn create_milestone(env: &TestEnv) -> String {
    let json = r#"{
        "title": "Test Feature",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "Testing projection" },
        "problem": { "description": "Need projection tests." },
        "scope": {
            "in_scope": ["projection"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            {
                "description": "AC1 description",
                "verification": "cargo test"
            },
            {
                "description": "AC2 description",
                "verification": "cargo test"
            }
        ]
    }"#;

    let out = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn projection_show_single_field() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "milestone.spec_status",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone"]["spec_status"].as_str().unwrap(), "draft");
    assert!(v["milestone"].get("title").is_none());
}

#[test]
fn projection_show_multiple_fields() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "milestone.spec_status,milestone.title",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone"]["spec_status"].as_str().unwrap(), "draft");
    assert!(!v["milestone"]["title"].as_str().unwrap().is_empty());
    assert!(v["milestone"].get("id").is_none());
}

#[test]
fn projection_list_milestones_array() {
    let env = TestEnv::new();
    let _id = create_milestone(&env);

    let out = env.run(&[
        "list",
        "milestones",
        "--fields",
        "milestones[].spec_status,milestones[].title",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().unwrap();
    assert!(!arr.is_empty());
    for item in arr {
        assert!(item.get("spec_status").is_some());
        assert!(item.get("title").is_some());
        assert!(item.get("id").is_none());
    }
}

#[test]
fn projection_status_top_level() {
    let env = TestEnv::new();
    let _id = create_milestone(&env);

    let out = env.run(&[
        "status",
        "--fields",
        "milestones.total,pending_review_count",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["milestones"]["total"].as_u64().unwrap() > 0);
    assert!(v["pending_review_count"].as_u64().is_some());
    assert!(v.get("archived_count").is_none());
}

#[test]
fn projection_validate_ok_only() {
    let env = TestEnv::new();

    let out = env.run(&["validate", "--fields", "ok"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v.get("errors").is_none());
    assert!(v.get("warnings").is_none());
}

#[test]
fn projection_reviews_pending_count() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "pending", "--fields", "count"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["count"].as_u64().is_some());
    assert!(v.get("pending").is_none());
}

#[test]
fn projection_array_index_access() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "acceptance_criteria[0].description",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let desc = v["acceptance_criteria"]["0"]["description"]
        .as_str()
        .unwrap();
    assert_eq!(desc, "AC1 description");
}

#[test]
fn projection_unknown_path_is_hard_error() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "milestone.nonexistent",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown path"), "stderr: {stderr}");
}

#[test]
fn projection_no_fields_emits_full_output() {
    let env = TestEnv::new();

    let out = env.run(&["validate"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v.get("errors").is_some());
    assert!(v.get("warnings").is_some());
}

#[test]
fn projection_steps_all_elements() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    // Approve and add steps so we have steps to project
    env.run(&["milestone", "approve", &id, "--format", "json"]);
    env.run(&["milestone", "wp", "add", &id, "--name", "WP1"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "step 1",
        "--tests",
        "manual: ok",
    ]);
    env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "step 2",
        "--tests",
        "manual: ok",
    ]);

    let out = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "steps[].id,steps[].status",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let steps = v["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    for step in steps {
        assert!(step.get("id").is_some());
        assert!(step.get("status").is_some());
        assert!(step.get("action").is_none());
    }
}

#[test]
fn projection_list_steps_with_fields() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    env.run(&["milestone", "approve", &id, "--format", "json"]);
    env.run(&["milestone", "wp", "add", &id, "--name", "WP1"]);
    env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "task",
        "--tests",
        "manual: ok",
    ]);

    let out = env.run(&[
        "list",
        "steps",
        "--fields",
        "steps[].milestone,steps[].step.status",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let steps = v["steps"].as_array().unwrap();
    assert!(!steps.is_empty());
    for step in steps {
        assert!(step.get("milestone").is_some());
        assert!(step.get("step").is_some());
        assert!(step["step"].get("status").is_some());
    }
}

#[test]
fn projection_inner_of_array_element() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );

    let wp = env.run(&["milestone", "wp", "add", &id, "--name", "WP1"]);
    assert!(
        wp.status.success(),
        "wp add failed: {}",
        String::from_utf8_lossy(&wp.stderr)
    );

    let step = env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "inner test",
        "--tests",
        "manual: ok",
    ]);
    assert!(
        step.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&step.stderr)
    );

    let out = env.run(&["show", "milestone", &id, "--fields", "steps[].id"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let steps = v["steps"].as_array().unwrap();
    assert!(!steps.is_empty(), "steps: {v:?}");
    for step in steps {
        assert!(step.get("id").is_some(), "step: {step:?}");
        assert!(step.get("status").is_none());
    }
}
