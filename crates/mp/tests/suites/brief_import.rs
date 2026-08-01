use crate::common::TestEnv;

#[test]
fn brief_import_from_handoff_file() {
    let env = TestEnv::new();

    let handoff = r#"# Problem & motivation
Users need a way to bootstrap plans from existing handoff documents.

## Target audience
Development teams adopting Master Plan for brownfield projects.

## Core capabilities
- Parse handoff markdown sections
- Match sections to brief topics
- Create new topics for unmatched sections

## Constraints
Must handle standard markdown heading syntax.
"#;

    let handoff_path = env.tmp.path().join("handoff.md");
    std::fs::write(&handoff_path, handoff).expect("write handoff");

    let import = env.run(&[
        "brief",
        "import",
        "--from-file",
        &handoff_path.to_string_lossy(),
        "--format",
        "json",
    ]);
    assert!(
        import.status.success(),
        "import failed: {}",
        String::from_utf8_lossy(&import.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&import.stdout).expect("import json");
    assert_eq!(result["ok"], true, "import not ok");
    assert!(
        result["topics_added"].as_i64().unwrap_or(0) > 0,
        "no topics added"
    );

    let show = env.run(&["brief", "show", "--format", "json"]);
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    let topics = shown["brief"]["topics"].as_array().expect("topics array");
    let matching: Vec<&serde_json::Value> = topics
        .iter()
        .filter(|t| {
            t["body"]
                .as_str()
                .is_some_and(|b| b.contains("bootstrap plans"))
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "should have exactly one topic matching the handoff content"
    );
    assert_eq!(matching[0]["status"], "filled");
}

#[test]
fn brief_import_empty_handoff_fails() {
    let env = TestEnv::new();

    let handoff_path = env.tmp.path().join("empty.md");
    std::fs::write(&handoff_path, "Just some text without headings.\n").expect("write");

    let import = env.run(&[
        "brief",
        "import",
        "--from-file",
        &handoff_path.to_string_lossy(),
        "--format",
        "json",
    ]);
    assert!(!import.status.success(), "should fail on empty handoff");
}

#[test]
fn brief_import_missing_file_fails() {
    let env = TestEnv::new();

    let import = env.run(&[
        "brief",
        "import",
        "--from-file",
        "/tmp/nonexistent-handoff-12345.md",
        "--format",
        "json",
    ]);
    assert!(!import.status.success(), "should fail on missing file");
}

#[test]
fn brief_todo_and_done_advances_planning_phase() {
    let env = TestEnv::new();

    let todo_json = env.run_json(&["brief", "todo", "--format", "json"]);
    assert!(todo_json["pending_count"].as_u64().unwrap() > 0);

    for id in ["T01", "T02", "T03", "T04", "T06", "T07", "T08"] {
        assert!(env
            .run(&["brief", "edit", id, "--body", "filled", "--format", "json"])
            .status
            .success());
    }
    assert!(env
        .run(&["brief", "skip", "T05", "--format", "json"])
        .status
        .success());

    let done_json = env.run_json(&["brief", "done", "--format", "json"]);
    assert_eq!(done_json["ok"], true);

    let plan: serde_json::Value =
        serde_json::from_slice(&env.run(&["plan", "show", "--format", "json"]).stdout).unwrap();
    assert_eq!(plan["plan"]["project"]["planning_phase"], "charter");
}

fn fill_brief_for_done(env: &TestEnv) {
    for id in ["T01", "T02", "T03", "T04", "T06", "T07", "T08"] {
        assert!(env
            .run(&["brief", "edit", id, "--body", "filled", "--format", "json"])
            .status
            .success());
    }
    assert!(env
        .run(&["brief", "skip", "T05", "--format", "json"])
        .status
        .success());
}

#[test]
fn brief_reopen_resets_phase() {
    let env = TestEnv::new();

    fill_brief_for_done(&env);
    assert!(env
        .run(&["brief", "done", "--format", "json"])
        .status
        .success());
    let plan_done: serde_json::Value =
        serde_json::from_slice(&env.run(&["plan", "show", "--format", "json"]).stdout).unwrap();
    assert_eq!(plan_done["plan"]["project"]["planning_phase"], "charter");

    let reopen = env.run(&["brief", "reopen", "--format", "json"]);
    assert!(
        reopen.status.success(),
        "{}",
        String::from_utf8_lossy(&reopen.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&reopen.stdout).unwrap();
    assert_eq!(json["status"], "in_progress");
    assert_eq!(json["planning_phase"], "brief");

    let brief: serde_json::Value =
        serde_json::from_slice(&env.run(&["brief", "show", "--format", "json"]).stdout).unwrap();
    assert_eq!(brief["brief"]["brief"]["status"], "in_progress");
}
