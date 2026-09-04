//! M207 / M209: `mp autopilot` command dispatch.
//!
//! The CLI surface (`AutopilotCmd`) is defined in
//! [`crate::cli::autopilot`]; this module owns the runtime behavior
//! for each subcommand.
//!
//! Currently routed:
//!
//! - `session list`    -> [`crate::autopilot::list_sessions`]
//! - `session show`    -> typed load + emit
//! - `note add`        -> typed insert + atomic save
//! - `session transition` -> typed transition + atomic save
//! - `config get/set`  -> umbrella-scoped read/write of the
//!   `autopilot.*` namespace; thin wrapper around `mp config`
//!   semantics so the dedicated surface can grow its own UX (schema
//!   validation, deep unset, raul Settings write-through) without
//!   disturbing the umbrella command.
//!
//! All session-write paths route through
//! [`crate::autopilot::save_session`] so the schema gate runs before
//! the disk write. Config writes route through
//! [`crate::config_cmd::config_set`] / [`crate::config_cmd::config_get`].

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::autopilot::notes::build_note;
use crate::autopilot::{
    self, AcStatus, AutopilotSession, NoteKind, OrchestrationEvent, ProjectionKey, RoleName,
    RoleState, Topology as AutopilotTopology, TransitionError,
};
use crate::cli::{
    AutopilotCmd, AutopilotConfigCmd, AutopilotNoteCmd, AutopilotSessionCmd, AutopilotStartArgs,
    NoteArgs, TransitionArgs,
};
use crate::commands::common::{emit, emit_and_exit_on_fail, emit_fields};
use crate::config_cmd::{config_get, config_set};
use crate::paths::PlanContext;

/// Top-level dispatch.
pub(crate) fn cmd_autopilot(
    ctx: &PlanContext,
    cmd: AutopilotCmd,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    match cmd {
        AutopilotCmd::Start(args) => cmd_autopilot_start(ctx, args, format),
        AutopilotCmd::Status { summary } => {
            crate::commands::watch_control::cmd_watch_control_status(ctx, summary, format, fields)
        }
        AutopilotCmd::Stop { pid, timeout_secs } => {
            crate::commands::watch_control::cmd_watch_control_stop(
                ctx,
                pid,
                timeout_secs,
                format,
                fields,
            )
        }
        AutopilotCmd::Output {
            max_bytes,
            timeout_ms,
            role,
        } => crate::commands::watch_control::cmd_watch_control_output(
            ctx, max_bytes, timeout_ms, role, format, fields,
        ),
        AutopilotCmd::Result { force } => {
            crate::commands::watch_control::cmd_watch_control_result(ctx, force, format, fields)
        }
        AutopilotCmd::Session { cmd } => cmd_autopilot_session(ctx, cmd, format, fields),
        AutopilotCmd::Note { cmd } => match cmd {
            AutopilotNoteCmd::Add(args) => cmd_autopilot_note_add(ctx, args, format, fields),
        },
        AutopilotCmd::Config { cmd } => cmd_autopilot_config(ctx, cmd, format, fields),
        AutopilotCmd::Migrate { dry_run } => cmd_autopilot_migrate(ctx, dry_run, format),
    }
}

/// M208 / S4: dispatch for `mp autopilot migrate [--dry-run]`.
/// Surface the typed migration outcome as JSON. The dry-run variant
/// inspects the legacy file and reports counts without writing; the
/// real run applies the migration idempotently.
fn cmd_autopilot_migrate(
    ctx: &PlanContext,
    dry_run: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    use crate::autopilot::migrate;
    let source_path = crate::autopilot::drive::default_state_path(&ctx.plan_dir);
    if dry_run {
        if !source_path.exists() {
            return emit(
                format,
                &json!({
                    "ok": true,
                    "dry_run": true,
                    "outcome": crate::autopilot::MigrationOutcome::NoLegacyState { source_path },
                }),
            );
        }
        // Inspect the legacy file without writing.
        let raw = std::fs::read(&source_path)
            .with_context(|| format!("read legacy watch state at {}", source_path.display()))?;
        let state: crate::autopilot::drive::state::WatchState = serde_json::from_slice(&raw)
            .with_context(|| format!("parse legacy watch state at {}", source_path.display()))?;
        return emit(
            format,
            &json!({
                "ok": true,
                "dry_run": true,
                "outcome": {
                    "kind": "would_migrate",
                    "source_path": source_path,
                    "milestones": state.milestones.len(),
                    "panes": state.panes.len(),
                    "schema_version": state.schema_version,
                },
            }),
        );
    }

    let outcome = match migrate::migrate_legacy_watch_state(ctx) {
        Ok(o) => o,
        Err(migrate::MigrationError::CorruptSource { path, reason }) => {
            bail!(
                "legacy watch state at {} is corrupt: {}",
                path.display(),
                reason
            )
        }
        Err(migrate::MigrationError::UnknownLegacySchema {
            path,
            found,
            expected,
        }) => bail!(
            "unknown legacy schema version {found} in {} (expected {expected})",
            path.display()
        ),
        Err(migrate::MigrationError::MigratedSessionInvalid(s)) => {
            bail!("migration produced an invalid session: {s}")
        }
        Err(migrate::MigrationError::Refused(s)) => bail!("migration refused: {s}"),
    };
    emit(
        format,
        &json!({
            "ok": true,
            "dry_run": false,
            "outcome": outcome,
        }),
    )
}

/// M208: `mp autopilot start [IDS]...` dispatches to the same internal
/// `cmd_watch` that powers `mp watch`. The legacy `mp watch` alias is
/// a thin wrapper around this function (with a deprecation notice on
/// stderr). AC-02 contract: identical exit codes + stdout; the only
/// permitted difference between `mp watch` and `mp autopilot start`
/// is the single legacy deprecation line on `mp watch` stderr.
pub(crate) fn cmd_autopilot_start(
    ctx: &PlanContext,
    args: AutopilotStartArgs,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    crate::commands::watch::cmd_watch(
        ctx,
        args.ids,
        args.dry_run,
        args.log_file,
        args.stall_timeout_ms,
        args.poll_interval_ms,
        args.resume,
        args.force,
        args.detach,
        format,
    )
}

fn cmd_autopilot_session(
    ctx: &PlanContext,
    cmd: AutopilotSessionCmd,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    // M208 / S3: before answering, attempt the legacy watch-state
    // migration if it has not already happened. The migration is
    // idempotent and best-effort — a failure here surfaces as an
    // empty session list, not a hard error, so a user who has no
    // legacy state (the common case) sees a clean response. Once
    // an autopilot session exists, the migration is a no-op.
    //
    // The migration error type intentionally does not propagate so a
    // corrupt legacy file does not block the session list view; the
    // operator can run `mp autopilot migrate` to see the typed
    // diagnostic. S4 makes that command visible.
    drop(autopilot::migrate_legacy_watch_state(ctx));
    match cmd {
        AutopilotSessionCmd::List => {
            let list = autopilot::list_sessions(ctx)?;
            emit_fields(format, &json!({ "ok": true, "sessions": list }), fields)
        }
        AutopilotSessionCmd::Show { id } => {
            let session = autopilot::load_session(ctx, &id).map_err(|e| anyhow::anyhow!("{e}"))?;
            emit_fields(format, &SessionShowReport::new(&id, &session), fields)
        }
        AutopilotSessionCmd::Recover { id } => {
            // M225 F-01 / AC-03 production wiring: run the
            // startup recovery on the named session and emit
            // the structured report. The recover function
            // writes the session back on `Recovered` and
            // leaves it untouched on `Rejected`. The caller
            // sees one report per session.
            let current = autopilot::spawn::MpBinaryProvenance::current();
            let report = autopilot::run_startup_recovery(ctx, &id, &current)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let payload = match &report.outcome {
                autopilot::StartupRecoveryOutcome::Recovered {
                    prev_cursor,
                    next_cursor,
                    event_count,
                } => {
                    json!({
                        "ok": true,
                        "session_id": report.session_id,
                        "outcome": "recovered",
                        "prev_cursor": prev_cursor,
                        "next_cursor": next_cursor,
                        "event_count": event_count,
                    })
                }
                autopilot::StartupRecoveryOutcome::Rejected {
                    reason,
                    event_count,
                } => {
                    json!({
                        "ok": false,
                        "session_id": report.session_id,
                        "outcome": "rejected",
                        "reason": reason,
                        "event_count": event_count,
                    })
                }
            };
            emit_fields(format, &payload, fields)
        }
        AutopilotSessionCmd::Transition(args) => {
            cmd_autopilot_transition(ctx, args, format, fields)
        }
    }
}

fn cmd_autopilot_note_add(
    ctx: &PlanContext,
    args: NoteArgs,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    let NoteArgs {
        session,
        kind,
        body,
        cycle,
        milestone,
    } = args;
    let kind = parse_note_kind(&kind)?;
    let mut loaded = autopilot::load_session(ctx, &session).map_err(|e| anyhow::anyhow!("{e}"))?;
    let note = build_note(&loaded, kind, &body, cycle, milestone.as_deref())?;
    let next_seq = loaded.event_cursor.next_seq();
    let event = OrchestrationEvent::new(
        next_seq,
        autopilot::EventKind::Note,
        "mp-cli",
        serde_json::to_value(&note)?,
    )
    .with_role(RoleName::Runner);
    loaded.event_cursor.advance_to(next_seq)?;
    loaded.events.push(event);
    loaded.runner_notes.push(note.clone());
    autopilot::save_session(ctx, &session, &loaded).map_err(|e| anyhow::anyhow!("{e}"))?;
    emit_fields(
        format,
        &json!({
            "ok": true,
            "session_id": session,
            "note": note,
        }),
        fields,
    )
}

fn cmd_autopilot_transition(
    ctx: &PlanContext,
    args: TransitionArgs,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    let TransitionArgs {
        session,
        role,
        state,
        working_on,
        actor,
    } = args;
    let role = role
        .parse::<RoleName>()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let next = state
        .parse::<RoleState>()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let wo = match working_on.as_deref() {
        Some(spec) => Some(parse_working_on(spec)?),
        None => None,
    };
    let mut loaded = autopilot::load_session(ctx, &session).map_err(|e| anyhow::anyhow!("{e}"))?;
    let outcome = autopilot::apply_transition(&mut loaded, role, next, &actor, wo)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let next_seq = loaded.event_cursor.next_seq();
    let payload = json!({
        "role": role.as_str(),
        "state": next.as_str(),
        "applied": outcome.was_applied(),
    });
    let event = OrchestrationEvent::new(next_seq, autopilot::EventKind::Transition, actor, payload)
        .with_role(role);
    loaded.event_cursor.advance_to(next_seq)?;
    loaded.events.push(event);
    autopilot::save_session(ctx, &session, &loaded).map_err(|e| anyhow::anyhow!("{e}"))?;
    emit_fields(
        format,
        &json!({
            "ok": true,
            "session_id": session,
            "role": role.as_str(),
            "state": next.as_str(),
            "applied": outcome.was_applied(),
            "record": outcome.record(),
        }),
        fields,
    )
}

/// Stub for `ApplyTransitionError` — re-exported so callers can name
/// the type without reaching into autopilot twice. Reserved for the
/// future error-type unification (currently `autopilot::apply_transition`
/// returns `Result<_, TransitionError>`).
#[allow(dead_code)]
pub type ApplyTransitionError = TransitionError;

/// Parse a `--working-on milestone:cycle` spec.
pub(crate) fn parse_working_on(spec: &str) -> Result<autopilot::WorkingOn> {
    let (milestone_id, cycle) = spec
        .split_once(':')
        .context("--working-on must be of the form milestone:cycle")?;
    let cycle: u32 = cycle
        .parse()
        .context("--working-on cycle must be a non-negative integer")?;
    if cycle == 0 {
        bail!("--working-on cycle must be >= 1");
    }
    Ok(autopilot::WorkingOn {
        milestone_id: milestone_id.to_string(),
        cycle,
        role: None,
    })
}

pub(crate) fn parse_note_kind(s: &str) -> Result<NoteKind> {
    Ok(match s {
        "info" => NoteKind::Info,
        "warn" => NoteKind::Warn,
        "blocker" => NoteKind::Blocker,
        "decision" => NoteKind::Decision,
        "reminder" => NoteKind::Reminder,
        "system" => NoteKind::System,
        other => bail!("unknown note kind {other:?}"),
    })
}

#[derive(Debug, Serialize)]
struct SessionShowReport {
    ok: bool,
    session_id: String,
    session: AutopilotSession,
}

impl SessionShowReport {
    fn new(id: &str, session: &AutopilotSession) -> Self {
        Self {
            ok: true,
            session_id: id.to_string(),
            session: session.clone(),
        }
    }
}

// ─── M209: `mp autopilot config …` dispatch ─────────────────────────

/// M209: dispatch for `mp autopilot config {get,set}`. The args are
/// the same as `mp config {get,set} autopilot.<key>` — the dedicated
/// surface exists so the autopilot UX can grow (deep unset, schema
/// hints, raul Settings write-through) without touching the umbrella
/// `mp config` command.
fn cmd_autopilot_config(
    ctx: &PlanContext,
    cmd: AutopilotConfigCmd,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    match cmd {
        AutopilotConfigCmd::Get { key } => cmd_autopilot_config_get(ctx, &key, format, fields),
        AutopilotConfigCmd::Set {
            key,
            value,
            dry_run,
        } => cmd_autopilot_config_set(ctx, &key, &value, dry_run, format, fields),
    }
}

fn cmd_autopilot_config_get(
    ctx: &PlanContext,
    key: &str,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    let normalized = ensure_autopilot_prefix(key);
    let value = config_get(ctx, &normalized)?;
    emit_fields(
        format,
        &json!({
            "ok": true,
            "key": normalized,
            "value": value,
        }),
        fields,
    )
}

fn cmd_autopilot_config_set(
    ctx: &PlanContext,
    key: &str,
    value: &str,
    dry_run: bool,
    format: crate::cli::OutputFormat,
    _fields: &[String],
) -> Result<()> {
    let normalized = ensure_autopilot_prefix(key);
    let report = config_set(ctx, &normalized, value, dry_run)?;
    let payload = json!({
        "ok": report.ok,
        "dry_run": report.dry_run,
        "key": normalized,
        "value": report.value,
        "errors": report.errors,
        "warnings": report.warnings,
    });
    // Mirror the umbrella `mp config set` contract: non-zero exit
    // on validation failure so callers can `set && use`. The set
    // report has no per-field projection surface today, so
    // `_fields` is reserved for a future schema-aware projection.
    emit_and_exit_on_fail(format, &payload, report.ok)
}

/// M209: accept either `autopilot.topology` or `topology` so the
/// user does not have to type the prefix twice. Anything else is a
/// structured error surfaced by `config_get` / `config_set` (which
/// already know the `autopilot.` prefix).
fn ensure_autopilot_prefix(key: &str) -> String {
    if key.starts_with("autopilot.") {
        key.to_string()
    } else {
        format!("autopilot.{key}")
    }
}

// Silence unused-imports: `AutopilotTopology` is reserved for a
// future `mp autopilot config schema` surface that lists the
// allowed topology values. The S3 commit uses `Topology` via
// `config_get_autopilot` (config_cmd.rs) where string round-trips
// matter more than the enum — the constant lives on for cross-module
// discoverability.
#[allow(dead_code)]
const _AUTOPILOT_TOPOLOGY: AutopilotTopology = AutopilotTopology::ThreeAgent;

// Reserved: a future command (e.g. `mp autopilot ac project …`)
// will route through the projection helpers below. The re-export
// keeps the surface intentional rather than dead-code.
#[allow(dead_code)]
fn _ac_surface(_: ProjectionKey, _: AcStatus) -> autopilot::ProjectionWriteOutcome {
    autopilot::ProjectionWriteOutcome::NoChange
}

#[allow(dead_code)]
fn _emit(format: crate::cli::OutputFormat, payload: &impl Serialize) -> Result<()> {
    emit(format, payload)
}
