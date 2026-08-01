//! Schema enforcement on step/wp write paths (not just milestone create/update).

use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;

fn milestone_file_path(env: &TestEnv, id: &str) -> std::path::PathBuf {
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    for entry in fs::read_dir(&plan_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&format!("{id}-")) {
            return entry.path();
        }
    }
    panic!("milestone file not found for id {id}");
}

#[test]
fn step_add_rejects_invalid_milestone_shape() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "schema-step",
        "intent": { "outcome": "Ship schema-step" },
        "problem": { "description": "Need schema-step." },
        "scope": {
            "in_scope": ["schema-step"],
            "out_of_scope": ["Other", "TBD"]
        },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: schema-step setup sanity check" }
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ],
    );
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

    // Invalidate the on-disk shape while keeping spec_status ready.
    let path = milestone_file_path(&env, &id);
    let content = fs::read_to_string(&path).unwrap();
    let broken = content.replace("\"outcome\": \"Ship schema-step\"", "\"outcome\": \"\"");
    fs::write(&path, broken).unwrap();

    let step = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "work",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        !step.status.success(),
        "step add should reject invalid milestone under strict schema"
    );
    let stderr = String::from_utf8_lossy(&step.stderr);
    assert!(
        stderr.contains("schema validation failed"),
        "expected schema enforcement error, got: {stderr}"
    );
}
