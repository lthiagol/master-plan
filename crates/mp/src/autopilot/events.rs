//! M207: append-only event log for autopilot sessions.
//!
//! Every orchestration action (dispatch, transition, review, decision,
//! control, note, recovery) lands in `session.json` as an
//! [`OrchestrationEvent`] with a strictly-monotonic `seq`. The
//! [`EventCursor`] tracks the highest issued sequence number and is
//! the basis for crash recovery (a torn write is detected by an event
//! cursor that points past the surviving events).
//!
//! Events are *append-only*: no API exists to mutate or remove a
//! recorded event. Recovery uses the cursor + the surviving tail;
//! corruption is contained rather than erased.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Discriminator for the kinds of orchestration events recorded in
/// `session.json`. The set is closed; bumping it requires a
/// `schema_version` bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Dispatch,
    Transition,
    Review,
    Decision,
    Control,
    Note,
    Recovery,
    /// M211: a typed task assignment dispatched through herdr argv.
    /// Appended only by [`crate::autopilot::task_assign`] after the
    /// herdr spawn outcome is known — a prose-only or empty
    /// assignment cannot reach this event (it is rejected upstream
    /// with [`crate::autopilot::task_assign::TaskAssignmentShapeViolation`]).
    AssignmentDispatched,
}

impl EventKind {
    /// Stable string form (snake_case). Matches what serde emits.
    pub const fn as_str(self) -> &'static str {
        match self {
            EventKind::Dispatch => "dispatch",
            EventKind::Transition => "transition",
            EventKind::Review => "review",
            EventKind::Decision => "decision",
            EventKind::Control => "control",
            EventKind::Note => "note",
            EventKind::Recovery => "recovery",
            EventKind::AssignmentDispatched => "assignment_dispatched",
        }
    }
}

/// A single append-only event.
///
/// `seq` is assigned at append time and is strictly monotonic within
/// a session. The [`EventCursor`] on the parent session holds the
/// highest seq issued; recovery uses (cursor, surviving_events) to
/// detect torn writes without trusting partial files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationEvent {
    /// Monotonic sequence number within the session.
    pub seq: u64,
    /// What kind of event this is. Discriminator for the `payload`.
    pub kind: EventKind,
    /// RFC3339 timestamp the event was recorded.
    pub at: String,
    /// Who emitted the event (e.g. "runner:M207", "coordinator", or
    /// a herdr pane id). The schema does not enforce a shape — the
    /// value is informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Optional session id mirror. Normally redundant with the parent
    /// session, but lets events be filtered in cross-session tooling
    /// without re-reading the parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Role context for the event (dispatch / transition always set
    /// it; note / recovery may leave it unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<crate::autopilot::RoleName>,
    /// Optional milestone id the event pertains to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<String>,
    /// Optional cycle number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u32>,
    /// Event payload (kind-specific). The schema keeps this open —
    /// each `kind` documents its expected shape in code (see the
    /// helpers below).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

impl OrchestrationEvent {
    /// Build a fresh event with `seq = cursor + 1` and the supplied
    /// kind / actor / payload. `at` is set to `now` (RFC3339).
    pub fn new(seq: u64, kind: EventKind, actor: impl Into<String>, payload: Value) -> Self {
        Self {
            seq,
            kind,
            at: crate::store::now_rfc3339(),
            actor: Some(actor.into()),
            session_id: None,
            role: None,
            milestone_id: None,
            cycle: None,
            payload: Some(payload),
        }
    }

    /// Builder-style attach of milestone id + cycle.
    pub fn with_context(mut self, milestone_id: impl Into<String>, cycle: u32) -> Self {
        self.milestone_id = Some(milestone_id.into());
        self.cycle = Some(cycle);
        self
    }

    /// Builder-style attach of role.
    pub fn with_role(mut self, role: crate::autopilot::RoleName) -> Self {
        self.role = Some(role);
        self
    }
}

/// Cursor into the event log. Tracks the highest seq issued so a
/// crash mid-append can be detected by `(cursor, surviving_events)`
/// without consulting anything else.
///
/// The cursor is bumped before the event is appended; on recovery,
/// any event with `seq > cursor` is treated as a torn tail and
/// discarded. The cursor itself is updated atomically with the
/// `events` array via [`crate::store::atomic_write`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventCursor {
    /// Highest seq issued so far. Zero means the log is empty.
    pub last_seq: u64,
}

impl EventCursor {
    pub fn new() -> Self {
        Self { last_seq: 0 }
    }

    /// Next sequence number to assign. Always `last_seq + 1`; once
    /// issued it must be persisted.
    pub fn next_seq(&self) -> u64 {
        self.last_seq + 1
    }

    /// Bump to the supplied seq. Returns an error if the supplied
    /// seq would regress (monotonic invariant).
    pub fn advance_to(&mut self, seq: u64) -> Result<(), CursorRegression> {
        if seq <= self.last_seq {
            return Err(CursorRegression {
                current: self.last_seq,
                attempted: seq,
            });
        }
        self.last_seq = seq;
        Ok(())
    }

    /// Reconcile against the surviving events. Returns the seq we
    /// will trust going forward (max of current cursor and
    /// surviving-events tail).
    pub fn reconcile(&mut self, surviving: &[OrchestrationEvent]) {
        if let Some(max) = surviving.iter().map(|e| e.seq).max() {
            if max > self.last_seq {
                self.last_seq = max;
            }
        }
    }
}

/// Error returned by [`EventCursor::advance_to`] when the caller
/// tries to move the cursor backwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRegression {
    pub current: u64,
    pub attempted: u64,
}

impl std::fmt::Display for CursorRegression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "event cursor regression: current={} attempted={}",
            self.current, self.attempted
        )
    }
}

impl std::error::Error for CursorRegression {}

/// Group events by kind for read-side projections.
pub fn events_by_kind(events: &[OrchestrationEvent]) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for event in events {
        *out.entry(event.kind.as_str()).or_insert(0) += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_kind_round_trips_via_serde() {
        for kind in [
            EventKind::Dispatch,
            EventKind::Transition,
            EventKind::Review,
            EventKind::Decision,
            EventKind::Control,
            EventKind::Note,
            EventKind::Recovery,
            EventKind::AssignmentDispatched,
        ] {
            let s = serde_json::to_string(&kind).unwrap();
            let back: EventKind = serde_json::from_str(&s).unwrap();
            assert_eq!(back, kind);
            assert_eq!(kind.as_str(), s.trim_matches('"'));
        }
    }

    #[test]
    fn cursor_starts_at_zero_and_advances_monotonically() {
        let mut c = EventCursor::new();
        assert_eq!(c.next_seq(), 1);
        c.advance_to(1).unwrap();
        assert_eq!(c.last_seq, 1);
        assert_eq!(c.next_seq(), 2);
        c.advance_to(2).unwrap();
        assert_eq!(c.last_seq, 2);
    }

    #[test]
    fn cursor_rejects_regression() {
        let mut c = EventCursor::new();
        c.advance_to(5).unwrap();
        let err = c.advance_to(3).unwrap_err();
        assert_eq!(err.current, 5);
        assert_eq!(err.attempted, 3);
    }

    #[test]
    fn cursor_reconciles_against_surviving_events() {
        let mut c = EventCursor::new();
        // Cursor was at 2 before the crash; survivors include seq=3
        // because the rename landed before the crash but the new
        // session.json was read in. Reconcile must trust the
        // surviving tail.
        c.advance_to(2).unwrap();
        let survivors = vec![
            OrchestrationEvent::new(1, EventKind::Dispatch, "test", json!({})),
            OrchestrationEvent::new(3, EventKind::Transition, "test", json!({})),
        ];
        c.reconcile(&survivors);
        assert_eq!(c.last_seq, 3);
    }

    #[test]
    fn event_with_context_attaches_milestone_and_cycle() {
        let e = OrchestrationEvent::new(1, EventKind::Dispatch, "test", json!({}))
            .with_context("M207", 1);
        assert_eq!(e.milestone_id.as_deref(), Some("M207"));
        assert_eq!(e.cycle, Some(1));
    }

    #[test]
    fn events_by_kind_groups_correctly() {
        let events = vec![
            OrchestrationEvent::new(1, EventKind::Dispatch, "a", json!({})),
            OrchestrationEvent::new(2, EventKind::Transition, "a", json!({})),
            OrchestrationEvent::new(3, EventKind::Dispatch, "a", json!({})),
            OrchestrationEvent::new(4, EventKind::Note, "a", json!({})),
        ];
        let grouped = events_by_kind(&events);
        assert_eq!(grouped.get("dispatch").copied(), Some(2));
        assert_eq!(grouped.get("transition").copied(), Some(1));
        assert_eq!(grouped.get("note").copied(), Some(1));
    }
}
