//! M207: `mp autopilot` command dispatch.
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
//!
//! All write paths route through [`crate::autopilot::save_session`]
//! so the schema gate runs before the disk write.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::autopilot::notes::build_note;
use crate::autopilot::{
    self, AcStatus, AutopilotSession, NoteKind, OrchestrationEvent, ProjectionKey, RoleName,
    RoleState, TransitionError,
};
use crate::cli::{AutopilotCmd, AutopilotNoteCmd, AutopilotSessionCmd, NoteArgs, TransitionArgs};
use crate::commands::common::{emit, emit_fields};
use crate::paths::PlanContext;

/// Top-level dispatch.
pub(crate) fn cmd_autopilot(
    ctx: &PlanContext,
    cmd: AutopilotCmd,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    match cmd {
        AutopilotCmd::Session { cmd } => cmd_autopilot_session(ctx, cmd, format, fields),
        AutopilotCmd::Note { cmd } => match cmd {
            AutopilotNoteCmd::Add(args) => cmd_autopilot_note_add(ctx, args, format, fields),
        },
    }
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
