//! M207 / AC-04: typed runner notes + cycle derivation.
//!
//! A [`RunnerNote`] is the typed record a runner leaves for the
//! reviewer. Each note carries a closed `kind` enum, free-form body,
//! and a cycle. The cycle is **required** at the API surface —
//! `mp autopilot note` either accepts `--cycle` or derives it from the
//! session's active `working_on`. If neither is available, the note is
//! rejected (no implicit cycle 0 / cycle 1 guess).
//!
//! ## Derivation rules
//!
//! 1. If `--cycle` is supplied, that value wins.
//! 2. Else if `session.working_on` is set (single in-flight milestone
//!    with a non-zero cycle), that cycle is used.
//! 3. Else if the queue has exactly one in-progress item with a
//!    non-zero cycle, that cycle is used.
//! 4. Else reject with [`NoteError::AmbiguousCycle`].

use serde::{Deserialize, Serialize};

use crate::autopilot::session::AutopilotSession;

/// Closed set of note kinds. New kinds require a schema_version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Info,
    Warn,
    Blocker,
    Decision,
    Reminder,
    System,
}

impl NoteKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            NoteKind::Info => "info",
            NoteKind::Warn => "warn",
            NoteKind::Blocker => "blocker",
            NoteKind::Decision => "decision",
            NoteKind::Reminder => "reminder",
            NoteKind::System => "system",
        }
    }
}

/// A typed runner note. Mirrors the `runner_note` schema entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunnerNote {
    pub kind: NoteKind,
    pub body: String,
    /// Cycle the note belongs to. Required (or derivable — see
    /// [`derive_cycle`]).
    pub cycle: u32,
    /// Optional milestone id (defaults to the in-flight milestone
    /// at insertion time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<String>,
    /// RFC3339 timestamp. Set at insertion.
    pub timestamp: String,
}

/// Errors raised by note insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteError {
    /// Neither --cycle nor an unambiguous session context was
    /// available. Caller must either re-run with `--cycle` or set
    /// `working_on` / ensure the queue has a single in-progress item.
    AmbiguousCycle,
    /// The supplied cycle was zero (cycles are 1-indexed).
    ZeroCycle,
    /// The supplied body was empty after trimming.
    EmptyBody,
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoteError::AmbiguousCycle => f.write_str(
                "note requires --cycle or an unambiguous session context (working_on set or exactly one in-progress queue item)",
            ),
            NoteError::ZeroCycle => f.write_str("cycle must be >= 1 (cycles are 1-indexed)"),
            NoteError::EmptyBody => f.write_str("note body must not be empty"),
        }
    }
}

impl std::error::Error for NoteError {}

/// Resolve the cycle for a new note given the session context.
///
/// Returns `Ok(None)` only when [`NoteError::AmbiguousCycle`] would
/// apply — callers should treat that as the rejection path.
pub fn derive_cycle(
    session: &AutopilotSession,
    explicit_cycle: Option<u32>,
) -> Result<u32, NoteError> {
    if let Some(0) = explicit_cycle {
        return Err(NoteError::ZeroCycle);
    }
    if let Some(cycle) = explicit_cycle {
        return Ok(cycle);
    }
    if let Some(working) = &session.working_on {
        if working.cycle >= 1 {
            return Ok(working.cycle);
        }
    }
    let in_progress: Vec<u32> = session
        .queue
        .iter()
        .filter(|item| matches!(item.stage.as_str(), "in-progress" | "executed"))
        .map(|item| item.cycle)
        .filter(|c| *c >= 1)
        .collect();
    if in_progress.len() == 1 {
        return Ok(in_progress[0]);
    }
    Err(NoteError::AmbiguousCycle)
}

/// Build a fresh note with `timestamp = now` and the supplied kind /
/// body / cycle. `milestone_id` is set to the in-flight milestone if
/// the caller did not pass one explicitly.
pub fn build_note(
    session: &AutopilotSession,
    kind: NoteKind,
    body: &str,
    explicit_cycle: Option<u32>,
    explicit_milestone: Option<&str>,
) -> Result<RunnerNote, NoteError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(NoteError::EmptyBody);
    }
    let cycle = derive_cycle(session, explicit_cycle)?;
    let milestone_id = explicit_milestone
        .map(str::to_string)
        .or_else(|| session.working_on.as_ref().map(|w| w.milestone_id.clone()))
        .or_else(|| {
            session
                .queue
                .iter()
                .find(|item| matches!(item.stage.as_str(), "in-progress" | "executed"))
                .map(|item| item.milestone_id.clone())
        });
    Ok(RunnerNote {
        kind,
        body: body.to_string(),
        cycle,
        milestone_id,
        timestamp: crate::store::now_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::session::{AutopilotSession, QueueItem, RoleName, Stage, WorkingOn};

    fn empty_session() -> AutopilotSession {
        AutopilotSession::blank("s1")
    }

    fn in_progress_session(cycle: u32) -> AutopilotSession {
        let mut s = empty_session();
        s.queue.push(QueueItem {
            milestone_id: "207".to_string(),
            stage: Stage::InProgress,
            cycle,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: None,
        });
        s
    }

    #[test]
    fn explicit_cycle_wins_over_session_context() {
        let s = in_progress_session(2);
        let note = build_note(&s, NoteKind::Info, "hello", Some(7), None).unwrap();
        assert_eq!(note.cycle, 7);
    }

    #[test]
    fn derive_cycle_from_working_on() {
        let mut s = empty_session();
        s.working_on = Some(WorkingOn {
            milestone_id: "M207".to_string(),
            cycle: 3,
            role: Some(RoleName::Runner),
        });
        assert_eq!(derive_cycle(&s, None).unwrap(), 3);
    }

    #[test]
    fn derive_cycle_from_single_in_progress_queue_item() {
        let s = in_progress_session(5);
        assert_eq!(derive_cycle(&s, None).unwrap(), 5);
    }

    #[test]
    fn derive_cycle_rejects_when_no_context() {
        let s = empty_session();
        let err = derive_cycle(&s, None).unwrap_err();
        assert_eq!(err, NoteError::AmbiguousCycle);
    }

    #[test]
    fn derive_cycle_rejects_ambiguous_multiple_in_progress() {
        let mut s = empty_session();
        s.queue.push(QueueItem {
            milestone_id: "01".to_string(),
            stage: Stage::InProgress,
            cycle: 2,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: None,
        });
        s.queue.push(QueueItem {
            milestone_id: "02".to_string(),
            stage: Stage::InProgress,
            cycle: 3,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: None,
        });
        let err = derive_cycle(&s, None).unwrap_err();
        assert_eq!(err, NoteError::AmbiguousCycle);
    }

    #[test]
    fn derive_cycle_rejects_zero_explicit() {
        let s = empty_session();
        let err = derive_cycle(&s, Some(0)).unwrap_err();
        assert_eq!(err, NoteError::ZeroCycle);
    }

    #[test]
    fn build_note_rejects_empty_body() {
        let s = in_progress_session(1);
        let err = build_note(&s, NoteKind::Info, "   ", Some(1), None).unwrap_err();
        assert_eq!(err, NoteError::EmptyBody);
    }

    #[test]
    fn build_note_attaches_milestone_id_from_session() {
        let mut s = in_progress_session(1);
        s.queue[0].milestone_id = "207".to_string();
        let note = build_note(&s, NoteKind::Warn, "watch out", Some(1), None).unwrap();
        assert_eq!(note.milestone_id.as_deref(), Some("207"));
    }

    #[test]
    fn build_note_attaches_milestone_from_working_on_over_queue() {
        let mut s = in_progress_session(1);
        s.queue[0].milestone_id = "207".to_string();
        s.working_on = Some(WorkingOn {
            milestone_id: "999".to_string(),
            cycle: 1,
            role: Some(RoleName::Runner),
        });
        let note = build_note(&s, NoteKind::Info, "x", Some(1), None).unwrap();
        assert_eq!(note.milestone_id.as_deref(), Some("999"));
    }

    #[test]
    fn note_kind_round_trips_via_serde() {
        for kind in [
            NoteKind::Info,
            NoteKind::Warn,
            NoteKind::Blocker,
            NoteKind::Decision,
            NoteKind::Reminder,
            NoteKind::System,
        ] {
            let s = serde_json::to_string(&kind).unwrap();
            let back: NoteKind = serde_json::from_str(&s).unwrap();
            assert_eq!(back, kind);
            assert_eq!(kind.as_str(), s.trim_matches('"'));
        }
    }
}