//! Plan write concurrency — sequential step adds must preserve the full step chain.
//!
//! Parallel `mp step add` calls on the same milestone can clobber each other (read-modify-write
//! on one TOML file). Agents must chain plan writes sequentially; this test guards the happy path.

use crate::common::TestEnv;

fn create_and_approve(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "manual: concurrency setup sanity check" }}
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
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
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
    id
}

#[test]
fn sequential_step_chain_preserved() {
    let env = TestEnv::new();
    let id = create_and_approve(&env, "concurrency-seq");

    for i in 1..=3 {
        let step_id = format!("S{i}");
        let action = format!("action {step_id}");
        let out = env.run(&[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--id",
            &step_id,
            "--action",
            &action,
            "--tests",
            "echo ok",
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "step add {step_id} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let json = env.run_json(&["show", "milestone", &id, "--format", "json"]);
    let steps = json["steps"].as_array().unwrap();
    let ids: Vec<&str> = steps.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(ids.len(), 3, "expected three steps, got: {ids:?}");
    for expected in ["S1", "S2", "S3"] {
        assert!(
            ids.contains(&expected),
            "missing step {expected} in {ids:?}"
        );
    }
}
