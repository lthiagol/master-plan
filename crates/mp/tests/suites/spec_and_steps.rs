use std::fs;

use crate::common::TestEnv;

#[test]
fn milestone_spec_workflow_update_and_set_status() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "OAuth Login",
        "intent": { "outcome": "User signs in." },
        "problem": { "description": "Need auth." },
        "scope": {
            "in_scope": ["OAuth"],
            "out_of_scope": ["Password", "SAML"]
        },
        "acceptance_criteria": [
            { "description": "Flow works", "verification": "cargo test oauth" }
        ]
    }"#;

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    assert!(create.status.success());
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    let interview = env.run(&[
        "milestone",
        "set-spec-status",
        id,
        "interview",
        "--format",
        "json",
    ]);
    assert!(interview.status.success());

    let update_json = r#"{"intent":{"outcome":"User signs in with Google OAuth."}}"#;
    let update = env.run(&[
        "milestone",
        "update",
        id,
        "--json",
        update_json,
        "--format",
        "json",
    ]);
    assert!(update.status.success());
    let updated: serde_json::Value = serde_json::from_slice(&update.stdout).unwrap();
    assert_eq!(updated["milestone"]["spec_status"], "review");

    let review = env.run(&[
        "milestone",
        "set-spec-status",
        id,
        "review",
        "--format",
        "json",
    ]);
    assert!(review.status.success());

    let ready = env.run(&[
        "milestone",
        "set-spec-status",
        id,
        "ready",
        "--format",
        "json",
    ]);
    assert!(ready.status.success());
    let ready_json: serde_json::Value = serde_json::from_slice(&ready.stdout).unwrap();
    assert_eq!(ready_json["milestone"]["spec_status"], "ready");
}

#[test]
fn step_add_set_status_and_done_on_approved_milestone() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let add = env.run(&[
        "milestone",
        "step",
        "add",
        "03",
        "--wp",
        "WP1",
        "--action",
        "Add rate limiting to callback",
        "--tests",
        "cargo test rate_limit",
        "--done-when",
        "Tests pass",
        "--covers-ac",
        "AC-01",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(added["step"]["id"], "S4");
    assert_eq!(added["step"]["work_package"], "WP1");

    let done = env.run(&["milestone", "step", "done", "03", "S4", "--format", "json"]);
    assert!(done.status.success());
    let done_json: serde_json::Value = serde_json::from_slice(&done.stdout).unwrap();
    assert_eq!(done_json["step"]["status"], "done");

    let milestone_dir = env.tmp.path().join("master-plan/milestones");
    let file = fs::read_dir(&milestone_dir)
        .expect("milestones")
        .find_map(|e| {
            let p = e.ok()?.path();
            if p.file_name()?.to_string_lossy().starts_with("03-") {
                Some(p)
            } else {
                None
            }
        })
        .expect("milestone 03 file");
    let raw = fs::read_to_string(&file).expect("read milestone");
    // New step is written to the top-level `"steps"` array (merged out of the
    // legacy `work_packages[].steps` shape, whose nested `steps` key is no
    // longer serialized).
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("milestone json");
    let steps = parsed["steps"].as_array().expect("top-level steps array");
    assert!(
        steps.iter().any(|s| s["id"] == "S4"),
        "S4 should be in top-level steps"
    );
    // Legacy nested steps under work_packages should be gone.
    for wp in parsed["work_packages"].as_array().into_iter().flatten() {
        assert!(
            wp.get("steps").is_none(),
            "work_packages should not carry nested steps anymore"
        );
    }
}
