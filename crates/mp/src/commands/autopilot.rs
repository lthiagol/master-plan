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
    }
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
    match cmd {
        AutopilotSessionCmd::List => {
            let list = autopilot::list_sessions(ctx)?;
            emit_fields(format, &json!({ "ok": true, "sessions": list }), fields)
        }
        AutopilotSessionCmd::Show { id } => {
            let session = autopilot::load_session(ctx, &id).map_err(|e| anyhow::anyhow!("{e}"))?;
            emit_fields(format, &SessionShowReport::new(&id, &session), fields)
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
