//! M110 AC-01 (S1): `mp milestone complete` caches byte-equal commands within
//! one invocation so AC verifications and step tests do not re-run the same shell.

use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;
use serde_json::json;

#[test]
fn complete_runs_byte_equal_command_once() {
    let env = TestEnv::new();
    let counter = env.tmp.path().join("gate-cache-counter.txt");
    let counter_path = counter.to_string_lossy();
    let shim = format!("sh -c 'echo 1 >> {counter_path}'");

    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();

    let milestone = json!({
        "milestone": {
            "id": "88",
            "title": "Gate cache fixture",
            "slug": "gate-cache-fixture",
            "spec_status": "ready",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-05",
            "updated": "2026-07-05",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "fixture" },
        "problem": { "description": "fixture" },
        "scope": { "in_scope": ["fixture"], "out_of_scope": ["a", "b"] },
        "verification": { "date": "", "branch": "", "evidence": "" },
        "acceptance_criteria": [
            {
                "id": "AC-01",
                "description": "first runnable",
                "verification": shim.clone(),
                "status": "pending",
                "evidence": "",
            },
            {
                "id": "AC-02",
                "description": "duplicate runnable",
                "verification": shim.clone(),
                "status": "pending",
                "evidence": "",
            },
        ],
        "steps": [
            {
                "id": "S1",
                "action": "fixture",
                "status": "done",
                "tests": shim,
                "done_when": "fixture",
                "files": [],
                "covers_ac": [],
                "depends_on_steps": [],
                "order": 1,
                "work_package": "WP1",
            },
        ],
        "work_packages": [
            { "id": "WP1", "name": "fixture", "goal": "fixture", "rollback": "" },
        ],
    });
    fs::write(
        dir.join("88-gate-cache-fixture.json"),
        format!("{}\n", serde_json::to_string_pretty(&milestone).unwrap()),
    )
    .unwrap();

    let out = lib_api::run(&env, &["milestone", "complete", "88", "--format", "json"]);
    assert!(
        out.status.success(),
        "complete failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let body = fs::read_to_string(&counter).expect("counter file");
    let runs = body.lines().filter(|l| !l.is_empty()).count();
    assert_eq!(
        runs, 1,
        "byte-equal command must run exactly once across ACs + step tests; counter=\n{body}"
    );
}
