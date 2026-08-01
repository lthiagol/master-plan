use crate::common::TestEnv;

#[test]
fn skill_context_returns_plan_state() {
    let env = TestEnv::new();

    let context = env.run(&["skill", "context", "--format", "json"]);
    assert!(
        context.status.success(),
        "skill context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&context.stdout).expect("json");
    assert_eq!(
        result["ok"],
        serde_json::Value::Null,
        "should have no ok field (raw report)"
    );
    assert!(
        result.get("project_name").is_some(),
        "project_name should be present"
    );
    assert!(
        result["profile"].as_str().is_some_and(|p| !p.is_empty()),
        "profile should not be empty"
    );
}

#[test]
fn skill_context_no_active_milestones() {
    let env = TestEnv::new();

    let context = env.run(&["skill", "context", "--format", "json"]);
    assert!(context.status.success());

    let result: serde_json::Value = serde_json::from_slice(&context.stdout).expect("json");
    let active = result["active_milestones"]
        .as_array()
        .expect("active_milestones");
    // Fresh init should have no in-progress milestones
    assert!(
        active.is_empty(),
        "fresh init should have no active milestones"
    );
}
