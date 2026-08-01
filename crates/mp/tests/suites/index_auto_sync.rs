use crate::common::TestEnv;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "M",
        "risk": "med",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{
                "description": "{title} works",
                "verification": "cargo test"
            }}
        ]
    }}"#
    );
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn plan_json(env: &TestEnv) -> serde_json::Value {
    let plan_path = env.tmp.path().join("master-plan/plan.json");
    let raw = std::fs::read_to_string(&plan_path).unwrap();
    serde_json::from_str(&raw).expect("plan.json")
}

/// Find a milestone entry in the plan.json index by id suffix.
fn milestone_entry<'a>(plan: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    plan["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("milestone {id} not in index: {plan}"))
}

#[test]
fn auto_sync_on_create() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(
        entry["id"], id,
        "create should auto-sync new milestone into index"
    );
    assert_eq!(
        entry["spec_status"], "draft",
        "index should reflect spec_status=draft"
    );
    assert_eq!(
        entry["execution_status"], "planned",
        "index should reflect execution_status=planned"
    );
}

#[test]
fn auto_sync_on_approve() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(entry["id"], id, "milestone should remain in index");
    assert_eq!(
        entry["spec_status"], "ready",
        "approve should update index spec_status to ready"
    );
}

#[test]
fn auto_sync_on_set_status() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");
    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());

    let set = env.run(&[
        "milestone",
        "set-status",
        &id,
        "in-progress",
        "--format",
        "json",
    ]);
    assert!(
        set.status.success(),
        "set-status failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(entry["id"], id, "milestone should remain in index");
    assert_eq!(
        entry["execution_status"], "in-progress",
        "set-status should update index execution_status"
    );
}

#[test]
fn auto_sync_on_set_spec_status() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let set = env.run(&[
        "milestone",
        "set-spec-status",
        &id,
        "review",
        "--format",
        "json",
    ]);
    assert!(
        set.status.success(),
        "set-spec-status failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(entry["id"], id, "milestone should remain in index");
    assert_eq!(
        entry["spec_status"], "review",
        "set-spec-status should update index spec_status"
    );
}

#[test]
fn auto_sync_on_complete() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    // Must approve, pass ACs, then complete
    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());
    let pass = env.run(&[
        "milestone",
        "criterion",
        "pass",
        &id,
        "AC-01",
        "--evidence",
        "tested",
        "--format",
        "json",
    ]);
    assert!(
        pass.status.success(),
        "criterion pass failed: {}",
        String::from_utf8_lossy(&pass.stderr)
    );

    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "all done",
        "--force",
        "--skip-review",
        "--format",
        "json",
    ]);
    assert!(
        complete.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&complete.stderr)
    );

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(entry["id"], id, "milestone should remain in index");
    assert_eq!(
        entry["spec_status"], "verified",
        "complete should update index spec_status to verified"
    );
    assert_eq!(
        entry["execution_status"], "done",
        "complete should update index execution_status to done"
    );
}

#[test]
fn auto_sync_on_block() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let block = env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "waiting on review",
        "--format",
        "json",
    ]);
    assert!(
        block.status.success(),
        "block failed: {}",
        String::from_utf8_lossy(&block.stderr)
    );

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(entry["id"], id, "milestone should remain in index");
    assert_eq!(
        entry["execution_status"], "blocked",
        "block should update index execution_status to blocked"
    );
}

#[test]
fn auto_sync_on_split() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let split = env.run(&[
        "milestone",
        "split",
        &id,
        "--into",
        "2",
        "--titles",
        "OAuth core,OAuth UI",
        "--format",
        "json",
    ]);
    assert!(
        split.status.success(),
        "split failed: {}",
        String::from_utf8_lossy(&split.stderr)
    );

    let plan = plan_json(&env);
    let child_id = format!("{id}.1");
    assert!(
        milestone_entry(&plan, &child_id)["id"] == child_id,
        "split should auto-sync child milestone into index"
    );
}

#[test]
fn no_manual_sync_needed() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    // Before the auto-sync fix, this sequence required a manual `mp sync`
    // between create and approve for the index to reflect `ready`.
    // Now it should be consistent immediately.
    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(approve.status.success());

    let validate = env.run(&["validate", "--format", "json"]);
    assert!(
        validate.status.success(),
        "validate should pass without manual sync"
    );
}

#[test]
fn auto_sync_on_step_auto_close() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "wp",
            "add",
            &id,
            "--name",
            "Work",
            "--id",
            "WP1",
            "--goal",
            "Do it",
            "--format",
            "json",
        ])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "only step",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    assert!(env
        .run(&["milestone", "step", "done", &id, "S1", "--format", "json"])
        .status
        .success());

    let plan = plan_json(&env);
    let entry = milestone_entry(&plan, &id);
    assert_eq!(
        entry["execution_status"], "in-progress",
        "all steps done should keep execution in-progress until milestone complete"
    );

    let validate = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&validate.stdout).unwrap_or_else(|_| {
        panic!(
            "validate should emit JSON: {}",
            String::from_utf8_lossy(&validate.stderr)
        )
    });
    let w03: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W03")
        .collect();
    assert!(
        w03.is_empty(),
        "no W03 drift expected after step mutation with auto-sync, got: {w03:?}"
    );
}
