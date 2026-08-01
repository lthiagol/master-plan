use crate::common::TestEnv;

fn create_test_milestones(env: &TestEnv) {
    for i in 0..3 {
        let title = format!("Test Feature {i}");
        let json = serde_json::json!({
            "title": title,
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "intent": {"outcome": "Testing filters"},
            "problem": {"description": "Need filter tests."},
            "scope": {"in_scope": ["filters"], "out_of_scope": ["x", "y"]},
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
fn filter_where_equality() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&["list", "milestones", "--where", "spec_status==draft"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    assert!(items.len() >= 3);
    for item in items {
        assert_eq!(item["spec_status"].as_str().unwrap(), "draft");
    }
}

#[test]
fn filter_where_not_equality() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&["list", "milestones", "--where", "execution_status!=done"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    for item in items {
        assert_ne!(item["execution_status"].as_str().unwrap(), "done");
    }
}

#[test]
fn filter_where_combined_with_fields() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&[
        "list",
        "milestones",
        "--where",
        "spec_status==draft",
        "--fields",
        "milestones[].title",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    for item in items {
        assert!(item.get("title").is_some());
        assert!(item.get("id").is_none());
    }
}

#[test]
fn filter_preset_force_bypassed() {
    let env = TestEnv::new();
    // Create and complete a milestone with --force to get a force-bypassed entry
    create_test_milestones(&env);

    // Check that none are force-bypassed initially
    let out = env.run(&["list", "milestones", "--preset", "force-bypassed"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    assert_eq!(
        items.len(),
        0,
        "no milestones should be force-bypassed initially"
    );
}

#[test]
fn filter_include_steps() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    // Get the first milestone, approve it, add a step
    let list_out = env.run(&["list", "milestones", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let id = v["milestones"][0]["id"].as_str().unwrap().to_string();

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
        "milestones",
        "--include",
        "steps",
        "--where",
        &format!("id=={id}"),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let steps = items[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["id"].as_str().unwrap(), "S1");
}

#[test]
fn filter_include_acceptance_criteria() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&[
        "list",
        "milestones",
        "--include",
        "acceptance_criteria",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    for item in items {
        let acs = item["acceptance_criteria"].as_array().unwrap();
        assert!(!acs.is_empty());
    }
}

#[test]
fn filter_multiple_include_flags() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&[
        "list",
        "milestones",
        "--include",
        "steps,acceptance_criteria,evidence",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    assert!(items.len() >= 3);
    for item in items {
        assert!(item.get("steps").is_some());
        assert!(item.get("acceptance_criteria").is_some());
        assert!(item.get("evidence").is_some());
    }
}

#[test]
fn filter_no_include_does_not_embed_steps() {
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&["list", "milestones", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().unwrap();
    for item in items {
        assert!(
            item.get("steps").is_none(),
            "steps should not appear without --include steps"
        );
    }
}

#[test]
fn filter_unknown_preset_errors_not_silently_matches_all() {
    // Regression: an unknown --filter value must error, not silently return
    // every milestone (which would let a typo/agent filter the wrong set unnoticed).
    let env = TestEnv::new();
    create_test_milestones(&env);

    let out = env.run(&["list", "milestones", "--filter", "force-bypassed"]);
    assert!(
        !out.status.success(),
        "unknown --filter preset must error (not silently match all): {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = env.run(&["list", "milestones", "--filter", "bogus-typo"]);
    assert!(!out.status.success(), "typo'd --filter must error");

    // A known preset still works.
    let out = env.run(&["list", "milestones", "--filter", "all", "--format", "json"]);
    assert!(
        out.status.success(),
        "known preset 'all' should work: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
