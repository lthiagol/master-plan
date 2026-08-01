//! Shared helpers for M196 review-gate tests.

use crate::common::TestEnv;
use serde_json::json;
// Used by the parent suite module via `crate::common::TestEnv`.

pub fn create_open_milestone(env: &TestEnv, change_kind: Option<&str>) -> String {
    let title = match change_kind {
        Some(ck) => format!("M196 Review Gate Test (change_kind={ck})"),
        None => "M196 Review Gate Test".to_string(),
    };
    let json = json!({
        "title": title,
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {"outcome": "Pin the review gate"},
        "problem": {"description": "Without the gate, complete-on-no-review is silently terminal."},
        "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
        // Q-02: empty / missing change_kind is non-track (fail closed).
        "change_kind": change_kind.unwrap_or(""),
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ]
    });
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &json.to_string(),
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

pub fn drive_to_in_progress(env: &TestEnv, id: &str) {
    assert!(env
        .run(&["milestone", "approve", id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "wp", "add", id, "--name", "WP1"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            id,
            "--wp",
            "WP1",
            "--action",
            "task",
            "--tests",
            "manual: ok",
        ])
        .status
        .success());
    assert!(env
        .run(&["milestone", "set-status", id, "in-progress"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "step", "set-status", id, "S1", "done"])
        .status
        .success());
}

pub fn read_lifecycle(env: &TestEnv, id: &str) -> String {
    let out = env.run(&["show", "milestone", id, "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["lifecycle"].as_str().unwrap().to_string()
}

pub fn read_evidence(env: &TestEnv, id: &str) -> String {
    let out = env.run(&["show", "milestone", id, "--fields", "verification.evidence"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["verification"]["evidence"]
        .as_str()
        .unwrap_or("")
        .to_string()
}
