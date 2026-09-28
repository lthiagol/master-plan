//! M246 WP3 / AC-03: `mp autopilot wait <id>` — block until a
//! milestone reaches the lifecycle its run is waiting for.
//!
//! The polling decision is unit-tested in
//! `autopilot::observe`; this file pins the CLI contract:
//!
//! - exit 0 with `{reached: true, lifecycle}` when the target is met,
//! - exit 1 with `{reached: false, reason, lifecycle}` otherwise,
//! - `reason` is one of `timeout`, `run-stopped`, `run-failed`,
//! - the target comes from the run state's recorded
//!   `target_lifecycle`, and a milestone that reaches `complete`
//!   satisfies the wait regardless.
//!
//! Every case seeds `.mp/autopilot-run.state.json` and a milestone
//! record so the command reads the same on-disk sources a live run
//! would, and uses short `--timeout` values so the suite never waits
//! 30 minutes for a negative.

mod common;

use crate::common::TestEnv;
use mp::autopilot::drive::{AutopilotRunState, LifecycleTarget, PromptStage, RunOutcome};
use serde_json::Value;

/// Create a milestone with the given lifecycle and return its id.
fn seed_milestone(env: &TestEnv, lifecycle: &str) -> String {
    let json = format!(
        r#"{{
            "title": "wait target",
            "lifecycle": "{lifecycle}",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "intent": {{ "outcome": "wait" }},
            "problem": {{ "description": "p" }},
            "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }},
            "acceptance_criteria": [
                {{ "description": "ac", "verification": "manual: yes" }}
            ]
        }}"#
    );
    let created = env.run_json(&["milestone", "create", "--json", &json, "--format", "json"]);
    let id = created["milestone"]["id"].as_str().unwrap().to_string();
    set_lifecycle(env, &id, lifecycle);
    id
}

/// Seed a run state that is actively driving `id` and aiming at
/// `target`. The pid is this test process, so the run reads as live.
fn seed_run_state(env: &TestEnv, id: &str, target: &str) -> AutopilotRunState {
    let mut state = AutopilotRunState::fresh(&[id.to_string()]);
    state.pid = std::process::id();
    state.set_active_milestone(0, id);
    state.set_active_stage(
        PromptStage::Execute,
        LifecycleTarget::from_lifecycle(target).expect("valid lifecycle target"),
    );
    let path = AutopilotRunState::path_for(&env.tmp.path().join("master-plan"));
    state.save(&path).expect("seed run state");
    state
}

fn seed_state_raw(env: &TestEnv, state: &AutopilotRunState) {
    let path = AutopilotRunState::path_for(&env.tmp.path().join("master-plan"));
    state.save(&path).expect("seed run state");
}

fn run_wait(env: &TestEnv, id: &str, timeout: u64) -> std::process::Output {
    env.run(&[
        "autopilot",
        "wait",
        id,
        "--timeout",
        &timeout.to_string(),
        "--format",
        "json",
    ])
}

/// Rewrite a created milestone's `lifecycle` on disk.
///
/// `mp milestone create` always starts a milestone at `draft`; the
/// wait verb is about lifecycles, so the tests put the record
/// directly where the verb reads it from rather than depending on a
/// transition verb that may have its own preconditions.
fn set_lifecycle(env: &TestEnv, id: &str, lifecycle: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    for entry in std::fs::read_dir(&dir).expect("milestones dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read milestone"))
                .expect("parse milestone");
        if raw["milestone"]["id"].as_str() == Some(id) {
            let mut out = raw;
            out["milestone"]["lifecycle"] = Value::String(lifecycle.to_string());
            std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
            return;
        }
    }
    panic!("no milestone file for id {id}");
}

fn body(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "wait must emit JSON: {e}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn wait_exits_zero_when_target_lifecycle_reached() {
    let env = TestEnv::new();
    let id = seed_milestone(&env, "self-reviewed");
    seed_run_state(&env, &id, "self-reviewed");

    let out = run_wait(&env, &id, 5);
    assert!(
        out.status.success(),
        "reaching the target must exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = body(&out);
    assert_eq!(v["reached"], true);
    assert_eq!(v["lifecycle"], "self-reviewed");
    assert_eq!(v["target_lifecycle"], "self-reviewed");
    assert!(v["reason"].is_null(), "a satisfied wait carries no reason");
}

#[test]
fn wait_complete_satisfies_an_earlier_stage_target() {
    // A run recorded `self-reviewed` as the stage target, but the
    // milestone has since gone all the way to `complete`. Waiting for
    // the exact string would hang forever on a milestone that already
    // passed it.
    let env = TestEnv::new();
    let id = seed_milestone(&env, "complete");
    seed_run_state(&env, &id, "self-reviewed");

    let out = run_wait(&env, &id, 5);
    assert!(out.status.success(), "complete must satisfy the wait");
    let v = body(&out);
    assert_eq!(v["reached"], true);
    assert_eq!(v["lifecycle"], "complete");
}

#[test]
fn wait_times_out_with_reason_timeout() {
    let env = TestEnv::new();
    let id = seed_milestone(&env, "in-progress");
    seed_run_state(&env, &id, "self-reviewed");

    let out = run_wait(&env, &id, 1);
    assert_eq!(out.status.code(), Some(1), "a timeout must exit 1");
    let v = body(&out);
    assert_eq!(v["reached"], false);
    assert_eq!(v["reason"], "timeout");
    assert_eq!(v["lifecycle"], "in-progress");
    assert_eq!(v["target_lifecycle"], "self-reviewed");
}

#[test]
fn wait_reports_run_failed_on_a_failed_run() {
    let env = TestEnv::new();
    let id = seed_milestone(&env, "in-progress");
    let mut state = seed_run_state(&env, &id, "self-reviewed");
    state.set_run_outcome(RunOutcome::PartialFailure);
    seed_state_raw(&env, &state);

    let out = run_wait(&env, &id, 30);
    assert_eq!(out.status.code(), Some(1));
    let v = body(&out);
    assert_eq!(v["reached"], false);
    assert_eq!(v["reason"], "run-failed");
}

#[test]
fn wait_reports_run_stopped_when_the_driver_is_gone() {
    // The state file records a terminal-less run whose pid is dead —
    // the driver crashed. Nothing will move the milestone, so the
    // wait must not burn the full timeout.
    let env = TestEnv::new();
    let id = seed_milestone(&env, "in-progress");
    let mut state = seed_run_state(&env, &id, "self-reviewed");
    state.pid = 999_999_999; // almost certainly not alive
    seed_state_raw(&env, &state);

    let out = run_wait(&env, &id, 30);
    assert_eq!(out.status.code(), Some(1));
    let v = body(&out);
    assert_eq!(v["reached"], false);
    assert_eq!(v["reason"], "run-stopped");
    assert_eq!(v["lifecycle"], "in-progress");
}

#[test]
fn wait_reports_run_stopped_with_no_state_file() {
    // No run at all: the operator waited on a milestone nothing is
    // driving.
    let env = TestEnv::new();
    let id = seed_milestone(&env, "in-progress");

    let out = run_wait(&env, &id, 30);
    assert_eq!(out.status.code(), Some(1));
    let v = body(&out);
    assert_eq!(v["reached"], false);
    assert_eq!(v["reason"], "run-stopped");
}

#[test]
fn wait_uses_complete_as_the_target_for_a_milestone_the_run_is_not_driving() {
    // The run is driving a different milestone, so there is no stage
    // target for this one — `complete` is the only success condition.
    // Sitting at `in-progress` must therefore NOT satisfy the wait.
    let env = TestEnv::new();
    let id = seed_milestone(&env, "in-progress");
    let other = seed_milestone(&env, "complete");
    let mut state = seed_run_state(&env, &other, "self-reviewed");
    state.set_active_milestone(0, &other);
    seed_state_raw(&env, &state);

    let out = run_wait(&env, &id, 1);
    assert_eq!(out.status.code(), Some(1));
    let v = body(&out);
    assert_eq!(v["reason"], "timeout");
    assert_eq!(v["target_lifecycle"], "complete");
}
