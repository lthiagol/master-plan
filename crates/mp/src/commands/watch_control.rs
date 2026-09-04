//! M178 S3 + S4 + S5 + S6 + S7 + S8: structured `mp watch-control *`
//! commands.
//!
//! Four verbs:
//! - `status`  — AC-01, AC-03, AC-06: read the v2 control-plane state
//!   and classify it as live / stale / terminal.
//! - `stop`    — AC-04: gracefully stop the recorded live watch.
//! - `output`  — AC-05: bounded, structured output from the active pane.
//! - `result`  — AC-06: read the latest terminal outcome.
//!
//! Each verb is a thin surface over [`crate::autopilot::drive::WatchRunState`]
//! and the existing herdr / shutdown / bridge primitives.

use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;

use crate::autopilot::drive::{
    is_pid_alive, RunOutcome, WatchRunState, WATCH_RUN_STATE_SCHEMA_VERSION,
};
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::paths::PlanContext;

use crate::autopilot::drive::classification::{classify_state, StatusReport};

/// AC-01 / AC-03 / AC-06: read the latest-run state and emit a
/// structured status. The classifier returns `Live` / `Stale` /
/// `Terminal` from the recorded PID + herdr-list probe; the
/// remaining v2 fields are surfaced verbatim.
pub(crate) fn cmd_watch_control_status(
    ctx: &PlanContext,
    summary: bool,
    format: Fmt,
    _fields: &[String],
) -> Result<()> {
    let mut report = build_status_report(ctx)?;
    if summary {
        // Strip the full state payload; keep only the classification
        // and the probes. Operators that want the full state pass
        // without `--summary`.
        report.state = None;
    }
    emit(format, &report)?;
    Ok(())
}

fn build_status_report(ctx: &PlanContext) -> Result<StatusReport> {
    let path = WatchRunState::path_for(&ctx.plan_dir);
    let state = WatchRunState::load_from(&path)?;
    let herdr_list = read_herdr_list_best_effort();
    let classification = classify_state(state.as_ref(), herdr_list.as_deref());
    let pid_alive = state.as_ref().map(|s| is_pid_alive(s.pid)).unwrap_or(false);
    let report = StatusReport {
        run_state: classification,
        state_file: path.display().to_string(),
        schema_version: WATCH_RUN_STATE_SCHEMA_VERSION,
        state,
        pid_alive,
        herdr_listed: herdr_list.is_some(),
    };
    Ok(report)
}

/// AC-04: graceful stop via SIGINT to the recorded PID. The pid
/// argument overrides the recorded PID when supplied (useful when
/// the state file is stale or missing). Stable non-destructive
/// response when no live run exists.
pub(crate) fn cmd_watch_control_stop(
    ctx: &PlanContext,
    pid_override: Option<u32>,
    timeout_secs: u64,
    format: Fmt,
    _fields: &[String],
) -> Result<()> {
    let path = WatchRunState::path_for(&ctx.plan_dir);
    let state = WatchRunState::load_from(&path)?;

    // Resolve the target PID from the override or the state file.
    let target_pid = match pid_override {
        Some(p) => Some(p),
        None => state.as_ref().map(|s| s.pid),
    };
    let report = match target_pid {
        None => StopReport {
            stopped: false,
            pid: None,
            timeout_secs,
            elapsed_secs: 0.0,
            message: "no live run; nothing to stop".to_string(),
            state_file: path.display().to_string(),
        },
        Some(pid) if !is_pid_alive(pid) => StopReport {
            stopped: false,
            pid: Some(pid),
            timeout_secs,
            elapsed_secs: 0.0,
            message: format!("pid {pid} not alive; nothing to stop"),
            state_file: path.display().to_string(),
        },
        Some(pid) => {
            // Send SIGINT (the existing graceful-shutdown path
            // listens for both SIGINT and SIGTERM; SIGINT is the
            // operator-friendly default).
            raise_sigint(pid)?;
            // Wait for the process to exit, bounded by timeout.
            let start = Instant::now();
            let timeout = Duration::from_secs(timeout_secs);
            let mut exited = false;
            while start.elapsed() < timeout {
                if !is_pid_alive(pid) {
                    exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let elapsed = start.elapsed().as_secs_f64();
            // Best-effort: re-persist the state with a terminal
            // outcome if the run wasn't already terminal. The
            // detached child should already do this in its
            // graceful-shutdown handler; the stop caller doing it
            // here covers the case where the child crashed before
            // it could clean up.
            if exited {
                match WatchRunState::load_from(&path) {
                    Ok(Some(s)) => {
                        if s.run_outcome.is_none() {
                            let mut store =
                                crate::autopilot::drive::WatchRunStore::new(path.clone(), s);
                            let _ = store.transition(
                                crate::autopilot::drive::WatchTransition::RunOutcome(
                                    RunOutcome::GracefullyStopped,
                                ),
                            );
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("[M178-STOP] warning: state reload failed during stop: {e:#}")
                    }
                }
            }
            StopReport {
                stopped: exited,
                pid: Some(pid),
                timeout_secs,
                elapsed_secs: elapsed,
                message: if exited {
                    format!("signaled pid {pid}; exited in {elapsed:.2}s")
                } else {
                    format!(
                        "signaled pid {pid}; still alive after {elapsed:.2}s (timeout {timeout_secs}s)"
                    )
                },
                state_file: path.display().to_string(),
            }
        }
    };
    emit(format, &report)?;
    // M180 S5: record one watch-stopped event when the stop
    // actually exited the recorded run. The detached child also
    // calls set_run_outcome(GracefullyStopped) before exiting;
    // when the stop caller races ahead of the child's flush, the
    // state file may already carry the terminal outcome and we
    // skip the duplicate event (the child's SystemDriveOps record
    // path emitted the canonical one).
    if report.stopped {
        if let Some(pid) = report.pid {
            let already_terminal = WatchRunState::load_from(&path)
                .ok()
                .flatten()
                .map(|s| s.run_outcome.is_some())
                .unwrap_or(false);
            if !already_terminal {
                crate::activity::append_event_best_effort(
                    ctx,
                    crate::activity::watch_stopped_event(pid),
                )?;
            }
        }
    }
    Ok(())
}

/// AC-05: bounded structured output from the active herdr pane.
/// Bounded by max_bytes (read limit) and timeout_ms (subprocess
/// wall-clock budget). Returns a structured error (not a hang) when
/// herdr is missing, the pane is dead, or the subprocess times out.
pub(crate) fn cmd_watch_control_output(
    ctx: &PlanContext,
    max_bytes: usize,
    timeout_ms: u64,
    role_override: Option<String>,
    format: Fmt,
    _fields: &[String],
) -> Result<()> {
    let path = WatchRunState::path_for(&ctx.plan_dir);
    let state = match WatchRunState::load_from(&path)? {
        Some(s) => s,
        None => {
            emit(
                format,
                &OutputReport {
                    ok: false,
                    reason: "no_state_file".to_string(),
                    role: None,
                    pane_id: None,
                    bytes: 0,
                    truncated: false,
                    elapsed_ms: 0,
                    output: String::new(),
                    message: "no watch state file; nothing to read".to_string(),
                },
            )?;
            return Ok(());
        }
    };

    // Parse the role override from the CLI string. Accepts
    // `runner` or `coordinator` (matching the serde rename_all
    // shape). An invalid string falls through to the recorded
    // active_role.
    let parsed_override =
        role_override
            .as_deref()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "runner" => Some(crate::autopilot::drive::Role::Runner),
                "coordinator" => Some(crate::autopilot::drive::Role::Coordinator),
                _ => None,
            });

    // Pick the role: --role override wins; otherwise the recorded
    // active_role; otherwise default to Runner.
    let role = parsed_override
        .or(state.active_role)
        .unwrap_or(crate::autopilot::drive::Role::Runner);
    let pane_id = match state.pane_ids.get(&role) {
        Some(id) => id.clone(),
        None => {
            emit(
                format,
                &OutputReport {
                    ok: false,
                    reason: "no_pane_for_role".to_string(),
                    role: Some(role),
                    pane_id: None,
                    bytes: 0,
                    truncated: false,
                    elapsed_ms: 0,
                    output: String::new(),
                    message: format!("no pane recorded for role {}", role.label()),
                },
            )?;
            return Ok(());
        }
    };
    // Use the existing bridge helper to read the pane's
    // custom-status. `read_custom_status_bounded` honors the
    // `timeout_ms` subprocess budget; we apply `max_bytes` as a
    // post-truncate so callers can cap the output regardless of
    // what herdr returned.
    let start = Instant::now();
    let herdr_bin = crate::autopilot::drive::resolve_herdr_binary().unwrap_or_default();
    let read_result: anyhow::Result<Option<String>> = if herdr_bin.as_os_str().is_empty() {
        Err(anyhow::anyhow!("herdr not on PATH"))
    } else {
        crate::autopilot::drive::read_custom_status_bounded(&herdr_bin, &pane_id, timeout_ms)
    };
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let report = match read_result {
        Ok(Some(text)) => {
            let (truncated, kept) = if text.len() > max_bytes {
                (true, truncate_at_char_boundary(&text, max_bytes))
            } else {
                (false, text)
            };
            OutputReport {
                ok: true,
                reason: "ok".to_string(),
                role: Some(role),
                pane_id: Some(pane_id),
                bytes: kept.len(),
                truncated,
                elapsed_ms,
                output: kept,
                message: "ok".to_string(),
            }
        }
        Ok(None) => OutputReport {
            ok: false,
            reason: "no_status_set".to_string(),
            role: Some(role),
            pane_id: Some(pane_id),
            bytes: 0,
            truncated: false,
            elapsed_ms,
            output: String::new(),
            message: "herdr returned no custom status for this pane".to_string(),
        },
        Err(e) => OutputReport {
            ok: false,
            reason: classify_output_error(&e),
            role: Some(role),
            pane_id: Some(pane_id),
            bytes: 0,
            truncated: false,
            elapsed_ms,
            output: String::new(),
            message: format!("{e:#}"),
        },
    };
    emit(format, &report)?;
    Ok(())
}

/// AC-06: read the latest terminal outcome (per-milestone log +
/// run_outcome). Distinct from `status` in that it never observes
/// a live run; returns `null` fields when the only run on record
/// is still in flight.
pub(crate) fn cmd_watch_control_result(
    ctx: &PlanContext,
    _force: bool,
    format: Fmt,
    _fields: &[String],
) -> Result<()> {
    let path = WatchRunState::path_for(&ctx.plan_dir);
    let state = WatchRunState::load_from(&path)?;
    let report = ResultReport {
        state_file: path.display().to_string(),
        state,
    };
    emit(format, &report)?;
    Ok(())
}

fn classify_output_error(e: &anyhow::Error) -> String {
    let msg = format!("{e:#}");
    if msg.contains("timed out") || msg.contains("TimeoutExpired") {
        return "pane_read_timeout".to_string();
    }
    if msg.contains("NotFound") || msg.contains("not found") || msg.contains("No such") {
        return "missing_herdr_or_pane".to_string();
    }
    if msg.contains("Permission") || msg.contains("denied") {
        return "permission_denied".to_string();
    }
    "pane_read_failed".to_string()
}

fn raise_sigint(pid: u32) -> Result<()> {
    // SAFETY: kill(pid, sig) with sig=SIGINT is async-signal-safe
    // per POSIX.1-2017. We only call it on a pid we trust (the
    // recorded mp watch pid, or a --pid override the operator
    // supplied).
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    if result != 0 {
        let err = std::io::Error::last_os_error();
        anyhow::bail!("kill(pid={pid}, SIGINT) failed: {err}");
    }
    Ok(())
}

fn read_herdr_list_best_effort() -> Option<String> {
    let herdr_bin = crate::autopilot::drive::resolve_herdr_binary().ok()?;
    let output = std::process::Command::new(&herdr_bin)
        .args(["agent", "list", "--format", "json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

// ─── output JSON shapes (M178 S3 / S4 / S5 / S7 / S8) ──────────────

#[derive(Debug, Serialize)]
struct StopReport {
    stopped: bool,
    pid: Option<u32>,
    timeout_secs: u64,
    elapsed_secs: f64,
    message: String,
    state_file: String,
}

#[derive(Debug, Serialize)]
struct OutputReport {
    ok: bool,
    reason: String,
    role: Option<crate::autopilot::drive::Role>,
    pane_id: Option<String>,
    bytes: usize,
    truncated: bool,
    elapsed_ms: u64,
    output: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct ResultReport {
    state_file: String,
    state: Option<WatchRunState>,
}

/// M178 external-review F-11: floor a byte cap to the previous UTF-8
/// char boundary so slicing never panics on multi-byte sequences
/// (emoji / CJK / accented Latin). Returns the longest prefix whose
/// byte length is `<= max_bytes`. Returns the empty string when even
/// the first character exceeds `max_bytes`.
fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> String {
    if max_bytes >= text.len() {
        return text.to_string();
    }
    // Walk char indices; advance `end` past each char whose end byte
    // is still <= max_bytes. Stop at the first char that would cross
    // the cap so we never include a partial sequence.
    let mut end = 0;
    for (i, c) in text.char_indices() {
        let next_end = i + c.len_utf8();
        if next_end > max_bytes {
            break;
        }
        end = next_end;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod truncate_tests {
    //! M178 external-review F-11 regression: `text[..max_bytes]`
    //! panicked when `max_bytes` fell inside a multi-byte sequence.
    //! Pin every boundary case so a future "simplification" back to
    //! a bare slice trips at test time rather than in production.
    use super::*;

    #[test]
    fn ascii_text_under_cap_is_returned_verbatim() {
        assert_eq!(truncate_at_char_boundary("hello", 10), "hello");
    }

    #[test]
    fn ascii_text_over_cap_truncates_cleanly() {
        assert_eq!(truncate_at_char_boundary("hello", 3), "hel");
    }

    #[test]
    fn max_bytes_inside_multibyte_sequence_floors_to_previous_boundary() {
        // "abécd": é is 2 bytes → indices are a=0, b=1, é=2..4, c=4, d=5.
        // max_bytes=3 lands inside é; the floor is byte 2 ("ab").
        assert_eq!(truncate_at_char_boundary("abécd", 3), "ab");
    }

    #[test]
    fn max_bytes_exactly_on_char_end_boundary_keeps_the_char() {
        // "abécd": a=1, b=1, é=2 bytes → a+b+é = 4 bytes.
        // max_bytes=4 lands exactly at é's end → "abé" (4 bytes).
        assert_eq!(truncate_at_char_boundary("abécd", 4), "abé");
        // max_bytes=5 includes c → "abéc" (5 bytes).
        assert_eq!(truncate_at_char_boundary("abécd", 5), "abéc");
    }

    #[test]
    fn emoji_sequence_floors_correctly() {
        // "a😀b": 😀 is 4 bytes → a=0, 😀=1..5, b=5.
        // max_bytes=2 lands inside the emoji → floor to "a".
        assert_eq!(truncate_at_char_boundary("a😀b", 2), "a");
        // max_bytes=5 lands exactly after the emoji → "a😀".
        assert_eq!(truncate_at_char_boundary("a😀b", 5), "a😀");
    }

    #[test]
    fn max_bytes_smaller_than_first_char_returns_empty_string() {
        // First char is 4-byte emoji; cap=2 cannot include it.
        assert_eq!(truncate_at_char_boundary("😀hello", 2), "");
    }

    #[test]
    fn zero_max_bytes_returns_empty_string() {
        assert_eq!(truncate_at_char_boundary("hello", 0), "");
    }

    #[test]
    fn empty_text_returns_empty_string() {
        assert_eq!(truncate_at_char_boundary("", 100), "");
    }
}
