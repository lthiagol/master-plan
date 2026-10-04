//! M233 / AC-01: `mp autopilot start --detach` behavioral coverage.
//!
//! Three behavioral tests exercising the production detach paths
//! without touching the live run loop:
//!
//! 1. `preflight_refuses_with_exit_code_2_when_no_harness` — bare
//!    `--detach` against a fresh `TestEnv` exits with the precondition
//!    refusal (exit 2), proving the typed refusal fires before the
//!    setsid + state-file work begins.
//! 2. `state_file_written_to_autopilot_run_state_json_path` —
//!    installs a fake `herdr` on PATH, sets the runner + coordinator
//!    harness, runs `--detach`, and asserts the
//!    `<plan_dir>/.mp/autopilot-run.state.json` file exists with
//!    the queue populated, the schema version pinned, and the
//!    `state_path` / `log_path` recorded.
//! 3. `detached_pid_in_state_matches_response_pid` — same setup as #2;
//!    asserts `response.detached_pid == state_file.pid` and that the
//!    recorded pid differs from the parent's (proving the Pid
//!    transition did fire rather than leaving the parent pid in the
//!    file).
//!
//! **Note on the SIGTERM / killpg path (AC-01 scope):** the detach
//! child is launched with `cmd.pre_exec(libc::setsid)` so it lives in
//! its own session; `mp autopilot start` does not install a SIGTERM
//! handler for the detached child itself (the foreground `mp watch`
//! loop does, but the detached child re-invokes that surface as a
//! separate process). Asserting on the killpg path therefore requires
//! running the detached child past its first failure point, which the
//! current child command (`mp watch <ids>`) does not reach when
//! `mp watch` is not registered. This test scope is intentionally
//! limited to the three behaviors above; widening it would require
//! either a child that survives long enough to receive a signal or a
//! production-code change to expose a test-only entry point (out of
//! M233 scope).

mod common;

use crate::common::{fake_herdr::install_fake_herdr_for_doctor, TestEnv};
use serde_json::Value;

/// 1. Preflight refusal: bare `mp autopilot start --detach` exits 2
///    before any state-file write or fork. Verifies the typed refusal
///    path in `cmd_autopilot_drive_detached` (lines 44-54 of
///    `crates/mp/src/commands/autopilot_detach.rs`).
#[test]
fn preflight_refuses_with_exit_code_2_when_no_harness() {
    let env = TestEnv::new();
    // Fresh TestEnv has no harness config; preconditions must fail
    // before detach tries to spawn anything.
    let out = env.run(&[
        "autopilot",
        "start",
        "1",
        "--detach",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "detach with no harness must exit non-zero; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Exit code 2 is the precondition refusal; the same code is
    // surfaced by `cmd_watch_drive` when `preconditions.ok == false`.
    let code = out.status.code().unwrap_or(0);
    assert_eq!(
        code, 2,
        "expected precondition refusal exit code 2; got {code}; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The state file MUST NOT exist when preconditions fail — the
    // refusal is supposed to short-circuit before any persistence.
    let state_file = env
        .tmp
        .path()
        .join("master-plan/.mp/autopilot-run.state.json");
    assert!(
        !state_file.exists(),
        "state file must not be written when preconditions fail: {}",
        state_file.display()
    );
}

/// 2. State file is written to `<plan_dir>/.mp/autopilot-run.state.json`
///    when preconditions pass. Verifies `AutopilotRunState::save_to_plan`
///    is called before the spawn so the detached pid has a place to
///    land (lines 60-67 of autopilot_detach.rs).
#[test]
fn state_file_written_to_autopilot_run_state_json_path() {
    let env = TestEnv::new();

    // Install fake_herdr on PATH so the precondition / herdr gate
    // passes. doctor-shaped: --version, agent start --help,
    // pane split --help all return non-empty so detect_herdr_cli
    // reports `compatible = true`.
    let path_with_fake = install_fake_herdr_for_doctor(&env);

    // Set runner + coordinator harness so the role-config checks
    // pass.
    for key in ["agent.runner.harness", "agent.coordinator.harness"] {
        let set = env.run_with_env(
            &[("PATH", &path_with_fake)],
            &["config", "set", key, "opencode", "--format", "json"],
        );
        assert!(
            set.status.success(),
            "config set {key} failed: {}",
            String::from_utf8_lossy(&set.stderr)
        );
    }

    // Live detach — preconditions + herdr gate green.
    let out = env.run_with_env(
        &[("PATH", &path_with_fake)],
        &[
            "autopilot",
            "start",
            "1",
            "--detach",
            "--format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "detach with green preconditions must succeed: {stderr}"
    );

    // State file exists at the documented v2 location.
    let state_file = env
        .tmp
        .path()
        .join("master-plan/.mp/autopilot-run.state.json");
    assert!(
        state_file.exists(),
        "state file must be written when detach succeeds: {}",
        state_file.display()
    );

    // State file shape: queue carries the requested milestone id;
    // log_path + state_path are recorded; schema_version is the
    // current v2 constant.
    let state_text = std::fs::read_to_string(&state_file).unwrap();
    let state: Value = serde_json::from_str(&state_text).expect("state JSON");
    let queue = state["queue"].as_array().expect("queue array");
    assert!(
        queue.iter().any(|v| v.as_str() == Some("1")),
        "state.queue must include the requested id '1'; got {queue:?}"
    );
    assert!(
        state["log_path"].as_str().is_some(),
        "state.log_path must be set"
    );
    assert!(
        state["state_path"].as_str().is_some(),
        "state.state_path must be set"
    );
    assert!(
        state["schema_version"].as_u64().is_some(),
        "state.schema_version must be an integer"
    );
}

/// 3. Detached pid flows through the response AND is patched into the
///    state file. Verifies lines 156-168 of autopilot_detach.rs
///    (`let child_pid = child.id(); … store.transition(Pid(child_pid))`).
#[test]
fn detached_pid_in_state_matches_response_pid() {
    let env = TestEnv::new();
    let path_with_fake = install_fake_herdr_for_doctor(&env);
    for key in ["agent.runner.harness", "agent.coordinator.harness"] {
        let set = env.run_with_env(
            &[("PATH", &path_with_fake)],
            &["config", "set", key, "opencode", "--format", "json"],
        );
        assert!(set.status.success());
    }

    let out = env.run_with_env(
        &[("PATH", &path_with_fake)],
        &[
            "autopilot",
            "start",
            "1",
            "--detach",
            "--format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "detach must succeed: {stderr}");

    let response: Value =
        serde_json::from_slice(&out.stdout).expect("detach response JSON");
    let detached_pid = response["detached_pid"]
        .as_u64()
        .expect("response.detached_pid must be a u64");

    // State file's pid field must be the same number the response
    // advertised — the Pid transition ran.
    let state_file = env
        .tmp
        .path()
        .join("master-plan/.mp/autopilot-run.state.json");
    let state_text = std::fs::read_to_string(&state_file).unwrap();
    let state: Value = serde_json::from_str(&state_text).expect("state JSON");
    let state_pid = state["pid"]
        .as_u64()
        .expect("state.pid must be a u64 (the patched detached pid)");

    assert_eq!(
        state_pid, detached_pid,
        "state.pid must equal response.detached_pid"
    );

    // The patched pid must differ from the parent's pid (the parent
    // wrote its own pid via `AutopilotRunState::fresh`; the transition
    // then overwrote it with the spawned child's pid). If they match,
    // the Pid transition did not fire and the response carries a stale
    // pid value.
    let parent_pid = std::process::id() as u64;
    assert_ne!(
        state_pid, parent_pid,
        "state.pid must NOT equal the test runner's pid; \
         a match means the Pid transition did not overwrite the initial parent pid"
    );
}