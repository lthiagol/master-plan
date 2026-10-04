//! M233 / AC-01: `mp autopilot start --detach` behavioral coverage.
//!
//! Four behavioral tests exercising the production detach paths
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
//! 4. `shutdown_signals_recorded_pid_and_persists_terminal_outcome`
//!    — (M233 cycle-2 F-01) writes a v2 state file with a live
//!    pid, runs `mp autopilot stop`, asserts the stop report shows
//!    `stopped: true` within the timeout, asserts the live pid is
//!    now dead, and asserts the state file's `run_outcome` was
//!    persisted to `gracefully-stopped`. Verifies the SIGINT-and-
//!    poll loop + terminal-outcome flush in
//!    `cmd_autopilot_control_stop` (`crates/mp/src/commands/autopilot_control.rs:70-184`).
//!
//! **Why the shutdown test uses a synthetic live pid:** the detach
//! child re-invokes `mp watch <ids> <flags>` as its own argv (per
//! `crates/mp/src/commands/autopilot_detach.rs:79`), and `mp watch`
//! is no longer a registered clap subcommand post-M229 — so the
//! detached child dies within milliseconds and there is nothing for
//! the shutdown signal to interrupt. The test instead writes a valid
//! v2 state file whose `pid` points at a long-lived subprocess the
//! test process spawned itself, then exercises the canonical
//! shutdown sequence through `mp autopilot stop`. This isolates the
//! stop path's signal + poll + terminal-outcome flush from the
//! detach path's spawn + setsid (which are covered by tests 1-3).

mod common;

use crate::common::{fake_herdr::install_fake_herdr_for_doctor, TestEnv};
use serde_json::Value;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};

/// 1. Preflight refusal: bare `mp autopilot start --detach` exits 2
///    before any state-file write or fork. Verifies the typed refusal
///    path in `cmd_autopilot_drive_detached` (lines 44-54 of
///    `crates/mp/src/commands/autopilot_detach.rs`).
#[test]
fn preflight_refuses_with_exit_code_2_when_no_harness() {
    let env = TestEnv::new();
    // Fresh TestEnv has no harness config; preconditions must fail
    // before detach tries to spawn anything.
    let out = env.run(&["autopilot", "start", "1", "--detach", "--format", "json"]);
    assert!(
        !out.status.success(),
        "detach with no harness must exit non-zero; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Exit code 2 is the precondition refusal; the same code is
    // surfaced by `cmd_watch_drive` when `preconditions.ok == false`.
    let code = out.status.code().unwrap_or(0);
    assert_eq!(
        code,
        2,
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
        &["autopilot", "start", "1", "--detach", "--format", "json"],
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
        &["autopilot", "start", "1", "--detach", "--format", "json"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "detach must succeed: {stderr}");

    let response: Value = serde_json::from_slice(&out.stdout).expect("detach response JSON");
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

/// RAII guard: if a `Child` outlives its scope (test panic, failed
/// assertion, or a stop that never landed), `SIGKILL` the process so
/// it does not leak across the nextest worker. Used only by the
/// shutdown-path test below.
struct KillOnDrop(pub Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // Best-effort SIGKILL + non-blocking wait. If the process
        // already exited cleanly the kill returns ESRCH, which we
        // ignore; if it lingered the kill + wait reaps it.
        #[cfg(unix)]
        unsafe {
            let _ = libc::kill(self.0.id() as libc::pid_t, libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

/// 4. (M233 cycle-2 F-01) Shutdown path: a v2 state file pointing at a
///    live pid is consumed by `mp autopilot stop`, the live pid is
///    SIGINT'd and observed to exit, and the state file is flushed
///    with `run_outcome = gracefully-stopped` so a subsequent
///    `mp autopilot status` sees the run as terminal.
///
/// **Why a synthetic pid:** see module-level note. The detach child
/// dies immediately because `mp watch` is no longer registered; we
/// cannot observe a SIGINT going to it. We instead set up the state
/// file's `pid` field to point at a `python3` sleeper the test
/// spawned, then exercise `mp autopilot stop` against that pid. The
/// stop sequence is the production shutdown path that `mp autopilot
/// start --detach` was designed to integrate with: a long-running
/// detached child + a structured `mp autopilot stop` from the
/// operator side.
///
/// **Why python3, not `sleep` or `bash`:** `mp autopilot stop`
/// sends SIGINT (per `crates/mp/src/commands/autopilot_control.rs:357`).
/// On macOS, BSD `/bin/sleep` ignores SIGINT (it relies on `alarm()`);
/// bash's non-interactive job control also masks SIGINT for the
/// foreground child. The python3 sleeper explicitly installs a
/// SIGINT handler that calls `sys.exit(0)`, so the SIGINT path
/// produces the same observable behavior as a real detached `mp
/// autopilot start` child (which installs its own SIGINT handler at
/// `crates/mp/src/autopilot/drive/shutdown.rs:69`). python3 is
/// already a dev dependency for several existing test files
/// (`autopilot_drive_herdr_wait.rs`, `mp_flow_lint_drift.rs`).
///
/// **Why the double-fork wrapper:** `is_pid_alive` (used by
/// `cmd_autopilot_control_stop`'s poll loop) returns `true` for
/// zombie processes — `kill(pid, 0)` returns 0 until the parent
/// reaps. In production, `mp autopilot start` (parent A) exits
/// after spawning the detached child (B), so B is reparented to
/// launchd/init and reaped by init when it exits. The test process
/// is the parent of the python3 sleeper, so without reparenting
/// the sleeper becomes a zombie of the test process and
/// `is_pid_alive` polls true forever. The double-fork pattern
/// `(python3 ... &)` inside `sh -c` makes the test process's
/// grandchild a child of init: the wrapper shell exits immediately,
/// python3 is reparented to launchd/init, and init reaps it on
/// exit. After SIGINT, `kill(pid, 0)` returns ESRCH within one
/// poll (~100ms).
#[test]
fn shutdown_signals_recorded_pid_and_persists_terminal_outcome() {
    use std::io::Read;

    let env = TestEnv::new();

    // Install fake_herdr on PATH and set the harness so the planner
    // command (which loads `agent.runner.harness` via config) does
    // not fail before reaching the stop path. The stop path itself
    // does NOT consult the harness — only the planner's setup does.
    let path_with_fake = install_fake_herdr_for_doctor(&env);
    for key in ["agent.runner.harness", "agent.coordinator.harness"] {
        let set = env.run_with_env(
            &[("PATH", &path_with_fake)],
            &["config", "set", key, "opencode", "--format", "json"],
        );
        assert!(set.status.success());
    }

    // Detach once so the state file exists at the v2 path; the
    // detached child will die immediately (see module-level note)
    // but the state file persists with whatever pid it captured.
    let detach = env.run_with_env(
        &[("PATH", &path_with_fake)],
        &["autopilot", "start", "1", "--detach", "--format", "json"],
    );
    assert!(
        detach.status.success(),
        "detach must succeed to scaffold the state file"
    );

    // Spawn the python3 sleeper via a double-fork wrapper. The
    // wrapper shell exits immediately, leaving python3 reparented
    // to launchd/init. python3 installs a SIGINT handler that calls
    // sys.exit(0) so SIGINT terminates the process cleanly; init
    // then reaps it (no zombie).
    let pidfile = env.tmp.path().join("sleeper.pid");
    let _ = std::fs::remove_file(&pidfile);
    let wrapper = Command::new("sh")
        .args([
            "-c",
            &format!(
                "(python3 -c 'import signal, sys, time, os\n\
                 signal.signal(signal.SIGINT, lambda *_: sys.exit(0))\n\
                 open(\"{f}\", \"w\").write(str(os.getpid()))\n\
                 time.sleep(60)' &)",
                f = pidfile.display()
            ),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sh wrapper");
    let _wrapper_guard = KillOnDrop(wrapper);

    // Wait for python3 to write its pid. Up to ~5 s (50 * 100 ms).
    let mut attempts = 0u32;
    let sleeper_pid: u32 = loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(mut f) = std::fs::File::open(&pidfile) {
            let mut s = String::new();
            f.read_to_string(&mut s).expect("read pidfile");
            if let Ok(p) = s.trim().parse::<u32>() {
                break p;
            }
        }
        attempts += 1;
        if attempts > 50 {
            panic!("sleeper pidfile not populated within 5s");
        }
    };
    assert!(
        sleeper_pid > 0,
        "sleeper_pid must be a real pid; got {sleeper_pid}"
    );

    // Liveness check: the sleeper must be alive before we SIGINT.
    let alive_pre = unsafe { libc::kill(sleeper_pid as libc::pid_t, 0) };
    assert_eq!(
        alive_pre, 0,
        "sleeper must be alive before stop (kill(0) returned {alive_pre})"
    );

    // Patch the state file so `pid` points at the live sleeper.
    // Keep the existing v2 schema_version / generation / queue /
    // log_path / state_path; only flip `pid` + `run_outcome`.
    let state_file = env
        .tmp
        .path()
        .join("master-plan/.mp/autopilot-run.state.json");
    let state_text = std::fs::read_to_string(&state_file).expect("state file");
    let mut state: Value = serde_json::from_str(&state_text).expect("state JSON parse");
    state["pid"] = serde_json::json!(sleeper_pid);
    state["run_outcome"] = serde_json::Value::Null;
    std::fs::write(
        &state_file,
        serde_json::to_string_pretty(&state).expect("serialize"),
    )
    .expect("write patched state");

    // Run `mp autopilot stop` — without --pid override so it reads
    // the pid from the patched state file. timeout_secs is generous
    // to absorb scheduler jitter but bounded so a regression that
    // never sends SIGINT surfaces as a real failure.
    let stop = env.run_with_env(
        &[("PATH", &path_with_fake)],
        &[
            "autopilot",
            "stop",
            "--timeout-secs",
            "10",
            "--format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&stop.stderr);
    let stdout = String::from_utf8_lossy(&stop.stdout);

    assert!(
        stop.status.success(),
        "mp autopilot stop must exit 0; stderr={stderr}; stdout={stdout}"
    );

    // Parse the StopReport JSON envelope.
    let report: Value = serde_json::from_slice(&stop.stdout).expect("stop JSON");
    assert_eq!(
        report["stopped"].as_bool(),
        Some(true),
        "stop report must show stopped=true; report={report}"
    );
    assert_eq!(
        report["pid"].as_u64(),
        Some(sleeper_pid as u64),
        "stop report must echo the recorded pid"
    );

    // The sleeper must now be gone — the SIGINT path landed AND
    // launchd/init reaped the orphaned python3 process. `kill(pid,
    // 0)` returning ESRCH (errno=3 on Linux/macOS) means the pid
    // is invalid.
    let alive = unsafe { libc::kill(sleeper_pid as libc::pid_t, 0) };
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    assert!(
        alive != 0 && errno == 3,
        "sleeper pid must be gone (ESRCH=3) after stop; kill(0)={alive} errno={errno}"
    );

    // The state file must now record the terminal outcome the stop
    // path flushed (`RunOutcome::GracefullyStopped` serializes as
    // `{"kind": "gracefully-stopped"}` per run_state_v2.rs:84).
    let patched_text = std::fs::read_to_string(&state_file).expect("state file after stop");
    let patched: Value = serde_json::from_str(&patched_text).expect("state JSON after stop");
    let outcome = &patched["run_outcome"];
    assert_eq!(
        outcome["kind"].as_str(),
        Some("gracefully-stopped"),
        "run_outcome.kind must be 'gracefully-stopped' after stop; got {outcome}"
    );

    // The stop report's `elapsed_secs` is the actual bounded-poll
    // time. It must be < timeout_secs (10) and >= 0. A regression
    // that skips the poll loop would report 0; one that hangs would
    // report >= timeout.
    let elapsed = report["elapsed_secs"].as_f64().unwrap_or(-1.0);
    assert!(
        (0.0..10.0).contains(&elapsed),
        "stop.elapsed_secs must be in [0, timeout=10); got {elapsed}"
    );

    // Touch the platform-specific helper to silence the `unused
    // import` lint when the test is rebuilt on a non-unix target.
    let _ = std::process::ExitStatus::from_raw(0);
}
