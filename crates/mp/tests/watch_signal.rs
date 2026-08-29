//! M152 S4 / AC-04: SIGINT / SIGTERM graceful shutdown.
//!
//! `mp watch` installs a signal handler for SIGINT and SIGTERM
//! that flips an atomic the drive loop polls between iterations.
//! When the run trips on the shutdown flag, the cli layer
//! performs cleanup before exiting:
//! 1. Flush `.mp/watch.state.json` so a subsequent
//!    `mp watch --resume` can re-attach to the panes the run
//!    owned.
//! 2. Record a flash note on the in-flight milestone via
//!    `mp reviews comment add` so the next operator sees
//!    "this was a graceful shutdown, not a crash".
//!
//! The exit code is 0 — a SIGINT is not a failure, it's the
//! user's intentional stop signal.

mod common;

use crate::common::TestEnv;
use mp::watch::{
    install_signal_handlers, perform_graceful_shutdown, request_shutdown, shutdown_requested,
    write_shutdown_state_for_test, PaneState, Role, WatchState,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Fake herdr that hangs on the first lifecycle poll, holding the
/// watch run in `wait_for_lifecycle` long enough for the test
/// process to send SIGINT. The exact hang duration does not
/// matter — the drive loop polls the shutdown flag at iteration
/// boundaries (between state transitions), so the test just
/// needs the run to take long enough to send the signal.
fn install_hanging_herdr(dir: &Path) -> PathBuf {
    let bin = dir.join("herdr");
    let body = r#"#!/bin/sh
# M197 followup: answer `agent start --help` and `pane split --help`
# so the herdr_cli_shape precondition (M197 WP3) passes — the
# precondition gate fires BEFORE the watch loop starts, so without
# these handlers the test never reaches the SIGINT path it was
# written to pin.
case "$1:$2:$3" in
  agent:start:--help)
    cat <<'HELP'
Usage: herdr agent start <NAME> --kind <KIND> --pane <ID>

Options:
  --kind <KIND>  Harness kind
  --pane <ID>    Existing pane id
HELP
    ;;
  pane:split:--help)
    echo "Usage: herdr pane split [OPTIONS]"
    ;;
esac
case "$2" in
  list) echo '{"agents":[]}' ;;
  start) echo '{"pane_id":"%N","status":"started"}' ;;
  send) exit 0 ;;
  send-keys) exit 0 ;;
  wait)
    # The `read_agent_status` path passes `--timeout 0` and
    # expects an immediate response. The drive loop calls
    # `read_lifecycle_via_mp` (a `mp show milestone` subprocess)
    # separately, then sleeps `poll_interval_ms` between polls.
    # The shutdown flag is observed at iteration boundaries,
    # so the run never has to wait on this `wait` call to
    # complete — the cycle returns immediately, the loop
    # re-polls, and we see the shutdown within one
    # poll interval.
    echo '{"status":"working"}'
    exit 1
    ;;
  read) echo "" ;;
  status) echo '{"status":"working"}' ;;
  *) echo ok ;;
esac
"#;
    fs::write(&bin, body).unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    bin
}

fn path_with(dir: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, dir.to_path_buf());
    std::env::join_paths(parts)
        .expect("joined PATH")
        .to_string_lossy()
        .into_owned()
}

fn seed_approved_milestone(env: &TestEnv, title: &str) -> String {
    let json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "{title}" }},
        "problem": {{ "description": "p" }},
        "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }},
        "acceptance_criteria": [
            {{ "description": "ac", "verification": "manual: yes" }}
        ]
    }}"#
    );
    let created = env.run_json(&["milestone", "create", "--json", &json, "--format", "json"]);
    let id = created["milestone"]["id"].as_str().unwrap().to_string();
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id, "--format", "json"]);
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);
    id
}

fn state_path(env: &TestEnv) -> PathBuf {
    WatchState::path_for(&env.tmp.path().join("master-plan"))
}

#[test]
fn signal_handlers_install_lazily_and_flip_shutdown_flag() {
    // Pin the in-process side of AC-04: install_signal_handlers
    // must be callable, and request_shutdown() must flip the
    // atomic so a subsequent shutdown_requested() returns true.
    // The cross-process path (real SIGINT) lives in the next
    // test.
    install_signal_handlers();
    assert!(!shutdown_requested());
    request_shutdown();
    assert!(shutdown_requested());
    // Reset so subsequent tests in this binary start clean.
    mp::watch::clear_shutdown_flag();
}

#[test]
fn perform_graceful_shutdown_writes_state_and_adds_flash_note() {
    // Black-box: the cleanup routine called by `cmd_watch_drive`
    // does both halves of AC-04 — writes `.mp/watch.state.json`
    // AND records a review comment (flash note) on the in-flight
    // milestone. The state file is absent before, present after.
    let env = TestEnv::new();
    install_signal_handlers();
    mp::watch::clear_shutdown_flag();

    let id = seed_approved_milestone(&env, "shutdown-cleanup");
    let path = state_path(&env);
    assert!(
        !path.exists(),
        "no state file should exist before a shutdown"
    );

    // Build a state file content matching what a running watch
    // would have. `perform_graceful_shutdown` flushes whatever
    // state the caller hands it — the test pins that contract.
    let ctx_dir = env.tmp.path().join("master-plan");
    let mut state = WatchState::fresh(std::slice::from_ref(&id));
    state.upsert_pane(PaneState {
        role: Role::Runner,
        label: "role-runner-1".into(),
        pane_id: "%5".into(),
        spawned_at: "t".into(),
        last_status: None,
    });
    state.upsert_milestone(mp::watch::MilestoneState {
        id: id.clone(),
        last_lifecycle: "in-progress".into(),
        target_lifecycle: "self-reviewed".into(),
        last_action_at: "t".into(),
    });

    let plan_ctx = mp::paths::PlanContext {
        plan_dir: ctx_dir.clone(),
        project_root: env.tmp.path().to_path_buf(),
    };
    perform_graceful_shutdown(&plan_ctx, &state, Some(&id), Some("in-progress"), None).unwrap();

    // Step 1: state file exists.
    assert!(path.is_file(), "state file must be flushed");
    let loaded = WatchState::load_from(&path).unwrap().expect("loads");
    assert_eq!(loaded.milestone(&id).unwrap().last_lifecycle, "in-progress");
    assert_eq!(loaded.pane_for(Role::Runner).unwrap().pane_id, "%5");

    // Step 2: flash note recorded as a review comment.
    let comments_out = env.run_json(&["reviews", "comment", "list", &id, "--format", "json"]);
    let comments = comments_out["comments"].as_array().unwrap();
    let flash = comments
        .iter()
        .find(|c| c["author"] == "mp watch")
        .expect("mp watch flash note must be recorded");
    let body = flash["body"].as_str().unwrap();
    assert!(
        body.contains("graceful shutdown"),
        "body must surface the shutdown reason: {body}"
    );
    assert!(body.contains(&id), "body must name the milestone: {body}");
    assert!(
        body.contains("in-progress"),
        "body must surface the last lifecycle: {body}"
    );
}

#[test]
fn perform_graceful_shutdown_is_resilient_when_milestone_load_fails() {
    // Cleanup must not crash when the in-flight milestone cannot
    // be re-read (e.g. a SIGINT during milestone setup, before
    // the milestone id was recorded). Calling with no active
    // milestone + no last_lifecycle is the contract: skip the
    // flash note, still flush the state file.
    let env = TestEnv::new();
    install_signal_handlers();
    mp::watch::clear_shutdown_flag();

    let path = state_path(&env);
    let state = WatchState::fresh(&[]);
    let plan_ctx = mp::paths::PlanContext {
        plan_dir: env.tmp.path().join("master-plan"),
        project_root: env.tmp.path().to_path_buf(),
    };
    perform_graceful_shutdown(&plan_ctx, &state, None, None, None).unwrap();
    assert!(path.is_file(), "state file still gets flushed");
}

#[test]
fn write_shutdown_state_for_test_seeds_state_file() {
    // Convenience helper used by the integration test fixture in
    // earlier versions of M152 S4. Kept as a regression pin —
    // removing it forces a manual state-file write that is more
    // brittle than this helper.
    let env = TestEnv::new();
    let dir = env.tmp.path().join("master-plan");
    let plan_ctx = mp::paths::PlanContext {
        plan_dir: dir.clone(),
        project_root: env.tmp.path().to_path_buf(),
    };
    let path = write_shutdown_state_for_test(&plan_ctx, "M-test", "in-progress").unwrap();
    assert!(path.is_file());
    let loaded = WatchState::load_from(&path).unwrap().expect("loads");
    assert_eq!(
        loaded.milestone("M-test").unwrap().last_lifecycle,
        "in-progress"
    );
}

#[test]
fn real_sigint_during_watch_run_exits_zero_and_flushes_state() {
    // Drive the headline M152 S4 / AC-04 contract: a real
    // SIGINT during `mp watch` causes exit code 0 (not the
    // usual failure 2) AND flushes `.mp/watch.state.json` so
    // `--resume` can re-attach.
    //
    // Strategy:
    // 1. Seed an approved milestone + config (harness must be set
    //    or the precondition gate fails before the run starts).
    // 2. Install a fake herdr that hangs on `wait` — holds the
    //    run inside the lifecycle poll just long enough for the
    //    test to send SIGINT.
    // 3. Spawn `mp watch <id>` and capture the child pid.
    // 4. After a short delay (poll cadence), send SIGINT.
    // 5. Wait for the child to exit. Assert exit status is 0.
    // 6. Read the on-disk state file. Assert it carries the
    //    in-flight milestone id and a pane entry.
    //
    // The flash note (review comment) recording is exercised in
    // `perform_graceful_shutdown_writes_state_and_adds_flash_note`
    // — this test is the cross-process check on the integration.
    let env = TestEnv::new();
    install_signal_handlers();
    mp::watch::clear_shutdown_flag();

    let bin_dir = env.tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    install_hanging_herdr(&bin_dir);

    let id = seed_approved_milestone(&env, "signal-cleanup");

    let mp_bin = common::mp_bin().to_path_buf();
    let mut cmd = std::process::Command::new(&mp_bin);
    cmd.current_dir(env.tmp.path())
        .env("PATH", path_with(&bin_dir))
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .args([
            "watch",
            &id,
            "--format",
            "json",
            "--stall-timeout-ms",
            "10000",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .expect("mp watch must be spawnable in the test env");
    let pid = child.id();

    // Take the stdout/stderr pipes; we read them after the child
    // exits rather than spawning threads (the per-thread pipe
    // ownership complicates panic-path join calls — the simpler
    // approach below reads buffered output post-exit, which is
    // sufficient for our 20-second timeout-driven panic anyway).
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    // Wait long enough for the run to start (preconditions pass,
    // first herdr interaction happens, the fake wait hangs).
    std::thread::sleep(Duration::from_millis(1000));

    let r = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    assert_eq!(r, 0, "kill must succeed against the spawned child");

    // Wait for the child to exit (bounded). Poll with a hard cap
    // so a hung run does not stall the suite for the full CI
    // timeout window.
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(20);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut stdout_buf = String::new();
                    let mut stderr_buf = String::new();
                    if let Some(s) = stdout_pipe.as_mut() {
                        let _ = std::io::Read::read_to_string(s, &mut stdout_buf);
                    }
                    if let Some(s) = stderr_pipe.as_mut() {
                        let _ = std::io::Read::read_to_string(s, &mut stderr_buf);
                    }
                    panic!(
                        "mp watch did not exit within {timeout:?} of SIGINT. \
                         stdout: {}, stderr: {}",
                        stdout_buf, stderr_buf,
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => panic!("try_wait failed: {e}"),
        }
    };

    // Drain the pipes so they don't block the child or hold open
    // file descriptors.
    if let Some(s) = stdout_pipe.as_mut() {
        let mut buf = String::new();
        let _ = std::io::Read::read_to_string(s, &mut buf);
        eprintln!("DEBUG: mp watch stdout = {buf}");
    }
    if let Some(s) = stderr_pipe.as_mut() {
        let mut buf = String::new();
        let _ = std::io::Read::read_to_string(s, &mut buf);
        eprintln!("DEBUG: mp watch stderr = {buf}");
    }

    assert!(
        exit.success(),
        "graceful shutdown must exit 0 (signal-driven exit); got {exit:?}"
    );

    // The state file must be flushed on disk. Pre-M152 there was
    // no state file at all; the SIGINT cleanup path is the only
    // thing that creates one in this scenario.
    let path = state_path(&env);
    assert!(
        path.is_file(),
        "graceful shutdown must leave .mp/watch.state.json flushed"
    );
    let loaded = WatchState::load_from(&path).unwrap().expect("loads");
    let tracked = loaded
        .milestone(&id)
        .expect("in-flight milestone must appear in flushed state");
    assert!(
        !tracked.last_lifecycle.is_empty(),
        "flushed state must carry a last_lifecycle value"
    );

    // Reset the global shutdown flag before the next test so
    // other modules see a clean slate.
    mp::watch::clear_shutdown_flag();
}
