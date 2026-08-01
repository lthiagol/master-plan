use crate::common::TestEnv;

fn create_and_complete(env: &TestEnv, title: &str, version: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
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
    let id = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "approve", &id, "--format", "json"]);
    env.run(&[
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
    env.run(&["milestone", "complete", &id, "--force", "--format", "json"]);
    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        version,
        "--format",
        "json",
    ]);
    id
}

#[test]
fn changelog_generate_creates_version_section() {
    let env = TestEnv::new();
    create_and_complete(&env, "Auth Module", "1.0.0");

    let out = env.run(&[
        "changelog",
        "generate",
        "--version",
        "1.0.0",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(
        content.contains("## v1.0.0"),
        "should contain version header"
    );
    assert!(
        content.contains("Auth Module"),
        "should contain milestone title"
    );
}

#[test]
fn changelog_generate_refuses_duplicate() {
    let env = TestEnv::new();
    create_and_complete(&env, "Auth Module", "1.0.0");

    env.run(&[
        "changelog",
        "generate",
        "--version",
        "1.0.0",
        "--format",
        "json",
    ]);
    let second = env.run(&[
        "changelog",
        "generate",
        "--version",
        "1.0.0",
        "--format",
        "json",
    ]);
    assert!(
        !second.status.success(),
        "second generate should refuse duplicate"
    );
}
