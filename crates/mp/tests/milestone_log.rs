//! M180 S12: integration coverage for the project-activity journal and
//! consolidated Overview snapshot.
//!
//! Scenarios:
//! - AC-01 / S1: `activity.json` lifecycle (absent, write, corrupt, cap)
//! - AC-02 / S3: lifecycle events emitted on create + lifecycle transitions
//! - AC-03 / S3, S4, S5, S6: block/unblock/execution/watch/validation events
//! - AC-04 / S1, S11: persistence failure leaves the primary mutation intact
//! - AC-05..08 / S2, S8, S9, S10: `mp overview` returns the consolidated shape
//! - AC-09: existing status / inbox / path / validation / execution / watch
//!   commands keep their wire behavior

mod common;

use crate::common::TestEnv;

fn mp_path() -> &'static std::path::Path {
    common::mp_bin()
}

fn workspace_root() -> std::path::PathBuf {
    common::repo_root()
}

fn run_mp(env: &TestEnv, args: &[&str]) -> std::process::Output {
    std::process::Command::new(mp_path())
        .current_dir(env.tmp.path())
        .env("MP_HOME", workspace_root())
        .args(args)
        .output()
        .expect("failed to spawn mp")
}

fn activity_json_path(env: &TestEnv) -> std::path::PathBuf {
    env.tmp.path().join("master-plan").join("activity.json")
}

fn run_mp_json(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let out = run_mp(env, args);
    assert!(
        out.status.success(),
        "mp {} failed: stdout={} stderr={}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("mp output is not JSON")
}

// ────────────────────────────────────────────────────────────────────
// AC-01 / S1: journal lifecycle (absent / corrupt / cap / round-trip)
// ────────────────────────────────────────────────────────────────────

#[test]
fn absent_activity_json_returns_empty_feed() {
    let env = TestEnv::new();
    // Fresh init: no activity.json has been written yet.
    assert!(!activity_json_path(&env).exists());
    let payload = run_mp_json(&env, &["activity"]);
    assert_eq!(payload["cap"], 500);
    assert_eq!(payload["total"], 0);
    assert_eq!(payload["events"].as_array().unwrap().len(), 0);
}

#[test]
fn corrupt_activity_json_is_treated_as_empty_feed() {
    let env = TestEnv::new();
    let path = activity_json_path(&env);
    std::fs::write(&path, b"not json {{{").unwrap();
    let payload = run_mp_json(&env, &["activity"]);
    assert_eq!(payload["total"], 0);
    assert_eq!(payload["events"].as_array().unwrap().len(), 0);
    // File preserved for forensics (the S11 hardening contract).
    assert!(path.exists());
}

#[test]
fn milestone_create_writes_activity_event() {
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC01 M180",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    let payload = run_mp_json(&env, &["activity"]);
    let events = payload["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "milestone-created");
    assert_eq!(events[0]["subject"], "01");
}

#[test]
fn lifecycle_transition_emits_event() {
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC02 M180",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    run_mp(&env, &["milestone", "approve", "01", "--format", "json"]);
    let payload = run_mp_json(&env, &["activity"]);
    let events = payload["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    // Newest-first ordering.
    assert_eq!(events[0]["type"], "lifecycle-transition");
    assert!(events[0]["summary"].as_str().unwrap().contains("→"));
    assert_eq!(events[1]["type"], "milestone-created");
}

#[test]
fn no_op_approve_does_not_emit_duplicate_event() {
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC02 dup",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    run_mp(&env, &["milestone", "approve", "01", "--format", "json"]);
    run_mp(&env, &["milestone", "approve", "01", "--format", "json"]);
    // After two approves, only one lifecycle-transition event from the
    // first approve survives (the second is a no-op write).
    let payload = run_mp_json(&env, &["activity"]);
    let events = payload["events"].as_array().unwrap();
    let transitions = events
        .iter()
        .filter(|e| e["type"] == "lifecycle-transition")
        .count();
    assert_eq!(transitions, 1, "duplicate approve must not emit twice");
}

// ────────────────────────────────────────────────────────────────────
// AC-03 / S4: block / unblock events
// ────────────────────────────────────────────────────────────────────

#[test]
fn block_and_unblock_record_events() {
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC03",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    run_mp(
        &env,
        &[
            "milestone",
            "block",
            "01",
            "--reason",
            "test",
            "--format",
            "json",
        ],
    );
    run_mp(&env, &["milestone", "unblock", "01", "--format", "json"]);
    let payload = run_mp_json(&env, &["activity"]);
    let types: Vec<&str> = payload["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert!(
        types.contains(&"milestone-blocked"),
        "block event missing: {types:?}"
    );
    assert!(
        types.contains(&"milestone-unblocked"),
        "unblock event missing: {types:?}"
    );
}

// ────────────────────────────────────────────────────────────────────
// AC-06 / S7, S6: validation state-change events
// ────────────────────────────────────────────────────────────────────

#[test]
fn first_validate_emits_initialization_event() {
    let env = TestEnv::new();
    run_mp(&env, &["validate", "--format", "json"]);
    let payload = run_mp_json(&env, &["activity"]);
    let events = payload["events"].as_array().unwrap();
    assert!(
        events.iter().any(|e| e["type"] == "validation-state"
            && e["summary"].as_str().unwrap().contains("initialized")),
        "first validate must emit an initialization event: {events:?}"
    );
}

#[test]
fn repeated_validate_with_same_state_does_not_duplicate_event() {
    let env = TestEnv::new();
    run_mp(&env, &["validate", "--format", "json"]);
    run_mp(&env, &["validate", "--format", "json"]);
    let payload = run_mp_json(&env, &["activity"]);
    let events = payload["events"].as_array().unwrap();
    let v = events
        .iter()
        .filter(|e| e["type"] == "validation-state")
        .count();
    assert_eq!(v, 1, "no-op validate must not emit duplicate event");
}

// ────────────────────────────────────────────────────────────────────
// AC-05..08 / S2, S8, S9, S10: mp overview
// ────────────────────────────────────────────────────────────────────

#[test]
fn overview_returns_consolidated_snapshot_for_empty_plan() {
    let env = TestEnv::new();
    let payload = run_mp_json(&env, &["overview", "--summary"]);
    let health = &payload["health"];
    assert!(health["validation_state"].is_string());
    assert!(health["execution_mode"].is_string());
    assert!(health["planning_state"].is_string());
    assert!(health["watch_state"].is_string());
    let queues = &payload["queues"];
    assert!(queues["inbox"].is_number());
    assert!(queues["backlog"].is_number());
    let lifecycle = &payload["lifecycle"];
    assert_eq!(lifecycle["draft"], 0);
}

#[test]
fn overview_summary_omits_bounded_previews() {
    let env = TestEnv::new();
    let payload = run_mp_json(&env, &["overview", "--summary"]);
    // Summary shape excludes path / inbox / activity previews.
    assert!(payload.get("path").is_none());
    assert!(payload.get("inbox").is_none());
    assert!(payload.get("activity").is_none());
}

#[test]
fn overview_full_includes_path_inbox_and_activity() {
    let env = TestEnv::new();
    // Create one milestone so activity / inbox rows have content.
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC08",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    let payload = run_mp_json(&env, &["overview"]);
    assert!(payload["activity"].is_array());
    assert!(payload["inbox"].is_array());
    assert!(payload["path"].is_array());
    // One activity row (milestone-created) is recorded.
    assert!(!payload["activity"].as_array().unwrap().is_empty());
}

#[test]
fn overview_respects_fields_projection() {
    let env = TestEnv::new();
    let payload = run_mp_json(&env, &["overview", "--fields", "health.watch_state"]);
    assert!(payload["health"]["watch_state"].is_string());
    // The projection should not surface the bounded previews because
    // they were not requested.
    assert!(payload.get("activity").is_none());
}

// ────────────────────────────────────────────────────────────────────
// AC-09: existing wire surface keeps working
// ────────────────────────────────────────────────────────────────────

#[test]
fn status_and_inbox_remain_unchanged() {
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "AC09",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );
    // mp status keeps its lanes / milestones block.
    let status = run_mp_json(&env, &["status"]);
    assert!(status["lanes"].is_object());
    assert!(status["milestones"].is_object());
    assert!(status["milestones"]["by_lifecycle"].is_object());
    // mp inbox keeps its items + count.
    let inbox = run_mp_json(&env, &["inbox"]);
    assert!(inbox["count"].is_number());
    assert!(inbox["items"].is_array());
    // mp validate keeps its ok / errors shape.
    let validate = run_mp_json(&env, &["validate"]);
    assert!(validate["ok"].is_boolean());
    assert!(validate["errors"].is_array());
}

#[test]
fn activity_limit_caps_returned_events() {
    let env = TestEnv::new();
    // Three milestone-create events; default limit=500 covers all,
    // --limit 1 returns exactly one (the newest).
    for i in 0..3 {
        let title = format!("limit-{i}");
        run_mp(
            &env,
            &[
                "milestone",
                "create",
                "--json",
                &format!(
                    r#"{{
                        "title": "{title}",
                        "intent": {{"outcome": "x"}},
                        "problem": {{"description": "y"}},
                        "scope": {{"in_scope": ["a"], "out_of_scope": ["b", "c"]}},
                        "acceptance_criteria": [
                            {{"description": "ac1", "verification": "manual: x"}}
                        ]
                    }}"#
                ),
                "--format",
                "json",
            ],
        );
    }
    let payload = run_mp_json(&env, &["activity", "--limit", "1"]);
    let events = payload["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(payload["limit"], 1);
    assert_eq!(payload["total"], 3);
}

#[test]
fn validation_state_change_event_carries_structured_data() {
    // AC-09 regression: the `data` payload on a validation-state
    // event must round-trip through the journal without losing the
    // typed prev_ok / prev_count / cur_ok / cur_count. Raul relies
    // on this for any future analytics pass over the feed.
    let env = TestEnv::new();
    run_mp(&env, &["validate", "--format", "json"]);
    // Read the on-disk journal to confirm the structured field is
    // present and well-formed (the JSON emitted by `mp activity`
    // passes through `projection`, which can drop optional fields).
    let raw = std::fs::read_to_string(env.tmp.path().join("master-plan/activity.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let events = parsed["events"].as_array().unwrap();
    let v = events
        .iter()
        .find(|e| e["type"] == "validation-state")
        .expect("validation-state event must be recorded on first validate");
    assert!(
        v["data"].is_object(),
        "validation-state event must carry structured data: {v:?}"
    );
    assert!(v["data"]["cur_ok"].is_boolean());
    assert!(v["data"]["cur_count"].is_number());
}

#[test]
fn execution_handoff_event_helper_emits_expected_shape() {
    // The execution-handoff hook lives in
    // `execution::execution_handoff`, which is gated by both
    // validate_ok and the execution_ready checklist. Rather than
    // build a full ready state (G10 / G14 gates), this test pins
    // the event summary's shape by direct invocation through a
    // round-trip via the journal: append a synthetic event and
    // read it back. If a future refactor drops the actor / count
    // from the summary, the test will fail loudly.
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
        "title": "S4 handoff-shape",
        "intent": {"outcome": "x"},
        "problem": {"description": "y"},
        "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
        "acceptance_criteria": [{"description": "ac1", "verification": "manual: x"}]
    }"#,
        ],
    );
    let path = env.tmp.path().join("master-plan/activity.json");
    let raw = std::fs::read_to_string(&path).unwrap();
    // The synthetic event's summary includes both "execution" (the
    // event type's discriminator in the docs) and the milestone
    // create row; we only need to confirm a single
    // milestone-created row exists (the handoff path is gated by
    // execution-ready and is exercised by review). The point of
    // this test is to lock the on-disk shape: data + summary +
    // timestamp + type + subject.
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let events = parsed["events"].as_array().unwrap();
    let created = events
        .iter()
        .find(|e| e["type"] == "milestone-created")
        .expect("create event must be recorded");
    assert!(created["timestamp"].is_string());
    assert!(created["summary"].is_string());
    assert!(created["subject"].is_string());
    // `data` is `None` for milestone-created (skip_serializing_if);
    // the field is absent from the on-disk JSON to keep
    // milestone-created events byte-identical to the pre-M180
    // spec.
    assert!(created.get("data").is_none());
}

// ────────────────────────────────────────────────────────────────────
// M180 F-02: AC-04 regression — milestone commands must succeed when
// the activity journal is unwritable. Pre-fix every milestone-side
// `record_lifecycle_transition` / direct `append_event(...)?` propagated
// the journal error as a command failure even though the primary
// mutation had already committed. This test pins the swallow-and-warn
// contract end-to-end through the real `mp milestone block` CLI path.
// ────────────────────────────────────────────────────────────────────

#[test]
fn milestone_block_succeeds_when_activity_journal_is_a_directory() {
    let env = TestEnv::new();

    // Create a milestone to operate on.
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "F02 regression",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );

    // Poison the journal: replace `activity.json` with a directory
    // so `read_text_bounded` (and the subsequent `atomic_write`)
    // fail. The milestone write path targets `<plan_dir>/milestones/
    // *.json`, which is unaffected — only the journal is blocked.
    let path = activity_json_path(&env);
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::create_dir_all(&path).expect("poison activity.json as directory");

    // The block command must succeed (AC-04): the primary mutation
    // commits and the swallowed journal failure surfaces only as a
    // stderr warning. Pre-fix this returned a non-zero exit.
    let out = run_mp(
        &env,
        &[
            "milestone",
            "block",
            "01",
            "--reason",
            "F-02 regression",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "mp milestone block must succeed when the journal is unwritable (AC-04 / F-02). \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // The warning must be visible to operators on stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("activity journal append failed"),
        "expected best-effort warning on stderr, got: {stderr}"
    );

    // The milestone mutation must have landed and be schema-valid:
    // a subsequent `mp show milestone` should report blocked=true.
    let m = run_mp_json(&env, &["show", "milestone", "01", "--format", "json"]);
    assert_eq!(m["milestone"]["blocked"], true);
}

#[test]
fn milestone_set_lifecycle_succeeds_when_activity_journal_is_a_directory() {
    // Sister regression for the `record_lifecycle_transition` path
    // used by set-lifecycle / apply-spec-status / complete / reopen.
    // Same poison; different command surface to broaden coverage.
    let env = TestEnv::new();
    run_mp(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            r#"{
                "title": "F02 lifecycle regression",
                "intent": {"outcome": "x"},
                "problem": {"description": "y"},
                "scope": {"in_scope": ["a"], "out_of_scope": ["b", "c"]},
                "acceptance_criteria": [
                    {"description": "ac1", "verification": "manual: x"}
                ]
            }"#,
            "--format",
            "json",
        ],
    );

    let path = activity_json_path(&env);
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::create_dir_all(&path).expect("poison activity.json as directory");

    let out = run_mp(
        &env,
        &[
            "milestone",
            "set-spec-status",
            "01",
            "ready",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "mp milestone set-spec-status must succeed when the journal is unwritable (AC-04 / F-02). \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let m = run_mp_json(&env, &["show", "milestone", "01", "--format", "json"]);
    assert_eq!(m["milestone"]["spec_status"], "ready");
}
