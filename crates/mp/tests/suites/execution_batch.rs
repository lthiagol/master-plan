use crate::common::{lib_api, TestEnv};

#[test]
fn path_pin_focus_plan_gaps_and_execution_check() {
    let env = TestEnv::from_fixture("linear-deps");

    let pin = lib_api::run(
        &env,
        &["path", "pin", "03", "--before", "02", "--format", "json"],
    );
    assert!(
        pin.status.success(),
        "{}",
        String::from_utf8_lossy(&pin.stderr)
    );

    let focus = lib_api::run(&env, &["path", "focus", "02", "--format", "json"]);
    assert!(focus.status.success());

    let gaps = lib_api::run(&env, &["plan", "gaps", "02", "--format", "json"]);
    assert!(gaps.status.success());
    let gaps_json: serde_json::Value = serde_json::from_slice(&gaps.stdout).unwrap();
    assert_eq!(gaps_json["milestone_id"], "02");

    let steps = lib_api::run(
        &env,
        &["list", "steps", "--milestone", "02", "--format", "json"],
    );
    assert!(steps.status.success());
    let steps_json: serde_json::Value = serde_json::from_slice(&steps.stdout).unwrap();
    assert!(steps_json["steps"].as_array().unwrap().len() >= 2);

    let status_json = lib_api::run_json(&env, &["status", "--format", "json"]);
    assert!(status_json.get("suggested_path").is_some());

    let check_json = lib_api::run_json(&env, &["execution", "check", "--format", "json"]);
    assert_eq!(check_json["can_handoff"], true);
}

#[test]
fn milestone_decompose_scaffolds_work_packages() {
    let env = TestEnv::blank();
    assert!(lib_api::run(&env, &["init", "--format", "json"])
        .status
        .success());

    let create_json = r#"{
        "title": "Feature",
        "intent": { "outcome": "Ship it." },
        "problem": { "description": "Missing feature." },
        "scope": { "in_scope": ["core"], "out_of_scope": ["mobile", "admin"] },
        "acceptance_criteria": [
            { "description": "Works", "verification": "cargo test" }
        ]
    }"#;
    let create = lib_api::run(
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
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap();

    for status in ["interview", "review", "ready"] {
        assert!(env
            .run(&[
                "milestone",
                "set-spec-status",
                id,
                status,
                "--format",
                "json"
            ])
            .status
            .success());
    }

    let decompose = lib_api::run(
        &env,
        &[
            "milestone",
            "decompose",
            id,
            "--work-packages",
            "2",
            "--format",
            "json",
        ],
    );
    assert!(decompose.status.success());
    let report: serde_json::Value = serde_json::from_slice(&decompose.stdout).unwrap();
    assert_eq!(report["scaffolded"], true);
    assert!(!report["gaps"]["missing"].as_array().unwrap().is_empty());
}
