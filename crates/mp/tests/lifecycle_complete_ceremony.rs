//! M145: lifecycle=complete ceremony — auto-promote on reviews pass
//! and W-LC-TERMINAL validate warning.
//!
//! AC-01: pass --verdict ok on legacy-shape triple promotes done → complete
//! AC-02: pass --verdict changes-needed does NOT promote
//! AC-03: validate surfaces W-LC-TERMINAL on stuck non-terminal
//! AC-04: integration coverage of the four paths

use std::process::Command;

mod common;
use common::TestEnv;

fn mp_bin() -> &'static std::path::Path {
    common::mp_bin()
}

fn workspace_root() -> std::path::PathBuf {
    common::repo_root()
}

fn run_mp(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(env.tmp.path())
        .env("MP_HOME", workspace_root())
        .args(args);
    cmd.output().expect("failed to run mp")
}

/// Create a fresh milestone via `mp milestone create`. Returns the
/// normalized id (e.g. `01`).
fn create_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{
                "description": "{title} works",
                "verification": "manual: M145 test sanity check"
            }}
        ]
    }}"#
    );
    let out = run_mp(
        env,
        &[
            "milestone",
            "create",
            "--json",
            &create_json,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "milestone create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn milestone_file_path(env: &TestEnv, id: &str) -> std::path::PathBuf {
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    for entry in std::fs::read_dir(&plan_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&format!("{id}-")) {
            return entry.path();
        }
    }
    panic!("milestone file not found for id {id}");
}

fn read_milestone(env: &TestEnv, id: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(milestone_file_path(env, id)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// Patch the milestone file to the legacy-shape triple
/// (lifecycle=done, spec_status=verified, execution_status=done) so
/// `mp reviews pass` has something to auto-promote.
fn set_legacy_done(env: &TestEnv, id: &str) {
    let path = milestone_file_path(env, id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("done");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

fn set_legacy_complete(env: &TestEnv, id: &str) {
    let path = milestone_file_path(env, id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("complete");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// AC-01: pass --verdict ok on legacy-shape triple promotes done → complete
// ---------------------------------------------------------------------------

#[test]
fn pass_ok_promotes_done_to_complete() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 promote");
    set_legacy_done(&env, &id);

    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test",
        ],
    );
    assert!(
        out.status.success(),
        "reviews pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let m = read_milestone(&env, &id);
    assert_eq!(m["milestone"]["lifecycle"], "complete");
    assert!(
        m["milestone"]["lifecycle_at"].is_string()
            && !m["milestone"]["lifecycle_at"].as_str().unwrap().is_empty(),
        "lifecycle_at should be stamped when missing"
    );
}

#[test]
fn pass_ok_is_idempotent_on_complete() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 idempotent");
    set_legacy_complete(&env, &id);
    // Pin a known lifecycle_at so we can assert it's preserved.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle_at"] = serde_json::json!("2026-06-01T00:00:00Z");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test",
        ],
    );
    assert!(out.status.success());
    let m = read_milestone(&env, &id);
    assert_eq!(m["milestone"]["lifecycle"], "complete");
    assert_eq!(
        m["milestone"]["lifecycle_at"], "2026-06-01T00:00:00Z",
        "existing lifecycle_at must be preserved on idempotent pass"
    );
}

#[test]
fn pass_ok_preserves_lifecycle_at_when_already_set() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 preserve ts");
    set_legacy_done(&env, &id);
    // Pre-pin a lifecycle_at; the auto-promote should NOT overwrite it.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle_at"] = serde_json::json!("2026-05-15T12:34:56Z");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test",
        ],
    );
    assert!(out.status.success());
    let m = read_milestone(&env, &id);
    assert_eq!(m["milestone"]["lifecycle"], "complete");
    assert_eq!(
        m["milestone"]["lifecycle_at"], "2026-05-15T12:34:56Z",
        "existing lifecycle_at must not be overwritten"
    );
}

// ---------------------------------------------------------------------------
// AC-02: pass --verdict changes-needed does NOT promote
// ---------------------------------------------------------------------------

#[test]
fn pass_changes_needed_does_not_promote() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 changes-needed");
    set_legacy_done(&env, &id);

    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "changes-needed",
            "--reviewer",
            "test",
        ],
    );
    assert!(out.status.success());
    let m = read_milestone(&env, &id);
    assert_eq!(
        m["milestone"]["lifecycle"], "done",
        "changes-needed verdict must NOT promote"
    );
}

// ---------------------------------------------------------------------------
// AC-03 + AC-04: validate surfaces W-LC-TERMINAL on stuck non-terminal
// ---------------------------------------------------------------------------

#[test]
fn validate_warns_on_legacy_shape_triple() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 warn");
    set_legacy_done(&env, &id);

    let out = run_mp(&env, &["validate", "--format", "json"]);
    // `mp validate` returns non-zero when warnings are present; we only
    // care that the JSON shape carries the W-LC-TERMINAL warning.
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("validate must emit JSON; stderr was not JSON");
    let warnings = report["warnings"].as_array().expect("warnings array");
    let lc_terminal = warnings
        .iter()
        .find(|w| w["code"] == "W-LC-TERMINAL")
        .unwrap_or_else(|| {
            panic!(
                "expected W-LC-TERMINAL warning, got: {}",
                serde_json::to_string_pretty(&report).unwrap()
            )
        });
    assert!(lc_terminal["message"].as_str().unwrap().contains("done"));
    assert_eq!(lc_terminal["milestone"].as_str().unwrap(), id);
}

#[test]
fn validate_silent_for_healthy_complete_milestone() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 complete");
    set_legacy_complete(&env, &id);

    let out = run_mp(&env, &["validate", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    let lc_terminal: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-TERMINAL" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        lc_terminal.is_empty(),
        "complete milestone should NOT trip W-LC-TERMINAL; got: {lc_terminal:?}"
    );
}

/// M166 ext-review F-09: W-LC-TERMINAL must also fire on `lifecycle=complete +
/// execution_status=<non-terminal>`. Pre-M166 the surface was silent on this
/// regression (the `mp milestone set-status <id> blocked ; <id> planned` path
/// lands a complete milestone at execution_status='planned' with 0 warnings).
/// Pin the F-09 widening so a future tightening can't silently re-narrow it.
#[test]
fn validate_warns_on_complete_milestone_with_regressed_execution_status() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M166 set-status regression");
    set_legacy_complete(&env, &id);
    // Hand-write the regressed state. `set_execution_status` (the
    // F-03 fix's surface) now refuses to land this state from a terminal
    // milestone; we reach the regressed state via hand-edit so the
    // validator's detection is the only thing under test.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["execution_status"] = serde_json::json!("planned");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = run_mp(&env, &["validate", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    let lc_terminal = warnings
        .iter()
        .find(|w| w["code"] == "W-LC-TERMINAL" && w["milestone"].as_str().unwrap_or("") == id)
        .unwrap_or_else(|| {
            panic!(
                "complete milestone with execution_status='planned' MUST trip W-LC-TERMINAL; got: {}",
                serde_json::to_string_pretty(&report).unwrap()
            )
        });
    let msg = lc_terminal["message"].as_str().unwrap();
    assert!(
        msg.contains("complete") && msg.contains("planned"),
        "warning must describe the complete<->planned regression; got: {msg}"
    );
}

#[test]
fn validate_silent_for_in_progress_milestone() {
    let env = TestEnv::new();
    // Newly-created milestone is lifecycle=draft / exec=in-progress-or-empty,
    // spec=draft. The legacy triple is NOT present, so W-LC-TERMINAL must
    // NOT fire (this is the healthy in-flight case).
    let id = create_milestone(&env, "M145 in progress");

    let out = run_mp(&env, &["validate", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    let lc_terminal: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-TERMINAL" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        lc_terminal.is_empty(),
        "in-progress milestone should NOT trip W-LC-TERMINAL; got: {lc_terminal:?}"
    );
}

// ---------------------------------------------------------------------------
// M202: AC-19 stage-8/9/10/11 wiring. The complete/review/remediate flow
// drives the auto-advance graph for stages 7-10; AC-19 pins each
// transition's effect on `flow_stages`.
// ---------------------------------------------------------------------------

#[test]
fn complete_marks_external_review_in_progress() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 stage-8 in-progress");

    // Promote to in-progress and then complete (with --skip-review so
    // non-track milestones reach terminal complete in tests).
    let approve = run_mp(&env, &["milestone", "approve", &id]);
    assert!(approve.status.success());
    let start = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(start.status.success());
    let complete = run_mp(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "M202 stage-8 pin",
            "--skip-review",
        ],
    );
    assert!(complete.status.success());

    let m = read_milestone(&env, &id);
    let flow = m["milestone"]["flow_stages"].as_object().expect("flow_stages map");
    // After complete: execute + self-review + complete all done;
    // external-review sits in_progress (the review queue).
    assert_eq!(flow["execute"]["status"], "done");
    assert_eq!(flow["self-review"]["status"], "done");
    assert_eq!(flow["complete"]["status"], "done");
    assert_eq!(
        flow["external-review"]["status"],
        "in_progress",
        "complete must land external-review at in_progress (AC-19)"
    );
}

#[test]
fn reviews_pass_marks_external_review_done() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 reviews pass external done");
    set_legacy_done(&env, &id);

    // The reviews pass --verdict ok path: external-review is currently
    // pending (the milestone was patched directly to lifecycle=done
    // without ever firing Complete). The hook must close it.
    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test",
        ],
    );
    assert!(
        out.status.success(),
        "reviews pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let m = read_milestone(&env, &id);
    let flow = m["milestone"]["flow_stages"].as_object().expect("flow_stages map");
    assert_eq!(
        flow["external-review"]["status"],
        "done",
        "reviews pass --verdict ok must close external-review (AC-19)"
    );
    assert!(
        flow["external-review"]["at"].is_string()
            && !flow["external-review"]["at"].as_str().unwrap().is_empty(),
        "external-review.at must be set to a non-empty RFC3339 timestamp"
    );
    // Re-review must stay pending — this was a first-time pass, not a
    // post-remediation pass.
    let re_review = flow.get("re-review");
    assert!(
        re_review.is_none() || re_review.unwrap()["status"] != "done",
        "re-review must stay pending on a first-time review pass; got: {re_review:?}"
    );
}

#[test]
fn enter_remediation_marks_external_review_done() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 enter remediation");
    // Promote to complete via the canonical path (apply_transition
    // Complete sets external-review to in_progress).
    let _ = run_mp(&env, &["milestone", "approve", &id]);
    let _ = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    let complete = run_mp(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "M202 enter remediation pin",
            "--skip-review",
        ],
    );
    assert!(complete.status.success());

    // File an external-phase finding on the complete milestone. The
    // add_finding_with_phase path auto-enters remediation when an
    // external finding lands on a `complete` milestone (per the
    // reviews.rs auto-remediation contract).
    let finding = run_mp(
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
            "M202 remediation entry pin",
            "--phase",
            "external",
            "--author",
            "test",
        ],
    );
    assert!(
        finding.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&finding.stderr)
    );

    let m = read_milestone(&env, &id);
    assert_eq!(m["milestone"]["lifecycle"], "remediation");
    let flow = m["milestone"]["flow_stages"].as_object().expect("flow_stages map");
    // EnterRemediation: external-review closes (done), remediate opens
    // (in_progress).
    assert_eq!(
        flow["external-review"]["status"],
        "done",
        "EnterRemediation must close external-review (AC-19)"
    );
    assert_eq!(
        flow["remediate"]["status"],
        "in_progress",
        "EnterRemediation must open remediate (S3 contract)"
    );
}

#[test]
fn reviews_pass_after_remediation_closes_re_review() {
    // M202 S4.1: when remediate is already done and a reviews pass fires,
    // both external-review AND re-review close. This is the post-
    // remediation second-pass path; the first-pass path is covered by
    // reviews_pass_marks_external_review_done.
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 re-review done");
    // Pre-seed flow_stages with remediate=done so the second-pass hook
    // fires when reviews pass runs.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("done");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
    let mut flow = serde_json::Map::new();
    flow.insert(
        "external-review".into(),
        serde_json::json!({"status": "in_progress", "at": "2026-09-01T00:00:00Z"}),
    );
    flow.insert(
        "remediate".into(),
        serde_json::json!({"status": "done", "at": "2026-09-02T00:00:00Z"}),
    );
    flow.insert(
        "re-review".into(),
        serde_json::json!({"status": "in_progress", "at": "2026-09-03T00:00:00Z"}),
    );
    m["milestone"]["flow_stages"] = serde_json::Value::Object(flow);
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = run_mp(
        &env,
        &[
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test",
        ],
    );
    assert!(out.status.success());

    let m = read_milestone(&env, &id);
    let flow = m["milestone"]["flow_stages"].as_object().expect("flow_stages map");
    assert_eq!(flow["external-review"]["status"], "done");
    assert_eq!(
        flow["re-review"]["status"],
        "done",
        "post-remediation pass must close re-review too (S4.1)"
    );
}

// ---------------------------------------------------------------------------
// M202 AC-10: post-complete document-done stage hook. Any post-completion
// activity on a `complete` milestone auto-closes `flow_stages.document`:
//   * `mp note add --milestone-id <id>` flips document=done (S9).
//   * `mp reviews finding resolve <id>` flips document=done (S10).
// Both hooks are idempotent — re-running on an already-done document is
// a no-op. Hand-off stays explicit-only regardless (AC-11).
// ---------------------------------------------------------------------------

fn promote_to_complete(env: &TestEnv, id: &str) {
    let _ = run_mp(env, &["milestone", "approve", id]);
    let _ = run_mp(env, &["milestone", "set-status", id, "in-progress"]);
    let complete = run_mp(
        env,
        &[
            "milestone",
            "complete",
            id,
            "--evidence",
            "M202 AC-10 fixture",
            "--skip-review",
        ],
    );
    assert!(complete.status.success());
}

#[test]
fn note_add_after_complete_marks_document_done() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 note-add hook");
    promote_to_complete(&env, &id);

    // Pre-condition: document is pending (or absent).
    let before = read_milestone(&env, &id);
    let before_flow = before["milestone"]["flow_stages"].as_object();
    assert!(
        before_flow.is_none()
            || !before_flow.unwrap().contains_key("document")
            || before_flow.unwrap()["document"]["status"] != "done",
        "document must start pending before the note add fires"
    );

    // Add a note tied to this milestone via the new --milestone-id flag.
    let out = run_mp(
        &env,
        &[
            "note",
            "add",
            "--title",
            "M202 post-complete note",
            "--body",
            "S9 hook pin",
            "--milestone-id",
            &id,
        ],
    );
    assert!(
        out.status.success(),
        "note add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = read_milestone(&env, &id);
    let flow = after["milestone"]["flow_stages"]
        .as_object()
        .expect("flow_stages present after note add");
    assert_eq!(
        flow["document"]["status"], "done",
        "AC-10: note add post-complete must flip flow_stages.document=done"
    );
    assert!(
        flow["document"]["at"].is_string()
            && !flow["document"]["at"].as_str().unwrap().is_empty(),
        "document.at must be set"
    );
    // Idempotency: a second note add must NOT advance the at timestamp.
    let first_at = flow["document"]["at"].as_str().unwrap().to_string();
    let second = run_mp(
        &env,
        &[
            "note",
            "add",
            "--title",
            "M202 second note",
            "--body",
            "Idempotency pin",
            "--milestone-id",
            &id,
        ],
    );
    assert!(second.status.success());
    let after2 = read_milestone(&env, &id);
    let flow2 = after2["milestone"]["flow_stages"].as_object().unwrap();
    assert_eq!(
        flow2["document"]["at"].as_str().unwrap(),
        first_at,
        "idempotent note add must preserve the original at timestamp"
    );
    assert_eq!(flow2["document"]["status"], "done");
}

#[test]
fn reviews_finding_resolve_after_complete_marks_document_done() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M202 finding-resolve hook");
    promote_to_complete(&env, &id);

    // Add a finding on the complete milestone (no auto-remediation
    // because there are no open findings at completion time).
    let added = run_mp(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "low",
            "--category",
            "nit",
            "--desc",
            "M202 S10 fixture finding",
            "--author",
            "test",
        ],
    );
    assert!(
        added.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&added.stderr)
    );

    // Pre-condition: document is pending.
    let before = read_milestone(&env, &id);
    let before_flow = before["milestone"]["flow_stages"].as_object();
    assert!(
        before_flow.is_none()
            || !before_flow.unwrap().contains_key("document")
            || before_flow.unwrap()["document"]["status"] != "done",
        "document must start pending before resolve fires"
    );

    // Resolve the finding — S10 hook fires.
    let resolved = run_mp(
        &env,
        &[
            "reviews",
            "finding",
            "resolve",
            &id,
            "F-01",
            "--commit",
            "M202-S10-pin",
        ],
    );
    assert!(
        resolved.status.success(),
        "finding resolve failed: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );

    let after = read_milestone(&env, &id);
    let flow = after["milestone"]["flow_stages"]
        .as_object()
        .expect("flow_stages present after resolve");
    assert_eq!(
        flow["document"]["status"], "done",
        "AC-10: finding resolve post-complete must flip flow_stages.document=done"
    );
}

#[test]
fn validate_silent_for_mid_review_lifecycle_even_with_legacy_triple() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "M145 mid-review");
    // Force the legacy-shape triple but with lifecycle=self-reviewed
    // (the in-review-loop state). W-LC-TERMINAL must NOT fire — the
    // milestone is in active review, not stuck-done.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("self-reviewed");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = run_mp(&env, &["validate", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    let lc_terminal: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-TERMINAL" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        lc_terminal.is_empty(),
        "mid-review lifecycle (self-reviewed) must NOT trip W-LC-TERMINAL even with the legacy triple; \
         the auto-promote advice would be unactionable. Got: {lc_terminal:?}"
    );
}
