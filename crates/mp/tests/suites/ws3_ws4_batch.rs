use crate::common::TestEnv;

#[test]
fn graph_inbox_hygiene_and_status() {
    let env = TestEnv::new();

    let graph_json = env.run_json(&["graph", "--format", "json"]);
    assert!(graph_json["baseline_order"].is_array());

    let explain = env.run(&["graph", "explain", "01", "--format", "json"]);
    // no milestones after bare init — explain may fail; create one first
    if !explain.status.success() {
        let create_json = r#"{
            "title": "Graph test",
            "intent": { "outcome": "Test graph." },
            "problem": { "description": "Need milestone." },
            "scope": { "in_scope": ["g"], "out_of_scope": ["a", "b"] },
            "acceptance_criteria": [{ "description": "ok", "verification": "test" }]
        }"#;
        assert!(env
            .run(&[
                "milestone",
                "create",
                "--json",
                create_json,
                "--format",
                "json"
            ])
            .status
            .success());
        let explain2 = env.run(&["graph", "explain", "01", "--format", "json"]);
        assert!(
            explain2.status.success(),
            "{}",
            String::from_utf8_lossy(&explain2.stderr)
        );
    }

    let inbox = env.run(&["inbox", "--format", "json"]);
    assert!(inbox.status.success());

    let hygiene = env.run(&["hygiene", "--stale-days", "30", "--format", "json"]);
    assert!(hygiene.status.success());

    let status_json = env.run_json(&["status", "--format", "json"]);
    assert!(status_json.get("inbox_count").is_some());
    assert!(status_json["blockers"].is_array());
}

#[test]
fn charter_backlog_decisions_config() {
    let env = TestEnv::new();

    assert!(env
        .run(&["plan", "goals", "add", "Ship v1", "--format", "json"])
        .status
        .success());

    assert!(env
        .run(&[
            "plan",
            "set",
            "--planning-status",
            "in-execution",
            "--format",
            "json",
        ])
        .status
        .success());

    let plan = env.run_json(&["plan", "show", "--format", "json"]);
    assert_eq!(plan["plan"]["project"]["planning_status"], "in-execution");

    let backlog = env.run(&[
        "backlog",
        "add",
        "--desc",
        "GitHub OAuth",
        "--priority",
        "high",
        "--format",
        "json",
    ]);
    assert!(backlog.status.success());
    let item_id = crate::common::json_from_stdout(&backlog.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(env
        .run(&[
            "backlog",
            "resolve",
            &item_id,
            "--wont-fix",
            "--reason",
            "v2",
            "--format",
            "json",
        ])
        .status
        .success());

    assert!(env
        .run(&[
            "decision",
            "add",
            "--summary",
            "Use TOML on disk",
            "--format",
            "json",
        ])
        .status
        .success());

    let decisions = env.run(&["list", "decisions", "--format", "json"]);
    assert!(decisions.status.success());

    assert!(env
        .run(&["config", "set", "next.prefer", "track", "--format", "json",])
        .status
        .success());

    let cfg_json = env.run_json(&["config", "get", "next.prefer", "--format", "json"]);
    assert_eq!(cfg_json["value"], "track");

    assert!(env
        .run(&[
            "plan",
            "metrics",
            "set",
            "--unit-tests",
            "42",
            "--format",
            "json",
        ])
        .status
        .success());
}
