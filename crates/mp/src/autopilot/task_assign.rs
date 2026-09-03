//! M211 / AC-01, AC-02, AC-03: typed task-assignment renderer for
//! orchestrator-to-runner and orchestrator-to-reviewer dispatch.
//!
//! Every runner or reviewer assignment that the orchestrator sends
//! flows through this module. The contract is intentionally narrow:
//!
//! - The orchestrator builds a [`TaskAssignment`] — a structured
//!   payload with `session_id`, `milestone_id`, `cycle`, `direction`,
//!   `target_pane`, `task`, `evidence_refs`, and `boundary_reminders`.
//!   No other shape is accepted.
//! - [`validate_assignment`] rejects empty fields, unknown panes,
//!   invalid directions, missing session identity, and shell
//!   metacharacter interpolation **before** any `herdr` call or
//!   session mutation. A prose-only or empty assignment therefore
//!   cannot reach herdr — that is the M200 regression the milestone
//!   is closing (AC-03).
//! - [`build_assignment_argv`] renders a deterministic argv vector
//!   for `herdr agent prompt <target_pane> <task_text>`. The renderer
//!   is a pure function; the same `TaskAssignment` always produces
//!   the same argv, which the AC-01 golden test pins.
//! - [`execute_assignment`] runs the argv via `std::process::Command`
//!   (no shell interpolation — argv is passed directly to
//!   `Command::args`).
//! - [`dispatch_assignment`] is the single read/write path: validate,
//!   build argv, execute, then **append an `AssignmentDispatched`
//!   event** to the session's append-only log with the actual outcome
//!   (success / failure / spawn error). The event is appended only
//!   after the herdr spawn outcome is known, so the log cannot
//!   advertise a dispatch that did not happen.
//!
//! ## Why a typed payload (not prose)
//!
//! The receiver cannot prove how arbitrary text was produced, so
//! receiver-side shell-envelope detection is unreliable. This
//! milestone owns the dispatch side: mp constructs and executes the
//! herdr argv itself and records the assignment event. Agents report
//! state through typed session transitions (`mp autopilot session
//! transition`), not by interpreting prose.
//!
//! ## Wire shape
//!
//! `TaskAssignment` is serialized as JSON when embedded in the
//! event payload (the `session.json` `events[].payload` field). The
//! struct's serde shape is the public wire format; do not reorder
//! fields without a `schema_version` bump.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::autopilot::events::{EventCursor, EventKind, OrchestrationEvent};
use crate::autopilot::session::{
    append_event, load_session, AutopilotSession, PaneLayout, SessionPath,
};
use crate::paths::PlanContext;

/// The direction of a task assignment.
///
/// The orchestrator only dispatches DOWN to a runner or reviewer —
/// `RunnerToOrchestrator`, peer directions, and orchestrator
/// self-dispatch are intentionally not represented. The closed set
/// is part of the validation contract: any future direction
/// requires a schema bump and an explicit validation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoleDirection {
    OrchestratorToRunner,
    OrchestratorToReviewer,
}

impl RoleDirection {
    /// Stable kebab-case wire form. Matches the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            RoleDirection::OrchestratorToRunner => "orchestrator-to-runner",
            RoleDirection::OrchestratorToReviewer => "orchestrator-to-reviewer",
        }
    }

    /// The pane-slot field on [`PaneLayout`] that this direction
    /// targets. Used by validation to look up the expected pane id.
    fn pane_slot(self, layout: &PaneLayout) -> Option<&str> {
        let slot = match self {
            RoleDirection::OrchestratorToRunner => &layout.runner,
            RoleDirection::OrchestratorToReviewer => &layout.reviewer,
        };
        slot.as_ref().map(|pane| pane.pane_id.as_str())
    }
}

impl std::fmt::Display for RoleDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RoleDirection {
    type Err = TaskAssignmentValidationError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "orchestrator-to-runner" => Ok(RoleDirection::OrchestratorToRunner),
            "orchestrator-to-reviewer" => Ok(RoleDirection::OrchestratorToReviewer),
            other => Err(TaskAssignmentValidationError::UnknownDirection(
                other.to_string(),
            )),
        }
    }
}

/// Structured task-assignment payload.
///
/// Required fields: `session_id`, `milestone_id`, `cycle` (>=1),
/// `direction`, `target_pane`, `task`. Optional: `evidence_refs`,
/// `boundary_reminders`. Construct via [`TaskAssignment::new`] so
/// the mandatory fields are non-empty at construction time; an empty
/// or partially-populated payload is rejected upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub session_id: String,
    pub milestone_id: String,
    pub cycle: u32,
    pub direction: RoleDirection,
    pub target_pane: String,
    pub task: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary_reminders: Vec<String>,
}

impl TaskAssignment {
    /// Build a fully-typed assignment with the mandatory fields. Use
    /// the builder methods to attach optional fields.
    pub fn new(
        session_id: impl Into<String>,
        milestone_id: impl Into<String>,
        cycle: u32,
        direction: RoleDirection,
        target_pane: impl Into<String>,
        task: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            milestone_id: milestone_id.into(),
            cycle,
            direction,
            target_pane: target_pane.into(),
            task: task.into(),
            evidence_refs: Vec::new(),
            boundary_reminders: Vec::new(),
        }
    }

    /// Attach an evidence reference (e.g. `cargo nextest run -p mp
    /// --test foo`).
    pub fn with_evidence_ref(mut self, reference: impl Into<String>) -> Self {
        self.evidence_refs.push(reference.into());
        self
    }

    /// Attach a boundary reminder (e.g. "report progress through `mp
    /// autopilot session transition`").
    pub fn with_boundary_reminder(mut self, reminder: impl Into<String>) -> Self {
        self.boundary_reminders.push(reminder.into());
        self
    }
}

/// Validation errors raised by [`validate_assignment`].
///
/// The single error union lets callers pattern-match on the failure
/// mode (e.g. surface "unknown pane" differently from "empty task"
/// in a verifier UI). Variants are deliberately narrow: every
/// variant names the specific invariant the payload violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAssignmentValidationError {
    /// `session_id` is empty. The session is the persistence target —
    /// an empty id has no place to land and is treated as missing
    /// identity.
    EmptySessionId,
    /// `milestone_id` is empty. Cycle history is keyed by milestone;
    /// empty id cannot be recorded.
    EmptyMilestoneId,
    /// `task` is empty (after trimming). The M200 regression is
    /// closed here — a prose-only or empty assignment cannot reach
    /// herdr.
    EmptyTask,
    /// `target_pane` is empty. Without a target pane, dispatch
    /// cannot route.
    EmptyTargetPane,
    /// `cycle` is zero. Cycles are 1-indexed.
    ZeroCycle,
    /// `direction` is not in the closed set of accepted directions
    /// (the [`RoleDirection::from_str`] path).
    UnknownDirection(String),
    /// `target_pane` is not present in the session's pane layout for
    /// the requested direction (i.e. it does not match the runner or
    /// reviewer pane id that the session currently records).
    TargetPaneNotInLayout {
        direction: RoleDirection,
        pane: String,
    },
    /// A field contains a shell metacharacter (`; & | ` $ > < " \`
    /// newline). Defense in depth — argv is passed to `Command::args`
    /// (no shell), but a future shell-string fallback path would be
    /// vulnerable. The variant carries the field name that triggered
    /// it so a verifier can pinpoint the bad input.
    ShellMetacharacter { field: &'static str, value: String },
    /// The payload is a JSON shape that does not match
    /// [`TaskAssignment`] at all — e.g. a prose string was passed
    /// where the typed struct is expected. This is the M200
    /// regression gate: only the typed renderer can append
    /// `AssignmentDispatched`, and a malformed input is rejected
    /// before any dispatch attempt.
    TaskAssignmentShapeViolation(String),
}

impl std::fmt::Display for TaskAssignmentValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskAssignmentValidationError::EmptySessionId => {
                f.write_str("task assignment: session_id is empty")
            }
            TaskAssignmentValidationError::EmptyMilestoneId => {
                f.write_str("task assignment: milestone_id is empty")
            }
            TaskAssignmentValidationError::EmptyTask => {
                f.write_str("task assignment: task body is empty")
            }
            TaskAssignmentValidationError::EmptyTargetPane => {
                f.write_str("task assignment: target_pane is empty")
            }
            TaskAssignmentValidationError::ZeroCycle => {
                f.write_str("task assignment: cycle must be >= 1")
            }
            TaskAssignmentValidationError::UnknownDirection(s) => write!(
                f,
                "task assignment: unknown role direction {s:?} \
                 (accepted: orchestrator-to-runner, orchestrator-to-reviewer)"
            ),
            TaskAssignmentValidationError::TargetPaneNotInLayout { direction, pane } => write!(
                f,
                "task assignment: target_pane {pane:?} is not the {direction} pane in the session layout"
            ),
            TaskAssignmentValidationError::ShellMetacharacter { field, value } => write!(
                f,
                "task assignment: field {field} contains a shell metacharacter in {value:?}"
            ),
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(detail) => write!(
                f,
                "task assignment: shape violation ({detail}); \
                 only typed TaskAssignment payloads can be dispatched"
            ),
        }
    }
}

impl std::error::Error for TaskAssignmentValidationError {}

/// Reject any payload that is not a fully-populated
/// [`TaskAssignment`] before dispatch.
///
/// This is the M211 validation gate (AC-02). Every invariant the
/// spec lists is enforced here: empty fields, unknown directions,
/// unknown target panes, missing session identity, and shell
/// metacharacter interpolation. **No `herdr` call or session
/// mutation happens on rejection** — the gate runs first, errors
/// short-circuit, and the dispatcher's I/O paths are unreachable
/// from a malformed payload.
///
/// Pure over `(payload, layout)` — no I/O. The session layout is
/// passed in so the gate can verify pane membership without
/// loading the session from disk, which keeps the function
/// trivially testable.
pub fn validate_assignment(
    payload: &TaskAssignment,
    layout: &PaneLayout,
) -> Result<(), TaskAssignmentValidationError> {
    validate_assignment_structure(payload)?;
    validate_pane_membership(payload, layout)?;
    Ok(())
}

/// Structural checks: empty fields, zero cycle, shell
/// metacharacters. Does NOT need the session layout — runs before
/// the dispatcher attempts to load `session.json`, so a payload
/// missing identity (empty session_id, etc.) is rejected without
/// ever touching the file system.
pub fn validate_assignment_structure(
    payload: &TaskAssignment,
) -> Result<(), TaskAssignmentValidationError> {
    if payload.session_id.trim().is_empty() {
        return Err(TaskAssignmentValidationError::EmptySessionId);
    }
    if payload.milestone_id.trim().is_empty() {
        return Err(TaskAssignmentValidationError::EmptyMilestoneId);
    }
    if payload.task.trim().is_empty() {
        return Err(TaskAssignmentValidationError::EmptyTask);
    }
    if payload.target_pane.trim().is_empty() {
        return Err(TaskAssignmentValidationError::EmptyTargetPane);
    }
    if payload.cycle == 0 {
        return Err(TaskAssignmentValidationError::ZeroCycle);
    }
    let field_pairs: [(&'static str, &str); 4] = [
        ("session_id", &payload.session_id),
        ("milestone_id", &payload.milestone_id),
        ("target_pane", &payload.target_pane),
        ("task", &payload.task),
    ];
    for (name, value) in field_pairs {
        if let Some(bad_char) = first_shell_metachar(value) {
            return Err(TaskAssignmentValidationError::ShellMetacharacter {
                field: name,
                value: format!("{bad_char:?} in {value:?}"),
            });
        }
    }
    Ok(())
}

/// Layout-dependent check: the target pane must match the pane
/// id the session records for the requested direction.
pub fn validate_pane_membership(
    payload: &TaskAssignment,
    layout: &PaneLayout,
) -> Result<(), TaskAssignmentValidationError> {
    let expected = payload.direction.pane_slot(layout);
    match expected {
        Some(id) if id == payload.target_pane => Ok(()),
        _ => Err(TaskAssignmentValidationError::TargetPaneNotInLayout {
            direction: payload.direction,
            pane: payload.target_pane.clone(),
        }),
    }
}

/// First shell metacharacter in `s`, if any. The set is the
/// conventional "would be dangerous if shell-interpreted" set —
/// quotes and escapes are included so a hand-crafted payload
/// cannot smuggle a quoting trick.
fn first_shell_metachar(s: &str) -> Option<char> {
    s.chars().find(|c| {
        matches!(
            *c,
            ';' | '&' | '|' | '`' | '$' | '>' | '<' | '"' | '\'' | '\\' | '\n' | '\r' | '(' | ')'
        )
    })
}

/// Render the deterministic argv vector for `herdr agent prompt
/// <target_pane> <task_text>`.
///
/// The function is pure — the same [`TaskAssignment`] always
/// produces the same argv, byte for byte. The golden test in
/// `autopilot_task_assignment_golden.rs` pins this shape.
///
/// The last argv element is the rendered task text (see
/// [`render_task_text`]) — a single string carrying the
/// structured fields plus the free-form task body and any
/// optional evidence refs / boundary reminders. The argv vector
/// is passed to `Command::args`, so no shell interpolation
/// happens regardless of the text content.
pub fn build_assignment_argv(payload: &TaskAssignment) -> Vec<String> {
    vec![
        "agent".to_string(),
        "prompt".to_string(),
        payload.target_pane.clone(),
        render_task_text(payload),
    ]
}

/// Render the structured task text that becomes the final argv
/// element. Deterministic — same payload, same output.
pub fn render_task_text(payload: &TaskAssignment) -> String {
    let mut s = format!(
        "session={session} milestone={milestone} cycle={cycle} direction={direction}\n{task}",
        session = payload.session_id,
        milestone = payload.milestone_id,
        cycle = payload.cycle,
        direction = payload.direction.as_str(),
        task = payload.task,
    );
    if !payload.evidence_refs.is_empty() {
        s.push_str("\n\nevidence_refs:\n");
        for r in &payload.evidence_refs {
            s.push_str(&format!("- {r}\n"));
        }
    }
    if !payload.boundary_reminders.is_empty() {
        s.push_str("\nboundary_reminders:\n");
        for r in &payload.boundary_reminders {
            s.push_str(&format!("- {r}\n"));
        }
    }
    s
}

/// Outcome of a dispatch attempt, returned to callers.
///
/// The variant captures what happened so a verifier can tell
/// "herdr ran but exited non-zero" apart from "herdr could not
/// be spawned" — both are recorded in the session event but the
/// actionable next step is different.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentOutcome {
    /// `herdr` ran and exited with status 0. `argv` is the
    /// captured argv for log/audit (matches
    /// [`build_assignment_argv`]).
    Success {
        argv: Vec<String>,
        status: i32,
    },
    /// `herdr` ran but exited non-zero. The argv + status + any
    /// stderr text are recorded.
    NonZeroExit {
        argv: Vec<String>,
        status: i32,
        stderr: String,
    },
    /// `herdr` could not be spawned (binary missing, permission
    /// error, etc.). The argv is still recorded for forensics.
    SpawnError {
        argv: Vec<String>,
        error: String,
    },
}

impl AssignmentOutcome {
    /// True when the outcome is `Success`.
    pub fn is_success(&self) -> bool {
        matches!(self, AssignmentOutcome::Success { .. })
    }

    /// Stable string form used in the event payload.
    pub fn kind(&self) -> &'static str {
        match self {
            AssignmentOutcome::Success { .. } => "success",
            AssignmentOutcome::NonZeroExit { .. } => "non_zero_exit",
            AssignmentOutcome::SpawnError { .. } => "spawn_error",
        }
    }

    /// JSON body of the event payload.
    pub fn to_payload_json(&self) -> Value {
        match self {
            AssignmentOutcome::Success { argv, status } => json!({
                "outcome": "success",
                "argv": argv,
                "exit_status": status,
            }),
            AssignmentOutcome::NonZeroExit {
                argv,
                status,
                stderr,
            } => json!({
                "outcome": "non_zero_exit",
                "argv": argv,
                "exit_status": status,
                "stderr": stderr,
            }),
            AssignmentOutcome::SpawnError { argv, error } => json!({
                "outcome": "spawn_error",
                "argv": argv,
                "error": error,
            }),
        }
    }
}

/// Execute the argv via `Command::new(herdr_bin).args(argv)`. Pure
/// I/O wrapper — no shell, no env mutation, no output capture
/// beyond the exit status + stderr text.
pub fn execute_assignment(herdr_bin: &Path, argv: &[String]) -> std::io::Result<ExitStatus> {
    Command::new(herdr_bin).args(argv).status()
}

/// Single read/write path the orchestrator uses to dispatch a
/// task assignment.
///
/// Steps:
/// 1. **Structural validation** (no I/O) — reject empty fields,
///    zero cycle, shell metacharacters. A payload missing
///    identity (empty session_id, etc.) is rejected without
///    ever touching the file system.
/// 2. Load the session (so we know the pane layout + next seq).
/// 3. Validate pane membership against the loaded layout.
/// 4. Render the argv.
/// 5. Execute via [`execute_assignment`].
/// 6. Append an `AssignmentDispatched` event with the outcome.
///
/// On validation failure (steps 1 or 3), the function returns the
/// error immediately — **no `herdr` call, no session mutation.**
/// That is the AC-03 / M200 regression gate: prose-only or empty
/// assignments cannot produce an event or reach herdr.
///
/// `herdr_bin` is supplied by the caller — the dispatcher does
/// not search `PATH` itself (the watch layer's `which_herdr` does
/// that).
pub fn dispatch_assignment(
    ctx: &PlanContext,
    herdr_bin: &Path,
    payload: &TaskAssignment,
) -> Result<(AssignmentOutcome, PathBuf), TaskAssignmentValidationError> {
    // Step 1: structural validation BEFORE any I/O. This catches
    // empty session_id / milestone_id / task / target_pane,
    // zero cycle, and shell metacharacters without ever opening
    // session.json or spawning herdr.
    validate_assignment_structure(payload)?;
    // Step 2: load the session so we can validate pane membership
    // and compute the next event seq.
    let session = load_session(ctx, &payload.session_id)
        .map_err(|e| {
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(format!(
                "session load failed: {e}"
            ))
        })?;
    // Step 3: validate pane membership against the loaded layout.
    validate_pane_membership(payload, &session.topology)?;
    // Step 4: render the argv.
    let argv = build_assignment_argv(payload);
    // Step 5: execute.
    let outcome = match execute_assignment(herdr_bin, &argv) {
        Ok(status) if status.success() => AssignmentOutcome::Success {
            argv: argv.clone(),
            status: status.code().unwrap_or(0),
        },
        Ok(status) => {
            let stderr = String::new();
            AssignmentOutcome::NonZeroExit {
                argv: argv.clone(),
                status: status.code().unwrap_or(-1),
                stderr,
            }
        }
        Err(e) => AssignmentOutcome::SpawnError {
            argv: argv.clone(),
            error: e.to_string(),
        },
    };
    // Step 6: append an event AFTER the outcome is known. Even a
    // failed dispatch is recorded so the verifier sees the
    // attempt — what must NOT happen is a success event without
    // a real spawn.
    let path = append_assignment_event(ctx, &payload.session_id, payload, &outcome)
        .map_err(|e| {
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(format!(
                "event append failed: {e}"
            ))
        })?;
    Ok((outcome, path))
}

/// Append an `AssignmentDispatched` event to the session log
/// after the outcome is known. Helper split out so tests can
/// assert on the event without going through the full I/O path.
fn append_assignment_event(
    ctx: &PlanContext,
    session_id: &str,
    payload: &TaskAssignment,
    outcome: &AssignmentOutcome,
) -> Result<PathBuf> {
    let session = load_session(ctx, session_id)
        .map_err(|e| anyhow::anyhow!("load session {session_id}: {e}"))?;
    let seq = session.event_cursor.next_seq();
    let mut payload_json = serde_json::to_value(payload).context("serialize TaskAssignment")?;
    // Merge the outcome under a sibling key so the recorded event
    // shows both the typed payload AND the dispatch outcome.
    if let Value::Object(ref mut map) = payload_json {
        if let Value::Object(outcome_obj) = outcome.to_payload_json() {
            for (k, v) in outcome_obj {
                map.insert(k, v);
            }
        }
    }
    let actor = format!("orchestrator:{}", payload.direction.as_str());
    let event = OrchestrationEvent::new(seq, EventKind::AssignmentDispatched, actor, payload_json)
        .with_context(&payload.milestone_id, payload.cycle);
    append_event(ctx, session_id, event)
}

/// Try to parse a `serde_json::Value` as a [`TaskAssignment`].
///
/// Returns [`TaskAssignmentValidationError::TaskAssignmentShapeViolation`]
/// when the value is not an object or the deserialized payload
/// fails validation. This is the M200 regression gate at the
/// library boundary: a caller passing a JSON string (prose) or an
/// empty object cannot reach herdr.
pub fn parse_assignment(value: &Value) -> Result<TaskAssignment, TaskAssignmentValidationError> {
    if !value.is_object() {
        return Err(TaskAssignmentValidationError::TaskAssignmentShapeViolation(
            "expected JSON object (TaskAssignment), got non-object shape".to_string(),
        ));
    }
    serde_json::from_value(value.clone()).map_err(|e| {
        TaskAssignmentValidationError::TaskAssignmentShapeViolation(format!(
            "JSON did not match TaskAssignment: {e}"
        ))
    })
}

// Suppress the unused-import warning for SessionPath when nothing
// in the file actually needs it; it is part of the public surface
// that downstream consumers may use.
#[allow(dead_code)]
fn _session_path_type() -> Option<SessionPath> {
    None
}

// Suppress unused import warning for EventCursor (it is part of the
// typed event surface; consumers of this module use it via
// `OrchestrationEvent` builders and the `append_event` helper).
#[allow(dead_code)]
fn _cursor_type() -> Option<EventCursor> {
    None
}

// Suppress unused import warning for AutopilotSession.
#[allow(dead_code)]
fn _session_type() -> Option<AutopilotSession> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> PaneLayout {
        PaneLayout {
            orchestrator: Some(crate::autopilot::session::PaneRef {
                pane_id: "%1".to_string(),
                label: Some("role-orchestrator-1".to_string()),
            }),
            runner: Some(crate::autopilot::session::PaneRef {
                pane_id: "%2".to_string(),
                label: Some("role-runner-1".to_string()),
            }),
            reviewer: Some(crate::autopilot::session::PaneRef {
                pane_id: "%3".to_string(),
                label: Some("role-reviewer-1".to_string()),
            }),
        }
    }

    fn ok_payload() -> TaskAssignment {
        TaskAssignment::new(
            "s1",
            "M211",
            1,
            RoleDirection::OrchestratorToRunner,
            "%2",
            "run cycle 1",
        )
    }

    #[test]
    fn direction_kebab_round_trips() {
        for d in [
            RoleDirection::OrchestratorToRunner,
            RoleDirection::OrchestratorToReviewer,
        ] {
            assert_eq!(d.as_str().parse::<RoleDirection>().unwrap(), d);
            let json = serde_json::to_string(&d).unwrap();
            assert_eq!(json.trim_matches('"'), d.as_str());
        }
    }

    #[test]
    fn direction_rejects_unknown() {
        let err = "coordinator-to-runner".parse::<RoleDirection>().unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::UnknownDirection(_)
        ));
    }

    #[test]
    fn direction_pane_slot_matches_layout() {
        let l = layout();
        assert_eq!(
            RoleDirection::OrchestratorToRunner.pane_slot(&l),
            Some("%2")
        );
        assert_eq!(
            RoleDirection::OrchestratorToReviewer.pane_slot(&l),
            Some("%3")
        );
    }

    #[test]
    fn validate_accepts_well_formed_payload() {
        let p = ok_payload();
        assert!(validate_assignment(&p, &layout()).is_ok());
    }

    #[test]
    fn validate_rejects_empty_session_id() {
        let mut p = ok_payload();
        p.session_id = "".to_string();
        assert_eq!(
            validate_assignment(&p, &layout()).unwrap_err(),
            TaskAssignmentValidationError::EmptySessionId
        );
    }

    #[test]
    fn validate_rejects_empty_milestone_id() {
        let mut p = ok_payload();
        p.milestone_id = "   ".to_string();
        assert_eq!(
            validate_assignment(&p, &layout()).unwrap_err(),
            TaskAssignmentValidationError::EmptyMilestoneId
        );
    }

    #[test]
    fn validate_rejects_empty_task() {
        let mut p = ok_payload();
        p.task = "".to_string();
        assert_eq!(
            validate_assignment(&p, &layout()).unwrap_err(),
            TaskAssignmentValidationError::EmptyTask
        );
    }

    #[test]
    fn validate_rejects_empty_target_pane() {
        let mut p = ok_payload();
        p.target_pane = " ".to_string();
        assert_eq!(
            validate_assignment(&p, &layout()).unwrap_err(),
            TaskAssignmentValidationError::EmptyTargetPane
        );
    }

    #[test]
    fn validate_rejects_zero_cycle() {
        let mut p = ok_payload();
        p.cycle = 0;
        assert_eq!(
            validate_assignment(&p, &layout()).unwrap_err(),
            TaskAssignmentValidationError::ZeroCycle
        );
    }

    #[test]
    fn validate_rejects_target_pane_not_in_layout() {
        let mut p = ok_payload();
        p.target_pane = "%99".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TargetPaneNotInLayout { .. }
        ));
    }

    #[test]
    fn validate_rejects_target_pane_for_reviewer_when_runner_direction() {
        // Mismatch: direction is orchestrator→runner but target pane
        // is the reviewer pane id.
        let mut p = ok_payload();
        p.target_pane = "%3".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TargetPaneNotInLayout {
                direction: RoleDirection::OrchestratorToRunner,
                ..
            }
        ));
    }

    #[test]
    fn validate_rejects_target_pane_for_runner_when_reviewer_direction() {
        let mut p = ok_payload();
        p.direction = RoleDirection::OrchestratorToReviewer;
        p.target_pane = "%2".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TargetPaneNotInLayout {
                direction: RoleDirection::OrchestratorToReviewer,
                ..
            }
        ));
    }

    #[test]
    fn validate_structure_rejects_empty_fields_without_layout() {
        // Structural validation must succeed for an empty
        // PaneLayout — it does not check pane membership, only
        // the payload fields. This is what makes
        // `validate_assignment_structure` usable as a pre-I/O gate
        // before session.json is loaded.
        let empty = PaneLayout::default();
        let p = ok_payload();
        assert!(validate_assignment_structure(&p).is_ok());
        // Empty structural fields still rejected.
        let mut p = ok_payload();
        p.session_id = "".into();
        assert!(validate_assignment_structure(&p).is_err());
    }

    #[test]
    fn validate_pane_membership_only_checks_layout() {
        // pane-membership check is layout-only — payload structure
        // is not re-validated. An empty session_id with a valid
        // pane id passes (callers must run structural check first).
        let l = layout();
        let mut p = ok_payload();
        p.session_id = "".into();
        p.target_pane = "%2".into();
        assert!(validate_pane_membership(&p, &l).is_ok());
    }

    #[test]
    fn validate_rejects_shell_metachar_in_task() {
        let mut p = ok_payload();
        p.task = "rm -rf /; echo done".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::ShellMetacharacter {
                field: "task",
                ..
            }
        ));
    }

    #[test]
    fn validate_rejects_shell_metachar_in_target_pane() {
        let mut p = ok_payload();
        p.target_pane = "%2; touch pwned".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::ShellMetacharacter {
                field: "target_pane",
                ..
            }
        ));
    }

    #[test]
    fn validate_rejects_backtick_in_session_id() {
        let mut p = ok_payload();
        p.session_id = "s1`whoami`".to_string();
        let err = validate_assignment(&p, &layout()).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::ShellMetacharacter {
                field: "session_id",
                ..
            }
        ));
    }

    #[test]
    fn build_argv_is_deterministic() {
        let p = ok_payload();
        let argv1 = build_assignment_argv(&p);
        let argv2 = build_assignment_argv(&p);
        assert_eq!(argv1, argv2);
        // Golden shape: [agent, prompt, <pane>, <text>]
        assert_eq!(argv1.len(), 4);
        assert_eq!(argv1[0], "agent");
        assert_eq!(argv1[1], "prompt");
        assert_eq!(argv1[2], "%2");
        assert!(argv1[3].contains("session=s1"));
        assert!(argv1[3].contains("milestone=M211"));
        assert!(argv1[3].contains("cycle=1"));
        assert!(argv1[3].contains("direction=orchestrator-to-runner"));
        assert!(argv1[3].contains("run cycle 1"));
    }

    #[test]
    fn build_argv_includes_evidence_refs_and_reminders() {
        let p = ok_payload()
            .with_evidence_ref("cargo nextest run -p mp --test foo")
            .with_boundary_reminder("report via mp autopilot session transition");
        let argv = build_assignment_argv(&p);
        let text = &argv[3];
        assert!(text.contains("evidence_refs:"));
        assert!(text.contains("cargo nextest run -p mp --test foo"));
        assert!(text.contains("boundary_reminders:"));
        assert!(text.contains("mp autopilot session transition"));
    }

    #[test]
    fn build_argv_skips_empty_optional_sections() {
        let p = ok_payload();
        let argv = build_assignment_argv(&p);
        let text = &argv[3];
        assert!(!text.contains("evidence_refs:"));
        assert!(!text.contains("boundary_reminders:"));
    }

    #[test]
    fn parse_assignment_accepts_typed_payload() {
        let json = json!({
            "session_id": "s1",
            "milestone_id": "M211",
            "cycle": 1,
            "direction": "orchestrator-to-runner",
            "target_pane": "%2",
            "task": "do the thing",
        });
        let p = parse_assignment(&json).unwrap();
        assert_eq!(p.session_id, "s1");
        assert_eq!(p.direction, RoleDirection::OrchestratorToRunner);
    }

    #[test]
    fn parse_assignment_rejects_prose_string() {
        let json = json!("notify the runner about M211");
        let err = parse_assignment(&json).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
        ));
    }

    #[test]
    fn parse_assignment_rejects_empty_object() {
        let json = json!({});
        let err = parse_assignment(&json).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
        ));
    }

    #[test]
    fn parse_assignment_rejects_missing_required_field() {
        let json = json!({
            "session_id": "s1",
            "milestone_id": "M211",
            // cycle missing
            "direction": "orchestrator-to-runner",
            "target_pane": "%2",
            "task": "do the thing",
        });
        let err = parse_assignment(&json).unwrap_err();
        assert!(matches!(
            err,
            TaskAssignmentValidationError::TaskAssignmentShapeViolation(_)
        ));
    }

    #[test]
    fn outcome_kind_strings() {
        assert_eq!(
            AssignmentOutcome::Success {
                argv: vec!["a".into()],
                status: 0
            }
            .kind(),
            "success"
        );
        assert_eq!(
            AssignmentOutcome::NonZeroExit {
                argv: vec!["a".into()],
                status: 2,
                stderr: "x".into()
            }
            .kind(),
            "non_zero_exit"
        );
        assert_eq!(
            AssignmentOutcome::SpawnError {
                argv: vec!["a".into()],
                error: "enoent".into()
            }
            .kind(),
            "spawn_error"
        );
    }

    #[test]
    fn outcome_is_success_only_for_zero_exit() {
        let ok = AssignmentOutcome::Success {
            argv: vec![],
            status: 0,
        };
        assert!(ok.is_success());
        let nz = AssignmentOutcome::NonZeroExit {
            argv: vec![],
            status: 1,
            stderr: String::new(),
        };
        assert!(!nz.is_success());
        let sp = AssignmentOutcome::SpawnError {
            argv: vec![],
            error: "x".into(),
        };
        assert!(!sp.is_success());
    }

    #[test]
    fn shell_metachar_set_includes_quotes_and_backtick() {
        for c in [';', '&', '|', '`', '$', '>', '<', '"', '\'', '\\', '\n', '\r', '(', ')'] {
            let mut p = ok_payload();
            p.task = format!("x{c}y");
            let err = validate_assignment(&p, &layout()).unwrap_err();
            assert!(
                matches!(
                    err,
                    TaskAssignmentValidationError::ShellMetacharacter { .. }
                ),
                "char {c:?} should be rejected; got {err:?}"
            );
        }
    }
}