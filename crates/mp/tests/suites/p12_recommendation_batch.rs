use std::process::Command;

use crate::common::TestEnv;

fn init_git(env: &TestEnv) {
    let root = env.tmp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .expect("git config name");
}

#[test]
fn brief_promote_to_idea_and_backlog() {
    let env = TestEnv::new();

    let topics = env.run(&["brief", "todo", "--format", "json"]);
    let topic_id = serde_json::from_slice::<serde_json::Value>(&topics.stdout).unwrap()["topics"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(env
        .run(&[
            "brief",
            "edit",
            &topic_id,
            "--body",
            "Add OAuth for GitHub and Google",
            "--format",
            "json",
        ])
        .status
        .success());

    let promote_idea = env.run(&[
        "brief",
        "promote",
        &topic_id,
        "--to-idea",
        "--format",
        "json",
    ]);
    assert!(
        promote_idea.status.success(),
        "{}",
        String::from_utf8_lossy(&promote_idea.stderr)
    );
    let idea_json: serde_json::Value = serde_json::from_slice(&promote_idea.stdout).unwrap();
    assert!(idea_json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("idea:"));

    let add = env.run(&["brief", "add", "--title", "Defer SAML", "--format", "json"]);
    assert!(add.status.success());
    let topic2 = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["topic"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(env
        .run(&[
            "brief",
            "edit",
            &topic2,
            "--body",
            "Not for v1",
            "--format",
            "json",
        ])
        .status
        .success());

    let promote_bl = env.run(&[
        "brief",
        "promote",
        &topic2,
        "--to-backlog",
        "--format",
        "json",
    ]);
    assert!(promote_bl.status.success());
    let bl_json: serde_json::Value = serde_json::from_slice(&promote_bl.stdout).unwrap();
    assert!(bl_json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("backlog:"));
}

#[test]
fn milestone_plan_scaffolds_work_packages_and_closure() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "OAuth Login",
        "depends_on": [],
        "effort": "M",
        "risk": "med",
        "intent": { "outcome": "User can sign in with OAuth." },
        "problem": { "description": "Auth required." },
        "scope": {
            "in_scope": ["OAuth"],
            "out_of_scope": ["Password login", "SAML"]
        },
        "acceptance_criteria": [
            {
                "description": "OAuth flow completes",
                "verification": "cargo test oauth"
            }
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
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());

    let plan = env.run(&[
        "milestone",
        "plan",
        &id,
        "--work-packages",
        "2",
        "--format",
        "json",
    ]);
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_json: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(plan_json["scaffolded"].as_bool().unwrap());
    assert!(plan_json["work_packages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|wp| wp == "WP-close"));

    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert!(show_json["work_packages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|wp| wp["id"] == "WP-close"));
}

#[test]
fn milestone_complete_triggers_git_commit_when_configured() {
    let env = TestEnv::blank();
    init_git(&env);
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());
    Command::new("git")
        .args(["add", "."])
        .current_dir(env.tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(env.tmp.path())
        .output()
        .unwrap();

    assert!(env
        .run(&[
            "config",
            "set",
            "git.commit_on_milestone_complete",
            "true",
            "--format",
            "json",
        ])
        .status
        .success());

    let create_json = r#"{
        "title": "Small fix",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "Done." },
        "problem": { "description": "Fix." },
        "scope": {
            "in_scope": ["fix"],
            "out_of_scope": ["other", "TBD"]
        },
        "acceptance_criteria": [
            { "description": "works", "verification": "cargo test" }
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
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());

    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "tests pass",
        "--force",
        "--format",
        "json",
    ]);
    assert!(
        complete.status.success(),
        "{}",
        String::from_utf8_lossy(&complete.stderr)
    );
    let complete_json: serde_json::Value = serde_json::from_slice(&complete.stdout).unwrap();
    assert_eq!(complete_json["git"]["committed"], true);

    let clean = env.run(&["git", "status", "--format", "json"]);
    let clean_json: serde_json::Value = serde_json::from_slice(&clean.stdout).unwrap();
    assert!(clean_json["clean"].as_bool().unwrap());
}
