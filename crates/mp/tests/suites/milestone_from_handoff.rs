use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn milestone_create_from_handoff_creates_milestones() {
    let env = TestEnv::new();

    let handoff = r#"# CLI foundation
Implement the basic CLI structure with command routing.

## Adoption and PM surface
Build adoption workflows and project management surface.

## Brownfield and execution
Support brownfield projects and execution tracking.
"#;

    let handoff_path = env.tmp.path().join("handoff.md");
    std::fs::write(&handoff_path, handoff).expect("write handoff");

    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--from-handoff",
            &handoff_path.to_string_lossy(),
            "--format",
            "json",
        ],
    );
    assert!(
        create.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&create.stdout).expect("create json");
    assert_eq!(result["ok"], true);
    assert_eq!(result["milestones_created"], 3, "expected 3 milestones");

    let milestones = result["milestones"].as_array().expect("milestones array");
    assert_eq!(milestones[0]["title"], "CLI foundation");
    assert_eq!(milestones[1]["title"], "Adoption and PM surface");
    assert_eq!(milestones[2]["title"], "Brownfield and execution");

    let show = lib_api::run(&env, &["show", "milestone", "01", "--format", "json"]);
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    assert_eq!(shown["milestone"]["title"], "CLI foundation");
    assert_eq!(shown["milestone"]["spec_status"], "draft");
}

#[test]
fn milestone_create_from_handoff_empty_fails() {
    let env = TestEnv::new();

    let handoff_path = env.tmp.path().join("empty.md");
    std::fs::write(&handoff_path, "Just text without headings.\n").expect("write");

    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--from-handoff",
            &handoff_path.to_string_lossy(),
            "--format",
            "json",
        ],
    );
    assert!(!create.status.success(), "should fail on empty handoff");
}
