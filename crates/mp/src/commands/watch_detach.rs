//! M178 S3 / AC-02: detach-safe `mp watch <ids...> --detach`.
//!
//! The starting client persists the v2 control-plane state, forks
//! a fully-detached child that re-runs the foreground driver, then
//! exits. The child uses `setsid` on Unix so it survives the
//! parent's exit (no SIGHUP propagation).
//!
//! ## Why a separate module
//!
//! The foreground `cmd_watch_drive` path is intentionally kept as a
//! single blocking call so the existing CLI behavior is unchanged.
//! Detach adds a fork/exec/wait dance that doesn't belong in the
//! hot foreground loop. The persisted-state contract (AC-01, AC-06)
//! is shared via [`crate::watch::WatchRunState`].

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::config::ProjectConfig;
use crate::paths::PlanContext;
use crate::watch::{PreconditionReport, WatchRunState};

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_watch_detached(
    ctx: &PlanContext,
    ids: &[String],
    cfg: &ProjectConfig,
    log_path: &Path,
    preconditions: PreconditionReport,
    stall_timeout_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
    resume: bool,
    force: bool,
    format: Fmt,
) -> Result<()> {
    // Validate preconditions up-front so the detached child doesn't
    // fail silently on the operator's behalf. Surface the same
    // structured report a foreground --dry-run would emit.
    if !preconditions.ok {
        let report = serde_json::json!({
            "dry_run": false,
            "detach": true,
            "preconditions": preconditions,
            "detached_pid": serde_json::Value::Null,
            "message": "preconditions failed; refusing to detach",
        });
        emit(format, &report)?;
        anyhow::bail!("preconditions failed; refusing to detach");
    }

    // Build the v2 control-plane state with the queue and the
    // expected log/state paths so the child process inherits the
    // same surface the client would see if it polled
    // `mp watch-control status`.
    let mut state = WatchRunState::fresh(ids);
    state.log_path = Some(log_path.to_string_lossy().into_owned());
    state.state_path = Some(
        crate::watch::default_run_state_path(&ctx.plan_dir)
            .to_string_lossy()
            .into_owned(),
    );
    state.save_to_plan(ctx)?;

    // Resolve our own binary path so we can re-invoke ourselves as
    // the detached child. The watch command path is the canonical
    // way for `mp` to discover itself.
    let mp_bin = std::env::current_exe().context("resolve mp binary for detach")?;

    // Build the child argv: `mp watch <ids...> <flags>` (no --detach).
    // We re-emit the original flags so the child takes the exact same
    // path through cmd_watch_drive that the foreground client would
    // have taken, minus --detach (the child runs in the foreground
    // of its own process tree).
    let mut child_args: Vec<String> = vec!["watch".to_string()];
    for id in ids {
        child_args.push(id.clone());
    }
    if let Some(p) = stall_timeout_ms {
        child_args.push("--stall-timeout-ms".into());
        child_args.push(p.to_string());
    }
    if let Some(p) = poll_interval_ms {
        child_args.push("--poll-interval-ms".into());
        child_args.push(p.to_string());
    }
    if resume {
        child_args.push("--resume".into());
    }
    if force {
        child_args.push("--force".into());
    }
    if let Some(lp) = log_path.to_str() {
        child_args.push("--log-file".into());
        child_args.push(lp.to_string());
    }
    child_args.push("--format".into());
    child_args.push(format!("{format:?}").to_lowercase());

    let install_dir = std::env::var("MP_INSTALL_DIR").ok().map(PathBuf::from);
    let plan_dir = ctx.plan_dir.clone();

    let mut cmd = Command::new(&mp_bin);
    // M178 external-review F-07: env_clear() drops inherited secrets
    // (AWS_*, GH_TOKEN, etc.) before re-applying the canonical mp
    // vars + a minimal PATH. The detached child is long-lived; an
    // operator who runs `GH_TOKEN=… mp watch --detach M178` would
    // otherwise have that secret propagated to every spawned agent
    // even after the starting shell exited.
    cmd.env_clear()
        .env("MP_HOME", ctx.project_root.clone())
        .env("MP_PLAN_DIR", &plan_dir)
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
        );
    if let Some(dir) = install_dir.as_ref() {
        cmd.env("MP_INSTALL_DIR", dir);
    }
    for arg in &child_args {
        cmd.arg(arg);
    }
    // Detach: stdin/stdout/stderr to /dev/null so the child doesn't
    // hold the parent's terminal.
    cmd.stdin(std::process::Stdio::null());
    // M178 external-review F-05: the watch logger inside
    // `cmd_watch_drive` writes structured JSONL to the log file
    // itself. The detached child's own stdout/stderr would only
    // duplicate noise (Rust panics, herdr subprocess output)
    // onto the operator's terminal — which is exactly what detach
    // is meant to avoid. Route both to /dev/null. Errors opening
    // /dev/null fall back to a piped stdio (the watch log
    // captures the meaningful output).
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    // setsid: detach the child into its own session so SIGHUP from
    // the parent's terminal exit doesn't reach it. (Unix only — the
    // detach flag is documented as Unix-only at the CLI level.)
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid(2) is async-signal-safe per POSIX.1-2017.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn().context("spawn detached mp watch child")?;
    let child_pid = child.id();

    // Patch the persisted state with the child's PID so a subsequent
    // `mp watch-control status` and `mp watch-control stop` can find
    // it without scanning the system process table.
    let state_after = WatchRunState::load_from(&WatchRunState::path_for(&ctx.plan_dir))?
        .unwrap_or_else(|| WatchRunState::fresh(ids));
    let path = WatchRunState::path_for(&ctx.plan_dir);
    let mut store = crate::watch::WatchRunStore::new(path, state_after);
    store.transition(crate::watch::WatchTransition::Pid(child_pid))?;

    let _ = cfg; // keep cfg referenced for symmetry with cmd_watch_drive
    let report = DetachReport {
        dry_run: false,
        detach: true,
        detached_pid: child_pid,
        log_file: log_path.display().to_string(),
        state_file: WatchRunState::path_for(&ctx.plan_dir).display().to_string(),
        preconditions,
        message: format!(
            "detached watch started; pid={child_pid}; poll with `mp watch-control status`"
        ),
    };
    emit(format, &report)?;
    Ok(())
}

#[allow(dead_code)]
fn redirect_log(_log_path: &Path) -> std::process::Stdio {
    // M178 external-review F-05: the helper is currently unused
    // (the detach path routes stdio to /dev/null directly via
    // `Stdio::null()`). Kept as a stub so a future caller that
    // wants to redirect to `log_path` (e.g. to capture child
    // stderr into the watch log) has a documented entry point.
    // Suppresses clippy because the parameter is intentionally
    // unused for now.
    let _ = _log_path;
    std::process::Stdio::null()
}

#[derive(Debug, Serialize)]
struct DetachReport {
    dry_run: bool,
    detach: bool,
    detached_pid: u32,
    log_file: String,
    state_file: String,
    preconditions: PreconditionReport,
    message: String,
}
