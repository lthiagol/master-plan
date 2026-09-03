//! `mp watch` command surface (M149).
//!
//! S0 precondition check; S2 CLI dispatch + per-milestone dry-run
//! preview; S3–S8 herdr layer + state machine + sequencer; S10
//! structured logging.
//!
//! Contract:
//! - `mp watch --help` prints usage.
//! - `mp watch <ids...> --dry-run` resolves each id, reports the
//!   current lifecycle / spec / execution status, the next action
//!   the runner *would* take, and the herdr command that *would* be
//!   run — without modifying `plan.json` or spawning any agent.
//! - `mp watch <ids...>` (no `--dry-run`) dispatches to
//!   [`run_milestones`] (S8) and drives each milestone via the
//!   state machine (S7) + herdr layer (S3–S5). Precondition failures
//!   halt at startup; the per-role pane cache persists across
//!   milestones (AC-04). Structured JSONL logs land at
//!   `<plan_dir>/.mp/watch.log` by default (S10).
//! - Non-zero exit on precondition failure (without `--dry-run`),
//!   skipped milestone, or iteration-cap exhaustion.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::autopilot::{AutopilotGateError, EX_AUTOPILOT_GATE};
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::config::ProjectConfig;
use crate::milestone::load_milestone_by_id;
use crate::model::MilestoneFile;
use crate::paths::PlanContext;
use crate::store;
use crate::watch::{
    build_pane_split_args, build_start_args, check_preconditions, default_log_path,
    harness_extra_flags, next_stage, pane_label_for, resolve_harness_kind, run_milestones,
    try_lazy_auto_set, which_herdr, PreconditionReport, PromptStage, Role, SequencerReport,
    SystemDriveOps, WatchLogEntry, WatchLogger, DEFAULT_PANE_N,
};

/// Maximum state-machine iterations per milestone before giving up
/// and halting the sequencer. Bounded to keep runaway remediation
/// loops from spinning forever (review finding #5).
const MAX_ITERATIONS_PER_MILESTONE: usize = 10;

/// M197 F-14: dry-run argv preview placeholder for the pane id that
/// the live spawn would inject from `pane_split`'s stdout. The
/// literal sentinel shows up in operator-facing JSON; the named
/// const keeps grep / docs references consistent (one source of
/// truth instead of a string literal repeated across the preview
/// builder).
const DRY_RUN_PANE_ID_PLACEHOLDER: &str = "%pane-id%";

/// M149 + M152 driver surface. Carries enough scalar arguments
/// that the parameter list exceeds clippy's default cap; the
/// `#[allow]` here matches the M149 precedent which routed the
/// earlier args through `DriveOpts` to stay under 7. M152's new
/// `--resume` / `--force` flags live alongside the rest of the
/// scalar surface for symmetry with the CLI module (the CLI
/// itself emits one `Watch` variant that maps directly to these
/// fields). Bundling them into a struct here would force a
/// parallel shape across `cli/mod.rs` + `app.rs` for no win.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_watch(
    ctx: &PlanContext,
    ids: Vec<String>,
    dry_run: bool,
    log_file: Option<PathBuf>,
    stall_timeout_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
    resume: bool,
    force: bool,
    detach: bool,
    format: Fmt,
) -> Result<()> {
    // M218 / AC-01 + AC-03: autopilot hard gate — refuse to start
    // when herdr is missing or below the required version. Fires
    // BEFORE any plan-state write (lazy auto-set), BEFORE any spawn
    // operation, and BEFORE the dry-run split so the legacy `mp
    // watch` and `mp autopilot start` paths share the same gate
    // behavior. `--force` does NOT bypass (herdr is required by
    // design; `--force` keeps its M178 double-spawn-guard role).
    if let Err(err) = crate::autopilot::check_autopilot_herdr_gate_default() {
        emit(format, &GateReport::from_error(&err))?;
        return Err(crate::ExitCode(err.exit_code).into());
    }

    let mut cfg = store::load_config(ctx);
    let log_path = log_file
        .clone()
        .unwrap_or_else(|| default_log_path(&ctx.plan_dir));

    // M197 WP1 / AC-01: lazy auto-set fallback. When the user runs
    // `mp watch` against a project that never went through
    // `mp init` (or whose harness skill was installed after init),
    // auto-fill the agent role harnesses from the single installed
    // harness. The decision is persisted to config.json so a
    // subsequent `mp doctor` reflects the auto-set. On ambiguity
    // (multiple installed harnesses) the config is left untouched
    // and the precondition check below will fail with the existing
    // "harness not set" message — the operator resolves the
    // ambiguity by hand via `mp config set`.
    let auto_set_decision = try_lazy_auto_set(&mut cfg);
    if let crate::harness::AutoSetDecision::AutoSet { harness } = &auto_set_decision {
        if let Err(e) = store::write_config(ctx, &cfg) {
            // Non-fatal: a config-write failure here just means a
            // follow-up `mp watch` will re-run the auto-set. Log
            // and continue.
            eprintln!("warning: failed to persist auto-set harness config: {e:#}");
        }
        // M197 F-04: emit a structured activity event so the
        // activity feed captures the auto-set path symmetrically
        // with the explicit `mp init` path. Best-effort: a write
        // failure here is non-fatal (the persisted config is the
        // source of truth; the activity event is audit-only).
        let _ = crate::activity::append_event_best_effort(
            ctx,
            crate::activity::lazy_auto_set_event(harness),
        );
    }
    let preconditions = check_preconditions(&cfg, &log_path);

    if dry_run {
        return cmd_watch_dry_run(ctx, &ids, &cfg, &log_path, preconditions, format);
    }

    // M178 S3 / AC-02: detach-safe mode. The starting client exits as
    // soon as the state file is persisted and the detached child is
    // spawned; the actual driver runs in the background and remains
    // discoverable through `mp watch-control status`. The detached
    // child uses `setsid` (Unix) so it survives the parent's exit.
    //
    // M178 external-review F-06: detach is Unix-only — Windows has
    // no setsid/SIGHUP semantics. Refuse the flag up-front on
    // non-Unix platforms with a structured error rather than
    // silently panicking on `/dev/null`.
    if detach {
        #[cfg(unix)]
        {
            return crate::commands::watch_detach::cmd_watch_detached(
                ctx,
                &ids,
                &cfg,
                &log_path,
                preconditions,
                stall_timeout_ms,
                poll_interval_ms,
                resume,
                force,
                format,
            );
        }
        #[cfg(not(unix))]
        {
            let report = serde_json::json!({
                "dry_run": false,
                "detach": true,
                "ok": false,
                "error": "platform_not_supported",
                "message": "--detach requires Unix (setsid + SIGHUP semantics are POSIX-only)",
            });
            emit(format, &report)?;
            anyhow::bail!("--detach requires Unix");
        }
    }

    cmd_watch_drive(DriveOpts {
        ctx,
        ids: &ids,
        cfg: &cfg,
        log_path: &log_path,
        preconditions,
        stall_timeout_ms,
        poll_interval_ms,
        resume,
        force,
    })
}

/// S2/S9 dry-run: print the execution plan without modifying plan.json
/// or spawning any agent. Precondition failures surface as JSON
/// (exit 0 — dry-run is a preview).
fn cmd_watch_dry_run(
    ctx: &PlanContext,
    ids: &[String],
    cfg: &ProjectConfig,
    log_path: &std::path::Path,
    preconditions: PreconditionReport,
    format: Fmt,
) -> Result<()> {
    let milestones = resolve_milestones(ctx, ids, cfg, &ctx.project_root);
    let plan = WatchPlan {
        dry_run: true,
        log_file: log_path.to_string_lossy().to_string(),
        preconditions,
        milestones,
    };
    emit(format, &plan)?;
    Ok(())
}

/// Bundled options for `cmd_watch_drive` so the function signature
/// stays under clippy's 7-arg ceiling (review-finding clippy fix).
struct DriveOpts<'a> {
    ctx: &'a PlanContext,
    ids: &'a [String],
    cfg: &'a ProjectConfig,
    log_path: &'a std::path::Path,
    preconditions: PreconditionReport,
    stall_timeout_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
    /// M152 / AC-02: re-attach to any herdr role panes that already
    /// exist for the active milestones (`mp watch --resume`).
    resume: bool,
    /// M152 / AC-03: bypass the double-spawn guard (`mp watch
    /// --force`). The default `mp watch` refuses to run when role
    /// panes already exist; `--force` skips that check. After the
    /// gate, `--force` and `--resume` are equivalent — both
    /// re-attach to the existing panes (see F-04 ext-review for the
    /// docs-vs-behavior note).
    force: bool,
}

/// S7/S8 real execution: drive each milestone through the state
/// machine via `run_milestones`. Halt on precondition failure or
/// MaxIterationsExhausted; surface per-milestone outcomes in the
/// JSON report. Exit non-zero when preconditions failed or any
/// milestone was skipped/exhausted.
fn cmd_watch_drive(opts: DriveOpts<'_>) -> Result<()> {
    let DriveOpts {
        ctx,
        ids,
        cfg,
        log_path,
        preconditions,
        stall_timeout_ms,
        poll_interval_ms,
        resume,
        force,
    } = opts;
    let format = Fmt::Json;
    // Review finding #1: the non-dry-run path now actually dispatches
    // to the sequencer. Previously this branch only emitted a report
    // and returned Ok(()) — mp watch was library-only.

    // M152 S4: install SIGINT / SIGTERM handlers BEFORE any long
    // subprocess work begins. The handlers flip a global atomic;
    // the drive loop polls the atomic on every iteration so a
    // Ctrl-C ends the run within one stage-transition latence.
    // Idempotent — calling twice is a no-op.
    crate::watch::install_signal_handlers();

    // Gate on preconditions before any subprocess work.
    if !preconditions.ok {
        let report = DriveReport {
            dry_run: false,
            log_file: log_path.to_string_lossy().to_string(),
            preconditions,
            sequencer: None,
        };
        emit(format, &report)?;
        return Err(crate::ExitCode(2).into());
    }

    // Resolve binary paths.
    let mp_bin = std::env::current_exe().with_context(|| {
        "mp watch: cannot determine current_exe (needed to spawn mp subprocesses)"
    })?;
    let herdr_bin = which_herdr().ok_or_else(|| {
        anyhow::anyhow!(
            "mp watch: herdr not found on PATH — install from https://herdr.dev/docs/install"
        )
    })?;

    // Attach structured logger (S10 / review finding #2).
    let logger = WatchLogger::open(log_path)
        .with_context(|| format!("mp watch: failed to open log file {}", log_path.display()))?;
    logger
        .log(&WatchLogEntry::new(
            "boot",
            format!(
                "mp watch starting: ids={:?} log={} resume={resume} force={force}",
                ids,
                log_path.display()
            ),
        ))
        .ok();

    let role_configs = crate::watch::state_machine::RoleConfigs {
        runner: cfg.runner_config().clone(),
        coordinator: cfg.coordinator_config().clone(),
    };

    // ─── M152 / AC-02 + AC-03: resume-or-refuse gate ─────────────────────
    // Query herdr agent list and reconcile against the (optional)
    // recorded watch state. Default `mp watch` refuses to spawn a
    // second role pane when one already exists for the active
    // milestones. `--resume` opts in to re-attaching to live panes;
    // `--force` skips the check (still re-uses on resume). A corrupt
    // or absent herdr list is treated as "no live panes" so the
    // check never blocks startup.
    let herdr_list_json = crate::watch::list_panes(&herdr_bin).unwrap_or_default();
    let recorded_state = crate::watch::WatchState::load(ctx).ok().flatten();
    let reconciliation = crate::watch::reconcile(recorded_state.as_ref(), &herdr_list_json);

    // ─── M225 F-01 wiring (AC-03: resume from last valid event) ─────────
    // On every `mp watch` / `cmd_autopilot_start` invocation, run
    // `run_startup_recovery_all` over every session.json under the
    // plan dir. Each session is loaded, the M225 cursor-vs-events
    // gate is consulted, and a `Recovered` verdict writes the
    // session back when the cursor moved. `Rejected` reports are
    // logged but do NOT block the run — a corrupt session in one
    // id does not stop other sessions from executing (the F-01
    // contract is "resume safely; never fabricate completion").
    let startup_recovery = match crate::autopilot::run_startup_recovery_all(
        ctx,
        &crate::autopilot::spawn::MpBinaryProvenance::current(),
    ) {
        Ok(reports) => reports,
        Err(e) => {
            // Recovery scan itself failed (e.g. read_dir error).
            // Log + continue; the resume gate below still runs.
            logger
                .log(&WatchLogEntry::new(
                    "startup_recovery_failed",
                    format!("{e}"),
                ))
                .ok();
            Vec::new()
        }
    };
    for report in &startup_recovery {
        match &report.outcome {
            crate::autopilot::StartupRecoveryOutcome::Recovered {
                prev_cursor,
                next_cursor,
                event_count,
            } => {
                logger
                    .log(&WatchLogEntry::new(
                        "startup_recovery_recovered",
                        format!(
                            "session={} prev_cursor={} next_cursor={} events={}",
                            report.session_id, prev_cursor, next_cursor, event_count
                        ),
                    ))
                    .ok();
            }
            crate::autopilot::StartupRecoveryOutcome::Rejected {
                reason,
                event_count,
            } => {
                logger
                    .log(&WatchLogEntry::new(
                        "startup_recovery_rejected",
                        format!(
                            "session={} events={} reason={}",
                            report.session_id, event_count, reason
                        ),
                    ))
                    .ok();
            }
        }
    }
    let live_panes: Vec<(&'static str, &str)> = [
        (
            "runner",
            match &reconciliation.runner {
                crate::watch::PaneStatus::Live { pane_id, .. } => Some(pane_id.as_str()),
                _ => None,
            },
        ),
        (
            "coordinator",
            match &reconciliation.coordinator {
                crate::watch::PaneStatus::Live { pane_id, .. } => Some(pane_id.as_str()),
                _ => None,
            },
        ),
    ]
    .into_iter()
    .filter_map(|(role, id)| id.map(|id| (role, id)))
    .collect();
    let has_live_panes = !live_panes.is_empty();

    // ─── M225 F-01 wiring (AC-02: pane loss classification) ─────────
    // The M152 reconciler classifies every role pane as Live /
    // Dead / Missing. M225 adds a typed classification on top of
    // `Dead`: re-spawn may be `Safe` (stored prompt + actor
    // available) or `AwaitingUser` (no stored prompt, role
    // removed from topology, etc.). The M225 contract is "no
    // fabricated completion after pane restart" — a `Dead` pane
    // with `AwaitingUser` outcome means the operator must
    // intervene; the drive loop must not silently re-spawn.
    let runner_dead = matches!(reconciliation.runner, crate::watch::PaneStatus::Dead { .. });
    let coordinator_dead = matches!(
        reconciliation.coordinator,
        crate::watch::PaneStatus::Dead { .. }
    );
    if runner_dead || coordinator_dead {
        for (role_label, is_dead) in [("runner", runner_dead), ("coordinator", coordinator_dead)] {
            if !is_dead {
                continue;
            }
            let role = match role_label {
                "runner" => crate::autopilot::RoleName::Runner,
                _ => crate::autopilot::RoleName::Reviewer, // legacy 2-pane coord
            };
            let stored_prompt: Option<String> = None;
            let stored_actor: Option<String> = None;
            let input = crate::autopilot::PaneLossInput {
                role,
                pane_live: false,
                topology_role_present: true,
                stored_prompt: stored_prompt.as_deref(),
                stored_actor: stored_actor.as_deref(),
            };
            let outcome = crate::autopilot::classify_pane_loss(&input);
            match outcome {
                crate::autopilot::PaneLossOutcome::SafeRespawn { .. } => {
                    logger
                        .log(&WatchLogEntry::new(
                            "pane_loss_safe_respawn",
                            format!("role={role_label}: stored prompt/actor absent; defaulting to re-spawn via state machine"),
                        ))
                        .ok();
                }
                crate::autopilot::PaneLossOutcome::AwaitingUser { reason } => {
                    logger
                        .log(&WatchLogEntry::new(
                            "pane_loss_awaiting_user",
                            format!("role={role_label} reason={reason}"),
                        ))
                        .ok();
                }
            }
        }
    }

    if has_live_panes && !resume && !force {
        // AC-03 default: refuse to double-spawn. Surface every live
        // pane the user might want to attach to so they can pick
        // --resume or --force from the message.
        let details = live_panes
            .iter()
            .map(|(role, id)| format!("{role}={id}"))
            .collect::<Vec<_>>()
            .join(", ");
        let msg = format!(
            "mp watch refused: herdr reports an existing role pane for the \
             active milestones ({details}). Two panes on one milestone \
             cause duplicate work and conflicting plan writes. Re-run with \
             `mp watch --resume <ids...>` to re-attach, or `mp watch \
             --force <ids...>` to bypass this check."
        );
        logger
            .log(&WatchLogEntry::new(
                "double_spawn_refused",
                format!("refused on existing panes: {details}"),
            ))
            .ok();
        let report = serde_json::json!({
            "dry_run": false,
            "log_file": log_path.to_string_lossy(),
            "preconditions": preconditions,
            "sequencer": serde_json::Value::Null,
            "resume_gate": {
                "ok": false,
                "reason": "existing_role_pane",
                "live_panes": live_panes
                    .iter()
                    .map(|(role, id)| serde_json::json!({"role": role, "pane_id": id}))
                    .collect::<Vec<_>>(),
                "hint": "use --resume to re-attach, or --force to ignore",
                "message": msg.clone(),
            }
        });
        emit(format, &report)?;
        anyhow::bail!("{msg}");
    }

    let mut ops = SystemDriveOps::new(
        mp_bin,
        herdr_bin,
        ctx.project_root.clone(),
        ids.first().cloned().unwrap_or_default(),
        role_configs,
    );
    ops.set_logger(logger.clone());
    // M226 F-01 wiring: stamp the autopilot session id on the
    // ops so `ensure_pane` consults the session event log for a
    // prior `AssignmentDispatched` event before spawning. The
    // session id is the milestone id (the autopilot drive model
    // has one session per milestone); when `mp autopilot session
    // create` has not run, the load is non-fatal and the dedup
    // check falls through to the normal spawn path.
    if let Some(first_id) = ids.first() {
        ops.set_session_id(first_id.clone());
    }

    // M180 S5: record one watch-started event. Best-effort — the
    // watch driver must run regardless of journal write success.
    crate::activity::append_event_best_effort(ctx, crate::activity::watch_started_event(ids))?;
    if let Some(poll) = poll_interval_ms {
        ops.set_wait_options(crate::watch::WaitOptions {
            poll_interval_ms: poll,
            ..ops.wait_options()
        });
    }
    if let Some(stall) = stall_timeout_ms {
        ops.set_wait_options(crate::watch::WaitOptions {
            stall_timeout_ms: stall,
            ..ops.wait_options()
        });
    }

    // AC-02: when resuming (or when re-using force-kept existing
    // panes), pre-populate the pane cache from the reconciliation
    // result so the state machine's first `ensure_pane` returns the
    // cached handle without an extra herdr spawn. Force is treated
    // the same way here — the difference between --resume and
    // --force is the gate we just enforced; once past the gate, the
    // ops-level pane reuse is identical.
    if has_live_panes {
        for (role_label, pane_id) in &live_panes {
            let role = match *role_label {
                "runner" => crate::watch::Role::Runner,
                "coordinator" => crate::watch::Role::Coordinator,
                _ => continue,
            };
            let label = format!("role-{role_label}-1");
            ops.pane_cache.insert(
                role,
                crate::watch::PaneHandle {
                    label,
                    pane_id: pane_id.to_string(),
                    reused: true,
                },
            );
        }
        logger
            .log(&WatchLogEntry::new(
                "resume_reuse",
                format!(
                    "re-attaching to live panes: {}",
                    live_panes
                        .iter()
                        .map(|(r, i)| format!("{r}={i}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
            .ok();
    }

    // AC-01 / M178 S1+S2: build the v2 control-plane state with the
    // supplied queue, attach it to the ops, and persist the initial
    // snapshot before the sequencer starts. Doing the write here
    // means even a crash in the first iteration cannot erase prior
    // state (the v2 schema also subsumes the v1 panes/milestones
    // tracking, so `--resume` against this file keeps working
    // through the existing reconciliation path).
    let mut state = crate::watch::WatchRunState::fresh(ids);
    // Carry the persisted panes/milestones v1-shape into the v2
    // struct so the legacy `--resume` reconciliation continues to
    // find pane id and last-known-lifecycle records.
    upsert_panes_from_cache_v2(&mut state, &ops);
    // The active milestone queue index starts unset — the
    // sequencer sets it on the first set_active_milestone call so
    // the persisted state and the in-memory ops never disagree on
    // queue order.
    state.log_path = Some(log_path.to_string_lossy().into_owned());
    state.state_path = Some(
        crate::watch::default_run_state_path(&ctx.plan_dir)
            .to_string_lossy()
            .into_owned(),
    );
    ops.attach_run_state(state.clone());
    let initial_state_save = state
        .save_to_plan(ctx)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| anyhow::anyhow!("{e}"));
    match initial_state_save {
        Ok(path) => {
            logger
                .log(&WatchLogEntry::new(
                    "state_persisted",
                    format!("initial state at {path}"),
                ))
                .ok();
        }
        Err(e) => {
            // State writes are best-effort during a normal run.
            // A disk error here surfaces in the watch.log for
            // forensics but does not abort the run — `mp watch`
            // continues; the graceful-shutdown path will retry
            // the save before exit.
            logger
                .log(&WatchLogEntry::new(
                    "state_persist_failed",
                    format!("initial state save failed: {e:#}"),
                ))
                .ok();
        }
    }

    let sequencer_result = run_milestones(&mut ops, ids, MAX_ITERATIONS_PER_MILESTONE);

    // M152 S4: if SIGINT/SIGTERM flipped the global shutdown flag
    // mid-run, the drive-loop returned the new
    // `DriveOutcome::Shutdown` variant; the sequencer halted. We
    // still need to (a) flush `.mp/watch.state.json` and (b)
    // record a flash note on the in-flight milestone so a
    // subsequent `mp watch --resume` can pick up where we left
    // off. `perform_graceful_shutdown` does both; it never
    // bubbles its own errors so a cleanup hiccup never blocks
    // exit.
    let graceful_shutdown = crate::watch::shutdown_requested();
    if graceful_shutdown {
        // Re-read milestones from disk to capture the latest
        // lifecycle for the flash note (instead of guessing from
        // the sequencer's last transition). Best-effort: a
        // reload failure degrades to "(unknown)" in the note.
        let active = ops.active_milestone_id.clone();
        let last_lifecycle = active
            .as_deref()
            .and_then(|id| crate::milestone::load_milestone_by_id(ctx, id).ok())
            .map(|m| m.milestone.lifecycle);
        // M178 S2: build a v2 state to hand to the legacy
        // perform_graceful_shutdown helper (which still takes the
        // v1 WatchState shape for backwards compatibility). We
        // translate by lifting the v2 panes + milestones back into
        // a fresh v1 state. The v2 control-plane state itself is
        // already attached to ops and re-persisted below.
        let mut legacy_state =
            crate::watch::WatchState::fresh(&active.clone().into_iter().collect::<Vec<_>>());
        if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Runner) {
            legacy_state.upsert_pane(crate::watch::PaneState {
                role: crate::watch::Role::Runner,
                label: pane.label.clone(),
                pane_id: pane.pane_id.clone(),
                spawned_at: "t".into(),
                last_status: None,
            });
        }
        if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Coordinator) {
            legacy_state.upsert_pane(crate::watch::PaneState {
                role: crate::watch::Role::Coordinator,
                label: pane.label.clone(),
                pane_id: pane.pane_id.clone(),
                spawned_at: "t".into(),
                last_status: None,
            });
        }
        if let (Some(ms), Some(lc)) = (active.as_deref(), last_lifecycle.as_deref()) {
            // M152 ext-review F-02 (2026-07-14): use the actual
            // target the sequencer was waiting for, not a hardcoded
            // placeholder. Falls back to "self-reviewed" if the ops
            // hasn't recorded one yet (e.g., shutdown before any
            // wait_for_lifecycle ran — graceful SIGINT during spawn).
            let target_lifecycle = ops
                .current_target()
                .map(|t| t.as_str().to_string())
                .unwrap_or_else(|| "self-reviewed".to_string());
            legacy_state.upsert_milestone(crate::watch::MilestoneState {
                id: ms.to_string(),
                last_lifecycle: lc.to_string(),
                target_lifecycle,
                last_action_at: crate::store::now_rfc3339(),
            });
        }
        // M178 S2: also stamp the v2 state with the terminal
        // outcome so `mp watch status` reads run_outcome=GracefullyStopped
        // after a SIGINT/SIGTERM. We don't pop an additional
        // per-milestone entry — the sequencer already wrote the
        // Shutdown outcome through record_milestone_outcome.
        if ops
            .run_state()
            .is_some_and(|state| state.run_outcome.is_none())
        {
            let _ = ops.transition(crate::watch::WatchTransition::RunOutcome(
                crate::watch::RunOutcome::GracefullyStopped,
            ));
        }
        let _ = crate::watch::perform_graceful_shutdown(
            ctx,
            &legacy_state,
            active.as_deref(),
            last_lifecycle.as_deref(),
            Some(&logger),
        );
        logger
            .log(&WatchLogEntry::new(
                "shutdown_signal",
                "graceful shutdown: state flushed + flash note recorded",
            ))
            .ok();
    }

    let halt = match &sequencer_result {
        Ok(report) => !report.all_complete,
        Err(_) => true,
    };
    // Emit a report regardless of pass/fail so callers always see
    // structured output. Errors from the sequencer propagate after
    // emit so the process exit code still reflects failure.
    let sequencer = sequencer_result.ok();
    let report = DriveReport {
        dry_run: false,
        log_file: log_path.to_string_lossy().to_string(),
        preconditions,
        sequencer,
    };
    emit(format, &report)?;

    // M152 S4: graceful shutdown exits 0 (not 2). The user asked
    // the run to stop; reporting "failure" would mislead `raul`
    // and any CI hook polling for `mp watch ...` exit codes.
    if graceful_shutdown {
        return Ok(());
    }
    if halt {
        return Err(crate::ExitCode(2).into());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct DriveReport {
    dry_run: bool,
    log_file: String,
    preconditions: PreconditionReport,
    sequencer: Option<SequencerReport>,
}

/// Per-milestone resolution entry. Mirrors the shape future steps
/// (S5/S7) will fill in with herdr pane ids, lifecycle transitions,
/// and evidence trails.
#[derive(Debug, Serialize)]
struct WatchMilestone {
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lifecycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spec_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked: Option<bool>,
    /// AC-02 readiness verdict: lifecycle=approved, spec_status=ready,
    /// execution_status=planned, not blocked, not cancelled, not
    /// deferred. AC-07 (skip logic) lives on top of this in S7.
    #[serde(skip_serializing_if = "Option::is_none")]
    ready: Option<bool>,
    /// Coarse next-action label for the dry-run preview. Refined by
    /// the state machine in S7.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_action: Option<&'static str>,
    /// S9 dry-run expansion: the stage the state machine would
    /// dispatch next, plus the lifecycle target it would wait for.
    /// Absent when the milestone is skipped or already complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_lifecycle: Option<&'static str>,
    /// S9 dry-run expansion: the herdr CLI argv that would be invoked
    /// to spawn the role's pane (one per role the loop would touch).
    /// Empty when the milestone is skipped or already complete.
    herdr_commands: Vec<HerdrPreview>,
    /// S9 dry-run expansion: the prompt text (truncated for the JSON
    /// view) that would be delivered to the pane. Absent when the
    /// milestone is skipped or the runner is already in-progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_preview: Option<String>,
    /// S9 / M153 ext-review F-10: stable `prompt_source` field for
    /// the rendered preview. One of `"override"` / `"default"` /
    /// `"hardcoded"` / the stage-specific hardcoded label. The
    /// companion `prompt_source_path` (when present) carries the
    /// override file the dry-run resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_source_path: Option<String>,
    /// S9 / M153 ext-review F-11: structured refusal diagnostics
    /// from template resolution. Each entry corresponds to one rung
    /// the loader examined and refused (empty file, missing
    /// `{header}`, non-regular, oversized, etc.). Empty when every
    /// rung was clean or absent.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    override_diagnostics: Vec<OverrideDiagnosticView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// One herdr invocation that the dry-run preview says it would run.
#[derive(Debug, Serialize)]
struct HerdrPreview {
    /// Which role this invocation targets.
    role: &'static str,
    /// The pane label that herdr would display.
    label: String,
    /// The full argv as a single shell-quoted string.
    argv: Vec<String>,
}

/// JSON-friendly view of an override refusal. Mirrors the fields
/// the watch log emits in `override_refused` events so the dry-run
/// report and the live log line up without a translation step.
#[derive(Debug, Serialize)]
struct OverrideDiagnosticView {
    rung: &'static str,
    kind: &'static str,
    path: String,
    message: String,
}

impl OverrideDiagnosticView {
    fn from(d: &crate::watch::OverrideDiagnostic) -> Self {
        let rung = match d.rung {
            crate::watch::OverrideRung::OverrideDir => "override_dir",
            crate::watch::OverrideRung::PlanDir => "plan_dir",
        };
        let kind = match d.kind {
            crate::watch::OverrideRefusalKind::NotRegular => "not_regular",
            crate::watch::OverrideRefusalKind::TooLarge => "too_large",
            crate::watch::OverrideRefusalKind::Empty => "empty",
            crate::watch::OverrideRefusalKind::HeaderMissing => "header_missing",
            crate::watch::OverrideRefusalKind::InvalidUtf8 => "invalid_utf8",
            crate::watch::OverrideRefusalKind::ReadError => "read_error",
        };
        Self {
            rung,
            kind,
            path: d.path.display().to_string(),
            message: d.message.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct WatchPlan {
    dry_run: bool,
    log_file: String,
    preconditions: PreconditionReport,
    milestones: Vec<WatchMilestone>,
}

/// Copy any cached panes from the ops state-machine view into the
/// supplied `WatchState` for persistence. The ops pane cache is the
/// authoritative view at runtime — populating the state file from
/// it means a crash leaves the recorded pane ids ready for
/// `--resume` to re-attach to.
#[allow(dead_code)]
fn upsert_panes_from_cache(state: &mut crate::watch::WatchState, ops: &SystemDriveOps) {
    if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Runner) {
        state.upsert_pane(crate::watch::PaneState {
            role: crate::watch::Role::Runner,
            label: pane.label.clone(),
            pane_id: pane.pane_id.clone(),
            spawned_at: crate::store::now_rfc3339(),
            last_status: None,
        });
    }
    if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Coordinator) {
        state.upsert_pane(crate::watch::PaneState {
            role: crate::watch::Role::Coordinator,
            label: pane.label.clone(),
            pane_id: pane.pane_id.clone(),
            spawned_at: crate::store::now_rfc3339(),
            last_status: None,
        });
    }
}

/// M178 S1+S2: v2 counterpart of [`upsert_panes_from_cache`]. The v2
/// state preserves the legacy `panes` array verbatim AND populates the
/// flat `pane_ids` map keyed by role. Both are read by `--resume` /
/// `mp watch output`; the flat map is the new authoritative read
/// path so callers don't have to scan the array.
fn upsert_panes_from_cache_v2(state: &mut crate::watch::WatchRunState, ops: &SystemDriveOps) {
    if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Runner) {
        state.panes.push(crate::watch::PaneState {
            role: crate::watch::Role::Runner,
            label: pane.label.clone(),
            pane_id: pane.pane_id.clone(),
            spawned_at: crate::store::now_rfc3339(),
            last_status: None,
        });
        state.record_pane(crate::watch::Role::Runner, pane.pane_id.clone());
    }
    if let Some(pane) = ops.pane_cache.get(&crate::watch::Role::Coordinator) {
        state.panes.push(crate::watch::PaneState {
            role: crate::watch::Role::Coordinator,
            label: pane.label.clone(),
            pane_id: pane.pane_id.clone(),
            spawned_at: crate::store::now_rfc3339(),
            last_status: None,
        });
        state.record_pane(crate::watch::Role::Coordinator, pane.pane_id.clone());
    }
}

fn resolve_milestones(
    ctx: &PlanContext,
    ids: &[String],
    cfg: &ProjectConfig,
    project_root: &std::path::Path,
) -> Vec<WatchMilestone> {
    ids.iter()
        .map(|raw| resolve_one(ctx, raw, cfg, project_root))
        .collect()
}

fn resolve_one(
    ctx: &PlanContext,
    raw: &str,
    cfg: &ProjectConfig,
    project_root: &std::path::Path,
) -> WatchMilestone {
    let mut entry = WatchMilestone {
        input: raw.to_string(),
        id: None,
        title: None,
        lifecycle: None,
        spec_status: None,
        execution_status: None,
        blocked: None,
        ready: None,
        next_action: None,
        stage: None,
        target_lifecycle: None,
        herdr_commands: Vec::new(),
        prompt_preview: None,
        prompt_source: None,
        prompt_source_path: None,
        override_diagnostics: Vec::new(),
        error: None,
    };
    let m = match load_milestone_by_id(ctx, raw) {
        Ok(m) => m,
        Err(e) => {
            entry.error = Some(format!("{e:#}"));
            return entry;
        }
    };
    let ready = is_ready(&m);
    let action = next_action(&m, ready);
    let ms = &m.milestone;
    entry.id = Some(ms.id.clone());
    entry.title = Some(ms.title.clone());
    entry.lifecycle = Some(ms.lifecycle.clone());
    entry.spec_status = Some(ms.spec_status.clone());
    entry.execution_status = Some(ms.execution_status.clone());
    entry.blocked = Some(ms.blocked);
    entry.ready = Some(ready);
    entry.next_action = Some(action);

    // S9: attach the stage plan + herdr preview for milestones the
    // state machine would actually drive. Skip milestones get neither.
    if let Some(plan) = next_stage(&m) {
        entry.stage = Some(plan.stage.label());
        entry.target_lifecycle = Some(plan.target.as_str());

        let role = plan.stage.role();
        let rc = match role {
            Role::Runner => cfg.runner_config(),
            Role::Coordinator => cfg.coordinator_config(),
        };
        // The herdr argv that ensure_pane would invoke on first
        // spawn. M197 WP2 / AC-03: the dry-run shows the new
        // 0.7.x two-step shape — `pane split --cwd <PATH>` to
        // create the pane, then `agent start <NAME> --kind
        // <KIND> --pane <PANE_ID>` to start the agent inside it.
        // The pane id is a placeholder (herdr creates the real
        // one on the wire); the dry-run is about argv shape, not
        // the synthesized pane id.
        let label = pane_label_for(role, DEFAULT_PANE_N);
        let kind = resolve_harness_kind(rc);
        // M197 followup: forward the harness-specific extras
        // (model / thinking flags from `HarnessRegistry::resolve_argv`)
        // into the dry-run preview so the operator sees the
        // harness argv herdr will forward. The dry-run is
        // authoritative for the wire shape; live spawn threads
        // the same extras through `ensure_pane` /
        // `spawn_pane(extras)`.
        let extras = harness_extra_flags(rc);
        entry.herdr_commands.push(HerdrPreview {
            role: role.label(),
            label: label.clone(),
            argv: build_pane_split_args(project_root),
        });
        entry.herdr_commands.push(HerdrPreview {
            role: role.label(),
            label,
            argv: build_start_args(
                &pane_label_for(role, DEFAULT_PANE_N),
                &kind,
                DRY_RUN_PANE_ID_PLACEHOLDER,
                &extras,
            ),
        });

        // Don't preview a prompt for in-progress milestones — the
        // state machine polls rather than re-prompting (S7
        // already_dispatched branch).
        let in_progress_execute =
            ms.lifecycle == "in-progress" && plan.stage == PromptStage::Execute;
        if !in_progress_execute {
            // M153 ext-review F-10: the dry-run must render through
            // the same override path as the live state machine,
            // otherwise the operator preview diverges from what the
            // runner pane will receive. Thread `plan_dir` and use
            // the diagnostics-aware renderer so refusal reasons
            // surface here too (F-11).
            let req = crate::watch::BuildPromptRequest {
                stage: plan.stage,
                milestone: &m,
                options: &crate::watch::PromptRenderOptions::default(),
                override_dir: None,
                plan_dir: Some(&ctx.plan_dir),
            };
            let rendered = crate::watch::build_prompt_full(&req, crate::watch::MAX_OVERRIDE_BYTES);
            entry.prompt_source = Some(rendered.source.label().to_string());
            entry.prompt_source_path = match &rendered.source {
                crate::watch::TemplateSource::ProjectOverride(p) => Some(p.display().to_string()),
                _ => None,
            };
            entry.override_diagnostics = rendered
                .override_diagnostics
                .iter()
                .map(OverrideDiagnosticView::from)
                .collect();
            // Truncate by CHAR count (not byte length) so multi-byte
            // titles don't get spurious "…" suffixes. Review finding #10.
            let prompt = rendered.text;
            let preview: String = prompt.chars().take(280).collect();
            let char_count = prompt.chars().count();
            let preview = if char_count > 280 {
                format!("{preview}…")
            } else {
                preview
            };
            entry.prompt_preview = Some(preview);
        }
    }

    entry
}

/// AC-02 readiness verdict. Centralized here so S7 (state machine)
/// and AC-07 (skip logic) consume one definition. Pub(crate) so the
/// watch state machine (crate-internal) can call into it without
/// exposing it through the public API.
pub(crate) fn is_ready(m: &MilestoneFile) -> bool {
    let ms = &m.milestone;
    ms.lifecycle == "approved"
        && ms.spec_status == "ready"
        && ms.execution_status == "planned"
        && !ms.blocked
        && !ms.cancelled
        && !ms.deferred
}

/// Coarse next-action label for the dry-run preview. The real state
/// machine (S7) drives lifecycle transitions; this is a human/agent
/// hint that summarizes what would happen next.
fn next_action(m: &MilestoneFile, ready: bool) -> &'static str {
    let ms = &m.milestone;
    if ms.cancelled || ms.deferred {
        return "skip";
    }
    if ms.blocked {
        return "skip_blocked";
    }
    match ms.lifecycle.as_str() {
        "approved" if ready => "execute",
        "approved" => "skip_not_ready",
        "in-progress" => "continue_execute",
        "self-reviewed" => "external_review",
        "reviewed" => "remediate_or_complete",
        "complete" => "skip_done",
        _ => "skip_unknown_lifecycle",
    }
}

/// Convenience helper kept for S0 callers / library users that want
/// the precondition report without the watch plan envelope.
#[allow(dead_code)]
pub(crate) fn cmd_watch_preconditions(ctx: &PlanContext, format: Fmt) -> Result<()> {
    let cfg = store::load_config(ctx);
    let log_path = default_log_path(&ctx.plan_dir);
    let report = check_preconditions(&cfg, &log_path);
    emit(format, &json!({ "preconditions": report }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MilestoneFile, MilestoneMeta};

    fn ms(id: &str, lifecycle: &str, spec: &str, exec: &str) -> MilestoneFile {
        MilestoneFile {
            milestone: MilestoneMeta {
                id: id.to_string(),
                lifecycle: lifecycle.to_string(),
                spec_status: spec.to_string(),
                execution_status: exec.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn ready_verdict_matches_ac02_definition() {
        let m = ms("1", "approved", "ready", "planned");
        assert!(is_ready(&m));
    }

    #[test]
    fn ready_verdict_rejects_inprogress() {
        let m = ms("1", "in-progress", "ready", "planned");
        assert!(!is_ready(&m), "in-progress is mid-execution, not ready");
    }

    #[test]
    fn ready_verdict_rejects_blocked() {
        let mut m = ms("1", "approved", "ready", "planned");
        m.milestone.blocked = true;
        assert!(!is_ready(&m));
    }

    #[test]
    fn ready_verdict_rejects_cancelled_and_deferred() {
        let mut m = ms("1", "approved", "ready", "planned");
        m.milestone.cancelled = true;
        assert!(!is_ready(&m));
        m.milestone.cancelled = false;
        m.milestone.deferred = true;
        assert!(!is_ready(&m));
    }

    #[test]
    fn next_action_routes_approved_ready_to_execute() {
        let m = ms("1", "approved", "ready", "planned");
        assert_eq!(next_action(&m, true), "execute");
    }

    #[test]
    fn next_action_routes_self_reviewed_to_external_review() {
        let m = ms("1", "self-reviewed", "ready", "in-progress");
        assert_eq!(next_action(&m, false), "external_review");
    }

    #[test]
    fn next_action_skips_complete() {
        let m = ms("1", "complete", "ready", "complete");
        assert_eq!(next_action(&m, false), "skip_done");
    }

    #[test]
    fn next_action_skips_cancelled_and_deferred() {
        let mut m = ms("1", "approved", "ready", "planned");
        m.milestone.cancelled = true;
        assert_eq!(next_action(&m, false), "skip");
        m.milestone.cancelled = false;
        m.milestone.deferred = true;
        assert_eq!(next_action(&m, false), "skip");
    }

    #[test]
    fn plan_serializes_to_json_with_expected_top_level_keys() {
        let plan = WatchPlan {
            dry_run: true,
            log_file: "/tmp/watch.log".to_string(),
            preconditions: PreconditionReport {
                ok: true,
                checks: vec![],
            },
            milestones: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&plan).unwrap();
        assert!(v["dry_run"].is_boolean());
        assert!(v["log_file"].is_string());
        assert!(v["preconditions"]["checks"].is_array());
        assert!(v["milestones"].is_array());
    }
}

/// M218 / AC-01 + AC-03: structured JSON envelope for the autopilot
/// hard-gate refusal. The `ok: false` flag is the contract every
/// downstream tool checks; the nested `autopilot_herdr_gate` payload
/// carries the typed reason + actionable hints. Both `mp autopilot
/// start` and the legacy `mp watch` alias emit this same shape.
#[derive(Debug, Serialize)]
struct GateReport {
    ok: bool,
    autopilot_herdr_gate: AutopilotGateError,
}

impl GateReport {
    fn from_error(err: &AutopilotGateError) -> Self {
        Self {
            ok: false,
            autopilot_herdr_gate: err.clone(),
        }
    }
}

// Suppress an "unused import" lint for `EX_AUTOPILOT_GATE` — the
// constant is referenced by name in the docstring above and is part
// of the agent contract (78 = EX_CONFIG). Importing it here keeps the
// doc + the import list in one place for future readers.
#[allow(dead_code)]
const _AUTOPILOT_GATE_EXIT: i32 = EX_AUTOPILOT_GATE;
