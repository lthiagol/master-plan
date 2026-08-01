use crate::common::TestEnv;

fn create_and_approve(env: &TestEnv, title: &str) -> String {
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
                "verification": "manual: accepted — release ship fixture"
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

    // Approve so it can be set to in-progress, then complete it
    let approve = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let start = env.run(&[
        "milestone",
        "set-status",
        &id,
        "in-progress",
        "--format",
        "json",
    ]);
    assert!(
        start.status.success(),
        "set-status in-progress failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
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
    let complete = env.run(&["milestone", "complete", &id, "--force", "--format", "json"]);
    assert!(
        complete.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&complete.stderr)
    );

    id
}

#[test]
fn release_ship_marks_shipped() {
    let env = TestEnv::new();
    let id = create_and_approve(&env, "Auth Module");

    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    let ship = env.run(&["release", "ship", "1.0.0", "--format", "json"]);
    assert!(
        ship.status.success(),
        "ship failed: {}",
        String::from_utf8_lossy(&ship.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&ship.stdout).unwrap();
    assert_eq!(json["status"], "shipped");
    assert!(
        !json["date"].as_str().unwrap().is_empty(),
        "date should be set"
    );
}

#[test]
fn release_ship_refuses_not_done() {
    let env = TestEnv::new();
    let id = {
        let create_json = r#"{
            "title": "Unfinished",
            "depends_on": [],
            "effort": "M",
            "risk": "med",
            "intent": { "outcome": "Unfinished" },
            "problem": { "description": "Not done yet" },
            "scope": { "in_scope": ["X"], "out_of_scope": ["Y", "Z"] },
            "acceptance_criteria": [
                { "description": "X works", "verification": "echo ok" }
            ]
        }"#;
        let out = env.run(&[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ]);
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    let ship = env.run(&["release", "ship", "1.0.0", "--format", "json"]);
    assert!(
        !ship.status.success(),
        "should refuse to ship with unfinished milestones"
    );
}

#[test]
fn release_ship_force_bypasses() {
    let env = TestEnv::new();
    let id = {
        let create_json = r#"{
            "title": "Unfinished",
            "depends_on": [],
            "effort": "M",
            "risk": "med",
            "intent": { "outcome": "Unfinished" },
            "problem": { "description": "Not done yet" },
            "scope": { "in_scope": ["X"], "out_of_scope": ["Y", "Z"] },
            "acceptance_criteria": [
                { "description": "X works", "verification": "echo ok" }
            ]
        }"#;
        let out = env.run(&[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ]);
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    env.run(&[
        "milestone",
        "set-target-version",
        &id,
        "1.0.0",
        "--format",
        "json",
    ]);
    let ship = env.run(&["release", "ship", "1.0.0", "--force", "--format", "json"]);
    assert!(
        ship.status.success(),
        "force ship failed: {}",
        String::from_utf8_lossy(&ship.stderr)
    );
}
