//! Gate matrix: each gate G1-G10 + G14 fires when its condition holds
//! and clears when the condition is resolved. Tests parse `mp validate --format json`
//! output to detect gate errors in the validation report.

use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;

fn validate_report(env: &TestEnv) -> serde_json::Value {
    let validate = lib_api::run(env, &["validate", "--format", "json"]);
    serde_json::from_slice(&validate.stdout).expect("validate JSON")
}

fn validate_has_gate(env: &TestEnv, gate: &str, milestone_substr: &str) -> bool {
    let report = validate_report(env);
    report["errors"].as_array().unwrap().iter().any(|e| {
        e["code"] == gate
            && e["milestone"]
                .as_str()
                .is_some_and(|m| m.contains(milestone_substr))
    })
}

fn validate_has_no_gate(env: &TestEnv, gate: &str) -> bool {
    let report = validate_report(env);
    !report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["code"] == gate)
}

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let json = format!(
        r#"{{
        "title": "{title}",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "cargo test" }}
        ]
    }}"#
    );
    let out = lib_api::run(
        env,
        &["milestone", "create", "--json", &json, "--format", "json"],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn patch_milestone(env: &TestEnv, id: &str, f: impl FnOnce(&mut String)) {
    let dir = env.tmp.path().join("master-plan/milestones");
    let entry = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().contains(id))
        .expect("find milestone file");
    let mut content = fs::read_to_string(entry.path()).unwrap();
    f(&mut content);
    fs::write(entry.path(), &content).unwrap();
}

/// Load the milestone JSON file for `id`, mutate it via `f`, and write it back
/// as pretty JSON. Used where string-replacement is fragile across the JSON
/// on-disk shape.
fn patch_milestone_json(env: &TestEnv, id: &str, f: impl FnOnce(&mut serde_json::Value)) {
    let dir = env.tmp.path().join("master-plan/milestones");
    let entry = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().contains(id))
        .expect("find milestone file");
    let content = fs::read_to_string(entry.path()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&content).expect("milestone json");
    f(&mut v);
    let json = serde_json::to_string_pretty(&v).unwrap();
    fs::write(entry.path(), format!("{json}\n")).unwrap();
}

// ── G1: in-progress requires spec_status ready or later ──

#[test]
fn g1_fires_when_in_progress_without_ready() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g1-test");
    patch_milestone_json(&env, &id, |v| {
        // M100: simulate a legacy-shape milestone at in-progress without
        // approved. Set the legacy execution_status so validate sees
        // execution_status=in-progress with spec_status=draft.
        v["milestone"]["spec_status"] = serde_json::Value::String("draft".into());
        v["milestone"]["execution_status"] = serde_json::Value::String("in-progress".into());
    });
    assert!(
        validate_has_gate(&env, "G1", &id),
        "G1 should fire when in-progress without ready"
    );
}

#[test]
fn g1_clears_when_ready() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g1-clear");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    assert!(
        validate_has_no_gate(&env, "G1"),
        "G1 should clear when spec is ready"
    );
}

// ── G2: open question unresolved at ready ──

#[test]
fn g2_fires_when_open_question_at_ready() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g2-test");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    // Inject open question via milestone update --json
    let update_json = r#"{
        "title": "g2-test",
        "open_questions": [
            { "id": "Q-01", "question": "Should we do this?", "status": "open", "answer": "" }
        ]
    }"#
    .to_string();
    let upd = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--json",
            &update_json,
            "--format",
            "json",
        ],
    );
    assert!(
        upd.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&upd.stderr)
    );
    assert!(
        validate_has_gate(&env, "G2", &id),
        "G2 should fire when open question at ready"
    );
}

#[test]
fn g2_clears_when_question_resolved() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g2-clear");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    // Inject a resolved open question directly into the milestone JSON.
    let update_json = r#"{
        "open_questions": [
            { "id": "Q-01", "question": "Should we do this?", "status": "resolved", "answer": "" }
        ]
    }"#;
    let upd = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--json",
            update_json,
            "--format",
            "json",
        ],
    );
    assert!(
        upd.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&upd.stderr)
    );
    assert!(
        validate_has_no_gate(&env, "G2"),
        "G2 should clear when question resolved"
    );
}

// ── G3: acceptance criteria required for review+ ──

#[test]
fn g3_fires_when_no_acceptance_criteria() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "g3-test",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": []
    }"#;
    let out = lib_api::run(
        &env,
        &["milestone", "create", "--json", json, "--format", "json"],
    );
    assert!(out.status.success());
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Promote spec_status to "ready" via file patch to bypass the approve gate
    patch_milestone_json(&env, &id, |v| {
        // M100: set lifecycle to "approved" (the new-field equivalent of
        // legacy spec_status="ready").
        v["milestone"]["lifecycle"] = serde_json::Value::String("approved".into());
    });
    assert!(
        validate_has_gate(&env, "G3", &id),
        "G3 should fire when no ACs"
    );
}

// ── G4: out-of-scope minimum from config ──

#[test]
fn g4_approve_respects_config_min_out_of_scope() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "g4-config",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "echo ok" }
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &["milestone", "create", "--json", json, "--format", "json"],
    );
    assert!(out.status.success());
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let config_path = env.tmp.path().join("master-plan/config.json");
    let config = fs::read_to_string(&config_path).unwrap();
    let config = config.replace(
        "\"require_min_out_of_scope\": 2",
        "\"require_min_out_of_scope\": 3",
    );
    fs::write(&config_path, config).unwrap();

    let approve = lib_api::run(&env, &["milestone", "approve", &id, "--format", "json"]);
    assert!(
        !approve.status.success(),
        "approve should fail when out_of_scope count < config minimum"
    );
    let stdout = String::from_utf8_lossy(&approve.stdout);
    assert!(
        stdout.contains("G4"),
        "expected G4 gate failure, got: {stdout}"
    );

    patch_milestone_json(&env, &id, |v| {
        // M100: set lifecycle to "groomed" (the new-field equivalent of
        // legacy spec_status="review").
        v["milestone"]["lifecycle"] = serde_json::Value::String("groomed".into());
    });
    assert!(
        validate_has_gate(&env, "G4", &id),
        "validate should also report G4 at review with config minimum 3"
    );
}

#[test]
fn g4_fires_when_fewer_out_of_scope_items() {
    let env = TestEnv::new();
    // Create with 2 out_of_scope items (passes schema), then patch to 1
    let json = r#"{
        "title": "g4-test",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "echo ok" }
        ]
    }"#;
    let out = lib_api::run(
        &env,
        &["milestone", "create", "--json", json, "--format", "json"],
    );
    assert!(out.status.success());
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Patch the file directly: set spec_status to review and out_of_scope to 1 item.
    // Manipulate the parsed JSON rather than fragile string slicing.
    patch_milestone_json(&env, &id, |v| {
        v["milestone"]["spec_status"] = serde_json::json!("review");
        v["scope"]["out_of_scope"] = serde_json::json!(["A"]);
    });
    assert!(
        validate_has_gate(&env, "G4", &id),
        "G4 should fire when too few out-of-scope"
    );
}

// ── G5: implementation plan before spec ready ──

#[test]
fn g5_fires_when_plan_before_spec_ready() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g5-test");
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "work",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("G5") || !out.status.success(),
        "G5 should block step add: {stderr}"
    );
}

#[test]
fn g5_clears_after_approve() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g5-clear");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    // Add a work package
    let wp_out = lib_api::run(
        &env,
        &[
            "milestone",
            "wp",
            "add",
            &id,
            "--name",
            "Work",
            "--id",
            "WP1",
            "--goal",
            "Do it",
            "--format",
            "json",
        ],
    );
    assert!(
        wp_out.status.success(),
        "wp add failed: {}",
        String::from_utf8_lossy(&wp_out.stderr)
    );
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "work",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "should allow steps after approve: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── G6: AC not passed at verified ──

#[test]
fn g6_fires_when_ac_not_passed_at_verified() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g6-test");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    let out = lib_api::run(&env, &["milestone", "verify", &id, "--format", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("G6") || !out.status.success(),
        "G6 should fire at verify: {stderr}"
    );
}

// ── G7: done requires verified ──

#[test]
fn g7_fires_when_done_without_verified() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g7-test");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    patch_milestone(&env, &id, |c| {
        *c = c.replace(
            "\"execution_status\": \"planned\"",
            "\"execution_status\": \"done\"",
        );
    });
    assert!(
        validate_has_gate(&env, "G7", &id),
        "G7 should fire when done without verified"
    );
}

// ── G8: dependency not done when in-progress ──

#[test]
fn g8_fires_when_dependency_not_done() {
    let env = TestEnv::new();
    let dep_id = create_milestone(&env, "g8-dep");
    let child_id = create_milestone(&env, "g8-child");
    assert!(env
        .run(&["milestone", "approve", &dep_id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "approve", &child_id, "--format", "json"])
        .status
        .success());
    patch_milestone_json(&env, &child_id, |v| {
        // M100: legacy-shape setting so validate's existing logic reads
        // execution_status=in-progress.
        v["milestone"]["spec_status"] = serde_json::Value::String("ready".into());
        v["milestone"]["execution_status"] = serde_json::Value::String("in-progress".into());
        v["milestone"]["depends_on"] =
            serde_json::Value::Array(vec![serde_json::Value::String(dep_id.clone())]);
    });
    assert!(
        validate_has_gate(&env, "G8", &child_id),
        "G8 should fire when dep not done"
    );
}

// ── G14: pending approval request ──

#[test]
fn g14_fires_when_approval_request_pending() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g14-test");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    let ann = lib_api::run(
        &env,
        &[
            "annotation",
            "create",
            &format!("M{id}"),
            "approval-request",
            "Approve?",
            "tester",
            "--format",
            "json",
        ],
    );
    assert!(
        ann.status.success(),
        "annotation create failed: {}",
        String::from_utf8_lossy(&ann.stderr)
    );
    assert!(
        validate_has_gate(&env, "G14", &id),
        "G14 should fire when approval request pending"
    );
}

#[test]
fn g14_clears_when_approval_resolved() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "g14-clear");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    let ann = lib_api::run(
        &env,
        &[
            "annotation",
            "create",
            &format!("M{id}"),
            "approval-request",
            "Approve?",
            "tester",
            "--format",
            "json",
        ],
    );
    assert!(ann.status.success(), "annotation create failed");
    let ann_id: String = serde_json::from_slice::<serde_json::Value>(&ann.stdout).unwrap()
        ["annotation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(env
        .run(&["annotation", "resolve", &ann_id, "--format", "json"])
        .status
        .success());
    assert!(
        validate_has_no_gate(&env, "G14"),
        "G14 should clear after approval resolved"
    );
}
