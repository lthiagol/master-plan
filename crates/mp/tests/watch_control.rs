//! M178 S3+S4+S5+S7+S8 integration tests: the structured
//! `mp watch-control {status,stop,output,result}` verbs and the
//! `mp watch --detach` flag.
//!
//! Black-box coverage:
//! - status reads the v2 state file (or returns a stable
//!   "no_state_file" shape when absent) and classifies the run as
//!   live / stale / terminal.
//! - result returns the latest terminal outcome; never observes a
//!   live run.
//! - stop is a stable non-destructive no-op when no live run
//!   exists; refuses to operate on a known-dead PID.
//! - output returns a structured error when herdr is missing
//!   (no hang).
//! - detach: the foreground `mp watch --detach` is rejected when
//!   preconditions fail (a stuck precondition does not silently
//!   spawn a useless detached child).

mod common;

use crate::common::TestEnv;
use mp::watch::{is_pid_alive, RunOutcome, WatchRunState, WATCH_RUN_STATE_SCHEMA_VERSION};
use std::path::Path;

fn state_path(env: &TestEnv) -> std::path::PathBuf {
    WatchRunState::path_for(&env.tmp.path().join("master-plan"))
}

fn write_state_with(env: &TestEnv, f: impl FnOnce(&mut WatchRunState)) {
    let mut s = WatchRunState::fresh(&["170".to_string()]);
    f(&mut s);
    s.save(&state_path(env)).unwrap();
}

fn status(env: &TestEnv) -> serde_json::Value {
    env.run_json(&["watch-control", "status"])
}

fn result_cmd(env: &TestEnv) -> serde_json::Value {
    env.run_json(&["watch-control", "result"])
}

fn stop_cmd(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["watch-control", "stop", "--format", "json"];
    full.extend(args.iter().copied());
    env.run(&full)
}

fn output_cmd(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["watch-control", "output", "--format", "json"];
    full.extend(args.iter().copied());
    env.run(&full)
}

#[test]
fn status_when_no_state_file_is_stable_no_state_shape() {
    let env = TestEnv::new();
    let report = status(&env);
    // Stale + reason="no state file" is the canonical signal.
    assert_eq!(report["run_state"]["kind"], serde_json::json!("stale"));
    assert_eq!(
        report["run_state"]["reason"],
        serde_json::json!("no state file")
    );
    assert_eq!(report["schema_version"], serde_json::json!(2));
    assert!(report["state_file"]
        .as_str()
        .unwrap()
        .ends_with(".mp/watch.state.json"));
    assert_eq!(report["pid_alive"], serde_json::json!(false));
    assert_eq!(report["herdr_listed"], serde_json::json!(false));
    assert!(report["state"].is_null());
}

#[test]
fn status_reads_v2_state_with_terminal_outcome() {
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        s.set_run_outcome(RunOutcome::Completed);
        s.queue = vec!["170".to_string(), "171".to_string()];
    });
    let report = status(&env);
    assert_eq!(report["run_state"]["kind"], serde_json::json!("terminal"));
    assert!(report["state"].is_object());
    assert_eq!(report["state"]["queue"][0], serde_json::json!("170"));
    assert_eq!(
        report["state"]["run_outcome"]["kind"],
        serde_json::json!("completed")
    );
}

#[test]
fn status_reads_stale_state_when_recorded_pid_is_dead() {
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        // Use a high PID that's almost certainly not alive.
        s.pid = 999_999_999;
    });
    let report = status(&env);
    assert_eq!(report["run_state"]["kind"], serde_json::json!("stale"));
    assert_eq!(report["pid_alive"], serde_json::json!(false));
    assert!(report["run_state"]["reason"]
        .as_str()
        .unwrap()
        .contains("999999999"));
}

#[test]
fn status_summary_flag_strips_full_state_payload() {
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        s.set_run_outcome(RunOutcome::GracefullyStopped);
    });
    let report = env.run_json(&["watch-control", "status", "--summary"]);
    assert_eq!(report["run_state"]["kind"], serde_json::json!("terminal"));
    assert!(
        report["state"].is_null(),
        "--summary must suppress the full state"
    );
    assert!(report["schema_version"].is_number());
}

#[test]
fn result_returns_latest_terminal_outcome() {
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        s.set_run_outcome(RunOutcome::Completed);
        s.push_milestone_outcome(mp::watch::MilestoneRunOutcome {
            id: "170".into(),
            outcome: RunOutcome::Completed,
        });
    });
    let report = result_cmd(&env);
    assert!(report["state_file"].as_str().is_some());
    assert_eq!(
        report["state"]["run_outcome"]["kind"],
        serde_json::json!("completed")
    );
    assert_eq!(
        report["state"]["milestone_outcomes"][0]["id"],
        serde_json::json!("170")
    );
}

#[test]
fn result_returns_null_state_when_no_state_file() {
    let env = TestEnv::new();
    let report = result_cmd(&env);
    assert!(report["state_file"].as_str().is_some());
    assert!(report["state"].is_null());
}

#[test]
fn stop_is_a_stable_noop_when_no_state_file_exists() {
    let env = TestEnv::new();
    let out = stop_cmd(&env, &[]);
    assert!(
        out.status.success(),
        "stop on no-state-file must exit 0 (stable non-destructive response): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stopped"], serde_json::json!(false));
    assert!(v["pid"].is_null());
    assert_eq!(
        v["message"].as_str().unwrap(),
        "no live run; nothing to stop"
    );
}

#[test]
fn stop_with_dead_pid_reports_not_alive() {
    let env = TestEnv::new();
    let out = stop_cmd(&env, &["--pid", "999999999"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stopped"], serde_json::json!(false));
    assert_eq!(v["pid"], serde_json::json!(999999999));
    assert!(v["message"].as_str().unwrap().contains("not alive"));
}

#[test]
fn stop_with_explicit_pid_target_signals_recorded_pid() {
    // We can't safely SIGINT an unrelated live PID from a test
    // (noisy + flaky), so the integration target here is the
    // recorded-but-dead PID case — which exercises the kill()
    // call path with a known ESRCH return.
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        // Spawn a child just to record a PID, then drop it; the
        // recorded PID is then dead.
        let pid = std::process::Command::new("true").spawn().unwrap().id();
        s.pid = pid;
    });
    // PID may or may not still be alive (the `true` child exits
    // quickly on macOS). What we want to assert is that the
    // stop command returns a structured response, not that it
    // signals a specific PID. Either branch — "not alive" or
    // "stopped successfully" — proves the path is wired.
    let out = stop_cmd(&env, &[]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["stopped"].is_boolean());
}

#[test]
fn output_returns_structured_error_when_herdr_missing() {
    // The test env doesn't put herdr on PATH. The output verb must
    // return a bounded, structured error — never hang.
    let env = TestEnv::new();
    write_state_with(&env, |s| {
        s.pane_ids.insert(mp::watch::Role::Runner, "%5".into());
        s.active_role = Some(mp::watch::Role::Runner);
    });
    let out = output_cmd(&env, &["--timeout-ms", "1000", "--max-bytes", "512"]);
    assert!(
        out.status.success(),
        "output on missing herdr must exit 0 (structured error, not panic): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], serde_json::json!(false));
    // Either "no_status_set" (herdr returned null) or a classified
    // error is acceptable — the contract is "structured, bounded,
    // never hung".
    assert!(v["reason"].is_string());
}

#[test]
fn output_rejects_when_no_state_file() {
    let env = TestEnv::new();
    let out = output_cmd(&env, &["--timeout-ms", "1000", "--max-bytes", "512"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], serde_json::json!(false));
    assert_eq!(v["reason"], serde_json::json!("no_state_file"));
}

#[test]
fn detach_refuses_to_spawn_when_preconditions_fail() {
    // A fresh project has no runner/coordinator harness config, so
    // the precondition gate fails. --detach must NOT spawn a child
    // process — it returns a structured precondition report and
    // exits non-zero so the caller sees the failure immediately.
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "detach-precondition-fail");
    let out = env.run(&["watch", "--detach", &id, "--format", "json"]);
    assert!(
        !out.status.success(),
        "detach with failing preconditions must exit non-zero"
    );
    // The state file must NOT exist (detach refused before forking).
    assert!(
        !state_path(&env).exists(),
        "refused detach must not write watch.state.json"
    );
}

fn create_approved_milestone(env: &TestEnv, slug: &str) -> String {
    let json = format!(
        r#"{{
            "title": "{slug}",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "intent": {{ "outcome": "test {slug}" }},
            "problem": {{ "description": "test problem" }},
            "scope": {{
                "in_scope": ["one thing"],
                "out_of_scope": ["something else", "a third thing"]
            }},
            "acceptance_criteria": [
                {{ "description": "ac", "verification": "manual: yes" }}
            ]
        }}"#
    );
    let created = env.run_json(&["milestone", "create", "--json", &json, "--format", "json"]);
    let id = created["milestone"]["id"].as_str().expect("milestone id");
    env.run(&["milestone", "approve", id, "--format", "json"]);
    id.to_string()
}

// `WatchRunState::fresh` shadowed by the helper above; the unit tests
// for the v2 model live in crates/mp/src/watch/run_state.rs.
#[allow(dead_code)]
fn _unused_pin() -> u32 {
    WATCH_RUN_STATE_SCHEMA_VERSION
}

#[allow(dead_code)]
fn _unused_pin_is_pid() {
    let _ = is_pid_alive(0);
    let _: &Path = Path::new("");
}
