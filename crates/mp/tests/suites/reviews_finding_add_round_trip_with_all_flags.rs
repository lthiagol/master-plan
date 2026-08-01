//! M101 R2: CLI round-trip — `mp reviews finding add` accepts every new
//! M101 flag (--phase, --anchor, --summary, --rationale, --confidence,
//! --tags) and persists them on the milestone. Pinned by an
//! end-to-end test that files a finding with every flag set and asserts
//! every field survives the round-trip via `mp reviews show`.

use crate::common::lib_api;
use crate::common::TestEnv;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let payload = serde_json::json!({
        "title": title,
        "intent": { "outcome": "M101 R2 round-trip" },
        "problem": { "description": "M101 R2 round-trip regression" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "AC-14", "verification": "echo ok" }],
        "spec_status": "ready",
    })
    .to_string();
    let out = lib_api::run(env, &["milestone", "create", "--json", &payload]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn reviews_finding_add_round_trip_with_all_flags() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "finding-round-trip");

    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "medium",
            "--category",
            "correctness",
            "--desc",
            "round-trip fixture",
            "--author",
            "test",
            "--phase",
            "self",
            "--anchor",
            "crates/mp/src/foo.rs:abc1234:10-20:5-15:0:old",
            "--summary",
            "short summary",
            "--rationale",
            "long rationale",
            "--confidence",
            "high",
            "--tags",
            "rust,review,m124",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Fetch the milestone and confirm every flag persisted.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "findings",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1, "expected exactly one finding");
    let f = &findings[0];

    assert_eq!(f["phase"], "self", "phase not persisted");
    assert_eq!(f["summary"], "short summary", "summary not persisted");
    assert_eq!(f["rationale"], "long rationale", "rationale not persisted");
    assert_eq!(f["confidence"], "high", "confidence not persisted");

    let tags: Vec<String> = f["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tags, vec!["rust", "review", "m124"], "tags not persisted");

    let anchor = &f["anchor"];
    assert_eq!(anchor["path"], "crates/mp/src/foo.rs");
    assert_eq!(anchor["commit"], "abc1234");
    assert_eq!(anchor["hunk_index"], 0);
    assert_eq!(anchor["side"], "old");
    assert_eq!(anchor["new_range"]["start_line"], 10);
    assert_eq!(anchor["new_range"]["end_line"], 20);
    assert_eq!(anchor["old_range"]["start_line"], 5);
    assert_eq!(anchor["old_range"]["end_line"], 15);
}

#[test]
fn thread_entry_on_open_finding_does_not_clear_gate() {
    // M101 AC-07 (regression pinned): adding a thread entry on an open
    // finding does NOT change transition gating. Gating is on
    // finding.status, not thread length.
    use mp_model::MilestoneFile;

    let env = TestEnv::new();
    let id = create_milestone(&env, "thread-gating");

    // Force lifecycle=done via the test-only path (no CLI exposes this
    // today; M100 design note: this is one of the gaps a future
    // milestone should address — separation of `done` as a stable
    // checkpoint from `complete` as the terminal state).
    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&id))
        .map(|e| e.path())
        .unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: MilestoneFile = serde_json::from_str(&raw).unwrap();
    m.milestone.lifecycle = "done".to_string();
    m.milestone.execution_status = "done".to_string();
    m.milestone.spec_status = "verified".to_string();
    m.milestone.priority = "normal".to_string();
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).unwrap()),
    )
    .unwrap();

    // File a self-phase finding → auto-enter remediation (per R1).
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "thread-gating seed",
            "--author",
            "test",
            "--phase",
            "self",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());

    // Lifecycle should be remediation now.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    let lc: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["lifecycle"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(lc, "remediation", "setup precondition");

    // Now: with the finding still open (no resolve), attempt to
    // complete the milestone. The gate must STILL fire even though we
    // conceptually "have a thread" on the open finding. (We can't
    // actually add a thread entry through the CLI today — Finding model
    // has the thread field but no CLI surface for it — so we use a
    // dummy thread entry via direct file edit.)
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: MilestoneFile = serde_json::from_str(&raw).unwrap();
    m.findings[0].thread.push(mp_model::FindingThreadEntry {
        author: "test".to_string(),
        at: "2026-07-06T12:00:00Z".to_string(),
        body: "thread entry that should NOT clear the gate".to_string(),
    });
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).unwrap()),
    )
    .unwrap();

    // Complete must still bail.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "should be blocked by thread-only change",
        ],
    );
    assert!(
        !out.status.success(),
        "complete must still bail — gate is on finding.status, not thread.len()"
    );
}
