use std::fs;

use crate::common::TestEnv;

fn write_ready_milestone(env: &TestEnv, id: &str, title: &str) {
    let create_json = format!(
        r#"{{
            "title": "{title}",
            "intent": {{ "outcome": "Test outcome." }},
            "problem": {{ "description": "Need milestone." }},
            "scope": {{ "in_scope": ["a"], "out_of_scope": ["b", "c"] }},
            "acceptance_criteria": [{{ "description": "ok", "verification": "manual: p5 polish batch setup" }}]
        }}"#
    );
    assert!(env
        .run(&[
            "milestone",
            "create",
            "--json",
            &create_json,
            "--format",
            "json"
        ])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-spec-status",
            id,
            "review",
            "--format",
            "json",
        ])
        .status
        .success());
    assert!(env
        .run(&["milestone", "approve", id, "--format", "json"])
        .status
        .success());
}

#[test]
fn execution_status_and_digest() {
    let env = TestEnv::new();

    let json = env.run_json(&["execution", "status", "--format", "json"]);
    assert_eq!(json["mode"], "planning");

    let digest_json = env.run_json(&["digest", "--since", "7d", "--format", "json"]);
    assert!(digest_json["summary"].is_string());
}

#[test]
fn delta_rebase_updates_base_version() {
    let env = TestEnv::new();
    assert!(env
        .run(&["specs", "init", "api", "--format", "json"])
        .status
        .success());

    let spec_path = env.tmp.path().join("master-plan/specs/api.json");
    fs::write(
        &spec_path,
        fs::read_to_string(&spec_path)
            .unwrap()
            .replace("\"version\": 1", "\"version\": 3"),
    )
    .unwrap();

    let milestone = serde_json::json!({
        "milestone": {
            "id": "04",
            "title": "Delta",
            "slug": "delta",
            "spec_status": "implemented",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "delta",
            "priority": "normal",
            "created": "2026-06-17",
            "updated": "2026-06-17",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "Change." },
        "problem": { "description": "Stale." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "ok",
            "verification": "test",
            "status": "passed",
            "evidence": "",
        }],
        "verification": { "date": "", "branch": "", "evidence": "" },
        "delta": {
            "domain": "api",
            "base_version": 1,
            "added": [{
                "id": "REQ-01",
                "statement": "New.",
            }],
        },
    });
    fs::write(
        env.tmp.path().join("master-plan/milestones/04-delta.json"),
        format!("{}\n", serde_json::to_string_pretty(&milestone).unwrap()),
    )
    .unwrap();

    let rebase = env.run(&["specs", "delta", "rebase", "04", "--format", "json"]);
    assert!(
        rebase.status.success(),
        "{}",
        String::from_utf8_lossy(&rebase.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&rebase.stdout).unwrap();
    assert_eq!(out["base_version"], 3);
}

#[test]
fn challenge_flow_audit_and_done() {
    let env = TestEnv::new();
    write_ready_milestone(&env, "01", "Challenge me");

    assert!(env
        .run(&[
            "milestone",
            "challenge",
            "start",
            "01",
            "--scope",
            "plan",
            "--format",
            "json"
        ])
        .status
        .success());

    let audit = env.run(&["milestone", "challenge", "audit", "01", "--format", "json"]);
    assert!(
        audit.status.success(),
        "{}",
        String::from_utf8_lossy(&audit.stderr)
    );
    let audit_json: serde_json::Value = serde_json::from_slice(&audit.stdout).unwrap();
    assert!(!audit_json["findings"].as_array().unwrap().is_empty());

    let list = env.run(&["milestone", "challenge", "list", "01", "--format", "json"]);
    assert!(list.status.success());
    let findings: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let open = findings["findings"].as_array().unwrap();
    for f in open {
        let fid = f["id"].as_str().unwrap();
        assert!(env
            .run(&[
                "milestone",
                "challenge",
                "dismiss",
                "01",
                fid,
                "--reason",
                "test",
                "--format",
                "json",
            ])
            .status
            .success());
    }

    let done_json = env.run_json(&["milestone", "challenge", "done", "01", "--format", "json"]);
    assert_eq!(done_json["challenge"]["status"], "closed");
}

#[test]
fn session_export_markdown() {
    let env = TestEnv::new();

    let start = env.run(&[
        "session",
        "start",
        "--branch",
        "feature/oauth",
        "--title",
        "OAuth session",
        "--format",
        "json",
    ]);
    assert!(start.status.success());
    let started: serde_json::Value = serde_json::from_slice(&start.stdout).unwrap();
    let sid = started["session_id"].as_str().unwrap();

    let export = env.run(&["session", "export", sid, "--format", "json"]);
    assert!(export.status.success());
    let export_json: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    assert!(export_json["body"]
        .as_str()
        .unwrap_or("")
        .contains("OAuth session"));
    assert!(export_json["body"]
        .as_str()
        .unwrap_or("")
        .contains("feature/oauth"));
}
