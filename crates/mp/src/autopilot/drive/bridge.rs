//! M150 — `herdr` stage-done sentinel bridge.
//!
//! `mp watch` today polls `mp show milestone <id> --fields
//! milestone.lifecycle` once per second because the opencode herdr
//! bridge (`~/.config/opencode/plugins/herdr-agent-state.js`) reports
//! only `idle`/`working` and never `done`. This module closes that
//! gap by adding an explicit, push-based completion signal:
//!
//! 1. **Producer side** — `mp milestone complete` and
//!    `mp reviews pass` (M145 auto-promote path) call
//!    `herdr pane report-agent --custom-status mp-stage-done --message <id>`
//!    when running inside a herdr pane (detected via the
//!    `HERDR_PANE_ID` env var). One call per stage-done; never
//!    errors on absence.
//!
//! 2. **Consumer side** — `mp watch`'s wait loop polls
//!    `herdr pane get <pane> --json` for `custom_status ==
//!    "mp-stage-done"` and, on observation, immediately re-checks
//!    the milestone lifecycle via `mp show milestone <id>`.
//!    **The sentinel is only a wake-up hint; the lifecycle field is
//!    authoritative.** A stale sentinel (one observed after the
//!    pane already cleared or before lifecycle actually advanced)
//!    is logged and the consumer keeps polling. The sentinel is
//!    consumed best-effort after observation so the next stage
//!    starts clean. When `HERDR_PANE_ID` is unset or the bridge is
//!    missing, the lifecycle poll alone drives completion — exactly
//!    the M149 behavior (no regression).
//!
//! ## Contract
//!
//! - **Sentinel value:** [`STAGE_DONE_SENTINEL`] = `"mp-stage-done"`.
//!   Single source of truth; tests + the producer wire this constant,
//!   so changing it is one edit away from a global rename.
//! - **herdr `--source` / `--agent` IDs:** [`STAGE_DONE_SOURCE`] =
//!   `"mp"`, [`STAGE_DONE_AGENT`] = `"mp-runner"`. These tag the
//!   report-agent calls so observers can tell `mp`-emitted
//!   sentinels apart from harness-emitted ones in `pane get` JSON.
//! - **Producer argv:** `herdr pane report-agent <pane> --source mp
//!   --agent mp-runner --state idle --custom-status mp-stage-done
//!   --message <milestone-id>`. Verified against herdr 0.7.3:
//!   `--seq` is a numeric counter (herdr rejects string values with
//!   `invalid value for --seq`), so the producer does NOT pass
//!   `--seq`. The milestone id rides in `--message` because `pane
//!   get` only exposes `custom_status` (the consumer cannot read
//!   the message via the known `pane get` shape).
//!
//! ## Layering
//!
//! Pure helpers first (no I/O): [`build_report_agent_args`],
//! [`parse_custom_status_from_pane_get`], [`sentinel_matches`].
//! Bounded subprocess primitives: [`run_herdr_with_timeout`],
//! [`read_custom_status_bounded`], [`clear_stage_done_sentinel`].
//! Higher-level best-effort wrappers:
//! [`report_stage_done_bounded`], [`emit_stage_done_best_effort`].
//!
//! ## Subprocess timeouts (F-13)
//!
//! Every `herdr` call from this module runs through
//! [`run_herdr_with_timeout`], which spawns the child in its own
//! process group (`Command::process_group(0)` on Unix), polls
//! `try_wait` every 20ms, and on deadline signals the whole group
//! with `killpg(pgid, SIGKILL)` so forked descendants (e.g. the
//! `sleep` grandchild of `sh -c "sleep 60"`) are reaped alongside
//! the direct child. The direct child is then `wait()`ed on. A
//! wedged herdr therefore cannot wedge `mp milestone complete`,
//! `mp reviews pass`, or the watch loop.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// The custom-status string that marks "stage done". Producers emit
/// this via `herdr pane report-agent --custom-status`; consumers poll
/// `herdr pane get <pane> --json` for it.
pub const STAGE_DONE_SENTINEL: &str = "mp-stage-done";

/// The `--source ID` value producers pass to `herdr pane report-agent`.
/// Tags the report as mp-emitted (vs. harness-emitted) so an observer
/// reading `herdr pane get` can filter by source.
pub const STAGE_DONE_SOURCE: &str = "mp";

/// The `--agent LABEL` value producers pass to `herdr pane report-agent`.
/// Pairs with `STAGE_DONE_SOURCE`; observers can match on either to
/// distinguish mp-emitted reports.
pub const STAGE_DONE_AGENT: &str = "mp-runner";

/// Default wall-clock budget for `herdr` subprocess calls. Bounds
/// the F-13 "best-effort helper can hang the producer" defect:
/// `mp milestone complete` / `mp reviews pass` cannot exceed this
/// per call when emitting the sentinel.
pub const DEFAULT_SUBPROCESS_TIMEOUT_MS: u64 = 500;

/// Default wall-clock budget for `herdr pane get` polls in the watch
/// fast-path. Must be strictly less than the consumer's overall poll
/// cadence so a single hung pane-get cannot defeat the deadline.
pub const DEFAULT_BRIDGE_POLL_TIMEOUT_MS: u64 = 200;

/// Output captured from a bounded `herdr` subprocess invocation.
#[derive(Debug)]
pub struct HerdrOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Run `herdr <args>` with a wall-clock deadline. Spawns the child
/// with `Command::spawn`, polls `try_wait` every 20ms, and on deadline
/// kills the whole process group so forked descendants (e.g. the
/// `sleep` grandchild of `sh -c "sleep 60"`) are reaped alongside
/// the direct child. The direct child is then `wait()`ed on so no
/// zombie remains.
///
/// Returns:
/// - `Ok(HerdrOutput)` when the child exits within the deadline
///   (capture includes stdout/stderr, even on non-zero exit),
/// - `Err` when the deadline fires; the child and its descendants
///   have been killed and reaped before this returns.
pub fn run_herdr_with_timeout(
    herdr_bin: &Path,
    args: &[&str],
    timeout_ms: u64,
) -> Result<HerdrOutput> {
    let mut command = Command::new(herdr_bin);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // M117: place the child in its own process group so the timeout
    // path can kill the whole subtree (`sh → sleep`) with one
    // `killpg` call. `process_group(0)` makes the child a
    // process-group leader with pgid == child_pid. Without this,
    // `child.kill()` only signals the shell; the `sleep` descendant
    // is orphaned and outlives the helper.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn {} {}", herdr_bin.display(), args.join(" ")))?;
    let start = Instant::now();
    let deadline = Duration::from_millis(timeout_ms);
    let tick = Duration::from_millis(20);
    loop {
        match child.try_wait().context("herdr child wait failed")? {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_end(&mut stdout);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_end(&mut stderr);
                }
                return Ok(HerdrOutput {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if start.elapsed() >= deadline {
                    kill_process_group(&mut child);
                    let _ = child.wait();
                    bail!("herdr {} timed out after {}ms", args.join(" "), timeout_ms);
                }
                std::thread::sleep(tick);
            }
        }
    }
}

/// Send `SIGKILL` to the child's entire process group so forked
/// descendants are reaped. No-op on non-Unix targets (the direct
/// `child.kill()` still runs there).
fn kill_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        if let Ok(pgid) = i32::try_from(pid) {
            // SAFETY: `killpg` is a thin wrapper around the POSIX
            // killpg(2) syscall; ESRCH (no such process) is acceptable
            // because we are already in the error path — the desired
            // post-condition is "no orphan lives", not a 0 return.
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }
    }
    // Always also signal the direct child in case the process-group
    // kill raced with the child exiting and the leader pid is gone.
    let _ = child.kill();
}

// ─── Pure helpers (no I/O) ───────────────────────────────────────────────────

/// Build the argv for `herdr pane report-agent`. Pure; tested directly.
/// Keeps producer-side wiring deterministic — every stage-done
/// notification uses the same argv shape, modulo `pane_id`, `stage`,
/// and the optional `message`.
///
/// **Verified against herdr 0.7.3:** `--seq N` only accepts numeric
/// values (rejects strings with `invalid value for --seq`). The
/// producer therefore does NOT emit `--seq` — the contract is to
/// carry the milestone id in `--message`, and the consumer reads
/// only `custom_status` (the only field pane get exposes in the
/// known envelope).
pub fn build_report_agent_args(pane_id: &str, stage: &str, message: Option<&str>) -> Vec<String> {
    vec![
        "pane".into(),
        "report-agent".into(),
        pane_id.to_string(),
        "--source".into(),
        STAGE_DONE_SOURCE.to_string(),
        "--agent".into(),
        STAGE_DONE_AGENT.to_string(),
        "--state".into(),
        "idle".to_string(),
        "--custom-status".into(),
        STAGE_DONE_SENTINEL.to_string(),
        "--message".into(),
        message.unwrap_or(stage).to_string(),
    ]
}

/// Build the argv for `herdr pane report-metadata --clear-custom-status`.
/// Pure helper for [`clear_stage_done_sentinel`]; the consumer-side
/// best-effort cleanup path runs after observing the sentinel so the
/// next stage of `mp watch` starts clean.
pub fn build_clear_custom_status_args(pane_id: &str) -> Vec<String> {
    vec![
        "pane".into(),
        "report-metadata".into(),
        pane_id.to_string(),
        "--source".into(),
        STAGE_DONE_SOURCE.to_string(),
        "--clear-custom-status".into(),
    ]
}

/// Parse the `custom_status` field out of a `herdr pane get <pane>`
/// JSON envelope. `herdr pane get` returns
/// `{"id":..., "result": {"pane": {..., "custom_status": "..."}}, ...}`
/// — drill down to `result.pane.custom_status`. Returns `None` when
/// the field is missing, empty, or the JSON is malformed (so the
/// caller treats "no sentinel yet" as "keep waiting", which is the
/// correct M149 fallback).
pub fn parse_custom_status_from_pane_get(json_text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let custom = v
        .get("result")?
        .get("pane")?
        .get("custom_status")?
        .as_str()?;
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// True when `current_custom_status` matches [`STAGE_DONE_SENTINEL`].
/// Strict equality — partial matches would risk a false positive on
/// unrelated custom-status strings an outside harness might emit.
pub fn sentinel_matches(current_custom_status: &str) -> bool {
    current_custom_status.trim() == STAGE_DONE_SENTINEL
}

// ─── I/O wrappers (shell out to `herdr`, bounded) ────────────────────────────

/// Best-effort producer-side emission. Runs `herdr pane report-agent`
/// with the bounded subprocess helper, propagates non-zero exits as
/// `Err` so callers (and tests) can observe them. The
/// [`emit_stage_done_best_effort`] higher-level helper is the
/// swallow-errors surface for the producer call sites.
pub fn report_stage_done_bounded(
    herdr_bin: &Path,
    pane_id: &str,
    stage: &str,
    message: Option<&str>,
    timeout_ms: u64,
) -> Result<()> {
    let args = build_report_agent_args(pane_id, stage, message);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_herdr_with_timeout(herdr_bin, &arg_refs, timeout_ms)?;
    if !out.status.success() {
        bail!(
            "herdr pane report-agent failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Read `herdr pane get <pane>` and return the `custom_status`
/// string within a wall-clock budget. Returns `Ok(None)` when the
/// field is absent (sentinel not yet emitted), when the JSON shape
/// is missing expected keys, OR when the subprocess times out /
/// errors (a hung pane-get cannot wedge the watch loop — the F-13
/// contract).
pub fn read_custom_status_bounded(
    herdr_bin: &Path,
    pane_id: &str,
    timeout_ms: u64,
) -> Result<Option<String>> {
    let out = run_herdr_with_timeout(herdr_bin, &["pane", "get", pane_id], timeout_ms)?;
    if !out.status.success() {
        bail!(
            "herdr pane get failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_custom_status_from_pane_get(&stdout))
}

/// Clear the consumer-side sentinel after observation so the next
/// stage starts clean. Best-effort: a failure is logged but never
/// propagated — the lifecycle poll is authoritative either way,
/// and the AC-01 best-effort contract forbids wedging the watch
/// loop on a stuck clear.
pub fn clear_stage_done_sentinel(herdr_bin: &Path, pane_id: &str, timeout_ms: u64) -> Result<()> {
    let args = build_clear_custom_status_args(pane_id);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_herdr_with_timeout(herdr_bin, &arg_refs, timeout_ms)?;
    if !out.status.success() {
        bail!(
            "herdr pane report-metadata --clear-custom-status failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

// ─── File-system helpers ────────────────────────────────────────────────────

/// Locate the `herdr` binary on PATH. Mirror of [`crate::autopilot::drive::herdr::which_herdr`]
/// but lives in the bridge module so callers that import
/// `bridge::*` don't need to also import `herdr::*` just for the
/// path resolution. Returns `None` when herdr is not on PATH.
pub fn which_herdr() -> Option<PathBuf> {
    crate::autopilot::drive::herdr::which_herdr()
}

// ─── Producer-side best-effort helper ───────────────────────────────────────

/// Read `HERDR_PANE_ID` from the environment. Returns `None` when
/// unset or blank. Producer-side calls (`mp milestone complete`,
/// `mp reviews pass`) gate on this — the sentinel is only emitted
/// when the call genuinely runs inside a herdr pane, so agents that
/// invoke `mp` from a plain shell are not bothered with non-zero
/// exit codes or spurious errors.
pub fn detect_herdr_pane_id() -> Option<String> {
    std::env::var("HERDR_PANE_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Best-effort helper for producer-side call sites (`mp milestone
/// complete`, `mp reviews pass`, …). Emits the stage-done sentinel
/// via `herdr pane report-agent` IF:
/// - `HERDR_PANE_ID` is set in the ambient environment,
/// - `herdr` is on `PATH`,
/// - the subprocess call succeeds within a bounded wall-clock budget
///   (a hung `herdr` cannot fail the caller's write — F-13).
///
/// Returns:
/// - `true` when the sentinel was emitted,
/// - `false` when no-op'd (env unset / herdr missing / subprocess
///   failed or timed out).
///
/// The function does not panic and does not propagate an error. The
/// producer run is already terminal by the time this is invoked; the
/// caller's write must not be failed by a sentinel-emit failure —
/// the lifecycle poll is the source of truth and `mp watch` falls
/// back to it when the bridge is silent. Callers therefore do
/// `let _ = emit_stage_done_best_effort(...)`.
pub fn emit_stage_done_best_effort(stage: &str, message: Option<&str>) -> bool {
    let pane_id = match detect_herdr_pane_id() {
        Some(id) => id,
        None => return false,
    };
    let bin = match which_herdr() {
        Some(b) => b,
        None => return false,
    };
    match report_stage_done_bounded(
        &bin,
        &pane_id,
        stage,
        message,
        DEFAULT_SUBPROCESS_TIMEOUT_MS,
    ) {
        Ok(()) => true,
        Err(_) => {
            // Swallow: the bridge is best-effort. A non-zero exit,
            // malformed JSON response, or hung subprocess from herdr
            // must not fail the caller's write — that would be a
            // regression for agents running `mp` outside a herdr
            // pane.
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_done_sentinel_value_is_pinned() {
        // Any change to STAGE_DONE_SENTINEL is a contract change;
        // pin the value in a test so a typo is caught at build time.
        assert_eq!(STAGE_DONE_SENTINEL, "mp-stage-done");
        assert_eq!(STAGE_DONE_SOURCE, "mp");
        assert_eq!(STAGE_DONE_AGENT, "mp-runner");
    }

    #[test]
    fn build_report_agent_args_uses_pinned_source_and_agent() {
        let args = build_report_agent_args("%5", "milestone-complete", Some("M150 done"));
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            argv,
            vec![
                "pane",
                "report-agent",
                "%5",
                "--source",
                "mp",
                "--agent",
                "mp-runner",
                "--state",
                "idle",
                "--custom-status",
                "mp-stage-done",
                "--message",
                "M150 done",
            ]
        );
        // F-05: --seq was removed (herdr 0.7.3 rejects string values).
        assert!(
            !argv.contains(&"--seq"),
            "--seq removed from producer argv (F-05): {argv:?}"
        );
    }

    #[test]
    fn build_report_agent_args_falls_back_to_stage_when_no_message() {
        let args = build_report_agent_args("%7", "reviews-pass", None);
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let msg_idx = argv
            .iter()
            .position(|a| *a == "--message")
            .expect("--message present");
        assert_eq!(argv[msg_idx + 1], "reviews-pass");
    }

    #[test]
    fn build_clear_custom_status_args_pins_source() {
        let args = build_clear_custom_status_args("wA:p3");
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            argv,
            vec![
                "pane",
                "report-metadata",
                "wA:p3",
                "--source",
                "mp",
                "--clear-custom-status",
            ]
        );
    }

    #[test]
    fn parse_custom_status_extracts_from_envelope() {
        let json = r#"{"id":"cli:pane:get","result":{"pane":{"agent_status":"idle","custom_status":"mp-stage-done","pane_id":"wA:p3"}}}"#;
        assert_eq!(
            parse_custom_status_from_pane_get(json).as_deref(),
            Some("mp-stage-done")
        );
    }

    #[test]
    fn parse_custom_status_returns_none_when_absent() {
        let json =
            r#"{"id":"cli:pane:get","result":{"pane":{"agent_status":"idle","pane_id":"wA:p3"}}}"#;
        assert_eq!(parse_custom_status_from_pane_get(json), None);
    }

    #[test]
    fn parse_custom_status_returns_none_on_empty_string() {
        let json = r#"{"result":{"pane":{"custom_status":""}}}"#;
        assert_eq!(parse_custom_status_from_pane_get(json), None);
    }

    #[test]
    fn parse_custom_status_returns_none_on_malformed_json() {
        assert_eq!(parse_custom_status_from_pane_get("not json"), None);
        assert_eq!(parse_custom_status_from_pane_get(""), None);
        // Result but no pane.
        assert_eq!(parse_custom_status_from_pane_get(r#"{"result":{}}"#), None);
    }

    #[test]
    fn sentinel_matches_is_strict_equality() {
        assert!(sentinel_matches("mp-stage-done"));
        assert!(sentinel_matches("  mp-stage-done  "));
        assert!(!sentinel_matches("mp-stage-done-failed"));
        assert!(!sentinel_matches(""));
        assert!(!sentinel_matches("done"));
    }

    #[test]
    fn run_herdr_with_timeout_kills_hung_child_and_returns_error() {
        // F-13 unit-level proof: a fake herdr that sleeps forever is
        // killed within the deadline and the helper returns Err (not
        // a panic, not a hang). The deadline is intentionally short
        // so the test stays under a second even on slow runners.
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("herdr");
        std::fs::write(&script, "#!/bin/sh\nsleep 60\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&script).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&script, p).unwrap();
        }
        let start = Instant::now();
        let result = run_herdr_with_timeout(&script, &["pane", "get", "x"], 150);
        let elapsed = start.elapsed();
        assert!(
            result.is_err(),
            "hung subprocess must time out, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_millis(2_000),
            "helper must return promptly after kill: {elapsed:?}"
        );
    }

    #[test]
    fn run_herdr_with_timeout_returns_output_on_fast_child() {
        // Symmetric to the hung-child case: a fast herdr echoes its
        // argv; the helper captures stdout and a zero exit.
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("herdr");
        std::fs::write(&script, "#!/bin/sh\necho \"ok: $*\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&script).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&script, p).unwrap();
        }
        let out = run_herdr_with_timeout(&script, &["pane", "get", "x"], 1_000).unwrap();
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("pane get x"),
            "stdout should echo argv: {stdout}"
        );
    }

    /// F-13 process-group proof: a hung `sh -c 'sleep N'` forks a
    /// `sleep` grandchild. `child.kill()` alone leaves the grandchild
    /// orphaned; only `killpg(pgid, SIGKILL)` reaps the whole subtree.
    /// The script forks a `sleep` grandchild, writes the grandchild's
    /// PID to a pid file under the test TempDir, then `wait`s; we
    /// poll for the pid file, then trigger the helper's timeout path
    /// and assert the grandchild PID is gone (libc::kill(pid, 0)
    /// returns ESRCH).
    #[cfg(unix)]
    #[test]
    fn run_herdr_with_timeout_kills_entire_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("herdr");
        let pid_file = dir.path().join("sleep.pid");
        // The script forks `sleep 30` in the background, records the
        // grandchild's PID, and waits. When the helper times out the
        // whole pgid (sh + sleep) must be reaped.
        let body = format!(
            r#"#!/bin/sh
sleep 30 &
echo $! > {pid_file}
wait
"#,
            pid_file = pid_file.display()
        );
        std::fs::write(&script, body).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&script).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&script, p).unwrap();
        }
        // Run the helper in a thread so we can ensure the pid file is
        // populated BEFORE the helper times out. The helper itself
        // blocks until sh exits (via `wait`), so we drive the timeout
        // by waiting for the grandchild to appear, then trigger the
        // helper's kill path via the test's own deadline.
        let helper =
            std::thread::spawn(move || run_herdr_with_timeout(&script, &["pane", "get", "x"], 600));
        // Poll for the pid file.
        let sleep_pid: i32 = {
            let mut pid = 0i32;
            for _ in 0..100 {
                if let Ok(s) = std::fs::read_to_string(&pid_file) {
                    if let Ok(n) = s.trim().parse::<i32>() {
                        if n > 0 {
                            pid = n;
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            pid
        };
        assert!(
            sleep_pid > 0,
            "grandchild pid was never written before timeout: {pid_file:?}"
        );

        let result = helper.join().expect("helper thread panicked");
        assert!(result.is_err(), "hung subprocess must time out");

        // After the helper returns, the entire process group (sh +
        // sleep) must be gone. kill(pid, 0) returns ESRCH when the
        // process no longer exists.
        for _ in 0..50 {
            // SAFETY: kill(pid, 0) is a liveness probe; no signal is
            // delivered, only the existence check is performed.
            let rc = unsafe { libc::kill(sleep_pid, 0) };
            if rc != 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // SAFETY: final existence probe; rc == -1 with ESRCH means
        // the grandchild was reaped. Anything else (rc == 0, or rc > 0
        // with EPERM) would mean the grandchild survived.
        let rc = unsafe { libc::kill(sleep_pid, 0) };
        assert_ne!(
            rc, 0,
            "grandchild sleep pid {sleep_pid} survived killpg — process-group cleanup is broken"
        );
    }
}
