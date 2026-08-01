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
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn release_map_lists_planned_releases() {
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

    let map = env.run(&["release", "map", "--format", "json"]);
    assert!(
        map.status.success(),
        "release map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&map.stdout).unwrap();
    let planned = json["planned"].as_array().unwrap();
    assert!(
        !planned.is_empty(),
        "should have at least one planned release"
    );
    assert_eq!(planned[0]["version"], "1.0.0");
    assert!(planned[0]["milestones"]
        .as_array()
        .unwrap()
        .contains(&serde_json::Value::String(id)));
}

#[test]
fn release_show_returns_release_info() {
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

    let show = env.run(&["release", "show", "1.0.0", "--format", "json"]);
    assert!(
        show.status.success(),
        "release show failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(json["release"]["version"], "1.0.0");
    assert_eq!(json["release"]["status"], "planned");
    assert!(json["release"]["milestones"]
        .as_array()
        .unwrap()
        .contains(&serde_json::Value::String(id)));
}

#[test]
fn release_list_empty() {
    let env = TestEnv::new();
    let list = env.run(&["release", "list", "--format", "json"]);
    assert!(list.status.success());
    let json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(
        json["releases"].as_array().unwrap().is_empty(),
        "no releases should exist yet"
    );
}
