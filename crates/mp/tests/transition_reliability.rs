//! M148: lifecycle transition reliability.
//!
//! AC-02: all steps done → `mp milestone complete` → lifecycle=complete
//! AC-03/04: W-LC-STUCK-EXEC when steps done but lifecycle still in-progress
//! AC-03: reviews pass --verdict ok promotes done → complete (M145 path)

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

fn create_ready_with_step(env: &TestEnv, title: &str) -> String {
    create_ready_with_steps(env, title, 1)
}

/// Create an approved + decomposed milestone with `count` steps
/// numbered S1..Sn. Each step uses the same placeholder action so
/// callers can flip them to done/skipped/pending without touching
/// each step individually.
fn create_ready_with_steps(env: &TestEnv, title: &str, count: usize) -> String {
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
                "verification": "manual: M148 transition reliability"
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
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = run_mp(env, &["milestone", "approve", &id]);
    assert!(
        out.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run_mp(env, &["milestone", "decompose", &id]);
    assert!(
        out.status.success(),
        "decompose failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for _ in 0..count {
        let out = run_mp(
            env,
            &[
                "milestone",
                "step",
                "add",
                &id,
                "--wp",
                "WP1",
                "--action",
                "do the work",
                "--done-when",
                "done",
                "--tests",
                "manual",
            ],
        );
        assert!(
            out.status.success(),
            "step add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    id
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

fn validate_warnings(env: &TestEnv) -> Vec<serde_json::Value> {
    let out = run_mp(env, &["validate", "--format", "json"]);
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("validate must emit JSON");
    report["warnings"].as_array().cloned().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// AC-02: all steps done → complete → lifecycle=complete
// ---------------------------------------------------------------------------

#[test]
fn all_steps_done_complete_reaches_lifecycle_complete() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 complete path");

    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let m = read_milestone(&env, &id);
    assert_eq!(m["milestone"]["lifecycle"], "in-progress");
    assert_eq!(m["milestone"]["execution_status"], "in-progress");

    let out = run_mp(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "M148 AC-02: complete after all steps done",
            "--skip-verify",
            // M196: the review gate. To reach terminal `complete` on a
            // non-track milestone with no recorded review, the test must
            // explicitly bypass the gate via `--skip-review` (which
            // records `[skip-review]` as debt in evidence). The test
            // wants to assert the lifecycle routing, not the review gate.
            "--skip-review",
        ],
    );
    assert!(
        out.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let m = read_milestone(&env, &id);
    assert_eq!(
        m["milestone"]["lifecycle"], "complete",
        "complete must set lifecycle=complete (not self-reviewed)"
    );
    assert_eq!(m["milestone"]["execution_status"], "done");
    assert_eq!(m["milestone"]["spec_status"], "verified");
}

// ---------------------------------------------------------------------------
// AC-04: W-LC-STUCK-EXEC when steps closed but lifecycle still in-progress
// ---------------------------------------------------------------------------

#[test]
fn validate_warns_steps_done_lifecycle_in_progress() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 stuck exec");

    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(out.status.success());

    let warnings = validate_warnings(&env);
    let stuck = warnings
        .iter()
        .find(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .unwrap_or_else(|| {
            panic!(
                "expected W-LC-STUCK-EXEC for {id}, got: {}",
                serde_json::to_string_pretty(&warnings).unwrap()
            )
        });
    let msg = stuck["message"].as_str().unwrap();
    assert!(
        msg.contains("in-progress") && msg.contains("complete"),
        "warning should name stuck state and complete command: {msg}"
    );
}

#[test]
fn validate_silent_stuck_exec_after_complete() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 no stuck after complete");

    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(out.status.success());
    let out = run_mp(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "clears stuck",
            "--skip-verify",
        ],
    );
    assert!(out.status.success());

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        stuck.is_empty(),
        "complete must clear W-LC-STUCK-EXEC; got {stuck:?}"
    );
}

#[test]
fn validate_silent_stuck_exec_while_steps_pending() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 steps pending");

    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    // S1 still pending — not stuck.

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        stuck.is_empty(),
        "pending steps must not trip W-LC-STUCK-EXEC; got {stuck:?}"
    );
}

// ---------------------------------------------------------------------------
// M148 ext-review F-03: W-LC-STUCK-EXEC branch coverage
// (cancelled, no-steps, skipped, multi-step) — gaps in the prior
// suite which only used a single-step fixture.
// ---------------------------------------------------------------------------

#[test]
fn stuck_exec_silent_on_cancelled_milestone() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 cancelled");
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(out.status.success());
    // Cancel via the milestone JSON shape (no public CLI flips the
    // flag without archiving). The validation pass must skip cancelled
    // milestones per plan.rs:554-555.
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["cancelled"] = serde_json::json!(true);
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        stuck.is_empty(),
        "cancelled milestone must not trip W-LC-STUCK-EXEC; got {stuck:?}"
    );
}

#[test]
fn stuck_exec_silent_when_no_steps() {
    let env = TestEnv::new();
    let id = create_ready_with_steps(&env, "M148 no steps", 0);
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        stuck.is_empty(),
        "milestone with no steps must not trip W-LC-STUCK-EXEC; got {stuck:?}"
    );
}

#[test]
fn stuck_exec_fires_with_skipped_step() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 skipped counts");
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(
        &env,
        &["milestone", "step", "set-status", &id, "S1", "skipped"],
    );
    assert!(
        out.status.success(),
        "set-status skipped failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert_eq!(
        stuck.len(),
        1,
        "skipped must count as closed; expected exactly one W-LC-STUCK-EXEC"
    );
}

#[test]
fn stuck_exec_fires_multi_step_all_done() {
    let env = TestEnv::new();
    let id = create_ready_with_steps(&env, "M148 multi done", 3);
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    for step in ["S1", "S2", "S3"] {
        let out = run_mp(&env, &["milestone", "step", "done", &id, step]);
        assert!(out.status.success(), "step {step} done failed");
    }

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert_eq!(
        stuck.len(),
        1,
        "3/3 done steps must trip exactly one W-LC-STUCK-EXEC"
    );
}

#[test]
fn stuck_exec_silent_multi_step_one_pending() {
    let env = TestEnv::new();
    let id = create_ready_with_steps(&env, "M148 multi pending", 3);
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(out.status.success());
    // S2 and S3 stay pending.

    let warnings = validate_warnings(&env);
    let stuck: Vec<_> = warnings
        .iter()
        .filter(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .collect();
    assert!(
        stuck.is_empty(),
        "1 pending step must suppress W-LC-STUCK-EXEC; got {stuck:?}"
    );
}

#[test]
fn stuck_exec_message_branches_on_open_self_findings() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 branch on findings");
    let out = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(out.status.success());
    let out = run_mp(&env, &["milestone", "step", "done", &id, "S1"]);
    assert!(out.status.success());
    // File an open self-phase finding via the on-disk shape (no
    // CLI flag exposed in the runner's lane — the agent uses the
    // mp reviews finding add path; tests bypass it for speed).
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["findings"] = serde_json::json!([
        {
            "id": "F-01",
            "severity": "medium",
            "category": "correctness",
            "description": "stuck-exec branch fixture",
            "status": "open",
            "author": "test",
            "fixed_in": "",
            "created": "2026-07-13",
            "resolved": "",
            "phase": "self"
        }
    ]);
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let warnings = validate_warnings(&env);
    let stuck = warnings
        .iter()
        .find(|w| w["code"] == "W-LC-STUCK-EXEC" && w["milestone"].as_str().unwrap_or("") == id)
        .expect("expected W-LC-STUCK-EXEC to fire");
    let msg = stuck["message"].as_str().unwrap();
    assert!(
        msg.contains("resolve") && msg.contains("self-phase"),
        "message must branch to resolver order when open self findings present: {msg}"
    );
    assert!(
        !msg.contains("\"in-progress\";                  run `mp milestone complete`"),
        "message must NOT use the bare-complete suggestion when findings block it"
    );
}

// ---------------------------------------------------------------------------
// AC-03: findings-resolved path — reviews pass promotes done → complete
// ---------------------------------------------------------------------------

#[test]
fn reviews_pass_ok_promotes_done_to_complete() {
    let env = TestEnv::new();
    let id = create_ready_with_step(&env, "M148 pass promote");

    // Legacy ceremony shape: done + verified + lifecycle=done
    let path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("done");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
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
            "coordinator",
        ],
    );
    assert!(
        out.status.success(),
        "reviews pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let m = read_milestone(&env, &id);
    assert_eq!(
        m["milestone"]["lifecycle"], "complete",
        "stage-10 reviews pass must reach terminal complete"
    );
}

// ---------------------------------------------------------------------------
// Skill contract: complete target is `complete`; stage 10 includes reviews pass
// ---------------------------------------------------------------------------

#[test]
fn skills_document_complete_terminal_and_stage10_pass() {
    let root = workspace_root();
    let runner = std::fs::read_to_string(root.join("templates/skills/mp-runner/SKILL.md")).unwrap();
    assert!(
        runner.contains("lifecycle to **`complete`**")
            || runner.contains("transitions lifecycle to **`complete`**")
            || runner.contains("to **`complete`**"),
        "mp-runner must document complete → complete"
    );
    assert!(
        !runner.contains("transitions lifecycle to\n   `self-reviewed`")
            && !runner.contains("transitions lifecycle to `self-reviewed`"),
        "mp-runner must not claim complete → self-reviewed"
    );
    assert!(
        runner.contains("W-LC-STUCK-EXEC"),
        "mp-runner must mention the stuck-exec warning"
    );

    let coord =
        std::fs::read_to_string(root.join("templates/skills/mp-coordinator/SKILL.md")).unwrap();
    assert!(
        coord.contains("mp reviews pass"),
        "mp-coordinator stage 10 must mandate reviews pass"
    );

    let stages =
        std::fs::read_to_string(root.join("templates/skills/mp-flow/stages.toml")).unwrap();
    assert!(
        stages.contains("complete` (terminal)")
            || stages.contains("to `complete` (terminal)")
            || stages.contains("lifecycle transitions to `complete`"),
        "stages.toml stage 7 must target complete"
    );
    assert!(
        stages.contains("mp reviews pass"),
        "stages.toml stage 10 must include reviews pass"
    );
}
