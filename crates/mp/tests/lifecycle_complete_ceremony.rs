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
// M145 F-02 (external review): W-LC-TERMINAL must NOT fire for a mid-review
// lifecycle (self-reviewed/reviewed) even when the legacy-shape triple is
// present. The prior broader condition fired here, but the warning's advice
// ("run `mp reviews pass --verdict ok`") is unactionable for non-`done`
// lifecycles because the auto-promote only covers `done`. Narrowing the
// trigger keeps the warning honest.
// ---------------------------------------------------------------------------

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
