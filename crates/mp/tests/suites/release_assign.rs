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
                "verification": "echo ok"
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
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn read_plan_json(env: &TestEnv) -> serde_json::Value {
    let raw = std::fs::read_to_string(env.tmp.path().join("master-plan/plan.json")).unwrap();
    serde_json::from_str(&raw).expect("plan.json")
}

/// Find a release entry by version in the plan.json index.
fn release_entry<'a>(plan: &'a serde_json::Value, version: &str) -> Option<&'a serde_json::Value> {
    plan["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["version"].as_str() == Some(version))
}

#[test]
fn set_target_version_updates_milestone() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let out = env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "set-target-version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show_json["milestone"]["target_version"], "1.0.0");
}

#[test]
fn set_target_version_updates_release_registry() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "OAuth Login");

    let out = env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let plan = read_plan_json(&env);
    let release = release_entry(&plan, "1.0.0").expect("plan.json should have a 1.0.0 release");
    let milestones = release["milestones"].as_array().expect("milestones array");
    assert!(
        milestones.iter().any(|m| m.as_str() == Some(id.as_str())),
        "plan.json should list milestone in release: {release}"
    );
}

#[test]
fn set_target_version_multiple_milestones_same_version() {
    let env = TestEnv::new();
    let id1 = create_milestone(&env, "Auth Module");
    let id2 = create_milestone(&env, "UI Update");

    env.run(&[
        "milestone",
        "set-target-version",
        &id1,
        "1.0.0",
        "--format",
        "json",
    ]);
    env.run(&[
        "milestone",
        "set-target-version",
        &id2,
        "1.0.0",
        "--format",
        "json",
    ]);

    let plan = read_plan_json(&env);
    let release = release_entry(&plan, "1.0.0").expect("1.0.0 release");
    let milestones = release["milestones"].as_array().unwrap();
    assert!(
        milestones.iter().any(|m| m.as_str() == Some(id1.as_str())),
        "first milestone in release"
    );
    assert!(
        milestones.iter().any(|m| m.as_str() == Some(id2.as_str())),
        "second milestone in release"
    );
}

#[test]
fn set_target_version_changes_existing() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "Auth Module");

    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "2.0.0",
        "--format",
        "json",
    ]);

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show_json["milestone"]["target_version"], "2.0.0");

    let plan = read_plan_json(&env);
    assert!(
        release_entry(&plan, "2.0.0").is_some(),
        "plan.json should have a 2.0.0 release"
    );
}
