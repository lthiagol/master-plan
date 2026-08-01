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

fn plan_index_has(env: &TestEnv, id: &str) -> bool {
    let plan_path = env.tmp.path().join("master-plan/plan.json");
    let raw = std::fs::read_to_string(&plan_path).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&raw).unwrap();
    plan["milestones"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|m| m["id"].as_str() == Some(id))
}

#[test]
fn sync_rebuilds_plan_milestone_index() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    // Auto-sync on create: milestone is already in the plan index
    assert!(
        plan_index_has(&env, &id),
        "create should auto-sync milestone into index"
    );

    // Running sync again is idempotent
    let sync = env.run(&["sync", "--format", "json"]);
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );

    let validate_after = env.run(&["validate", "--format", "json"]);
    assert!(validate_after.status.success());
    assert!(plan_index_has(&env, &id));
}

#[test]
fn milestone_split_creates_decimal_children() {
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
        "{}",
        String::from_utf8_lossy(&split.stderr)
    );
    let split_json: serde_json::Value = serde_json::from_slice(&split.stdout).unwrap();
    let children = split_json["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], format!("{id}.1"));
    assert_eq!(children[1], format!("{id}.2"));

    let show_child = env.run(&["show", "milestone", &format!("{id}.1"), "--format", "json"]);
    assert!(show_child.status.success());
    let child_json: serde_json::Value = serde_json::from_slice(&show_child.stdout).unwrap();
    assert_eq!(child_json["milestone"]["title"], "OAuth core");
    assert_eq!(
        child_json["milestone"]["depends_on"]
            .as_array()
            .unwrap()
            .first()
            .and_then(|v| v.as_str()),
        Some(id.as_str())
    );
}
