//! M207 / AC-06: typed role-state machine for autopilot sessions.
//!
//! Every role-state transition goes through [`transition`]. Direct
//! edits to `session.json` `role_state.*.state` are technically
//! possible on disk but the autopilot driver never does that —
//! transitions are gated by a small state machine that rejects
//! invalid moves, stamps the actor, and bumps
//! `last_state_change_at`.
//!
//! The set of valid transitions is intentionally tiny: agents don't
//! generally skip states, they move along the working lifecycle. New
//! states or transitions require a `schema_version` bump.

use serde::{Deserialize, Serialize};

use crate::autopilot::session::{AutopilotSession, RoleName};
use crate::store::now_rfc3339;

/// Closed set of role states. Mirrors the `role_state_record` enum
/// in the embedded schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleState {
    Idle,
    Starting,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl RoleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoleState::Idle => "idle",
            RoleState::Starting => "starting",
            RoleState::Working => "working",
            RoleState::Blocked => "blocked",
            RoleState::Done => "done",
            RoleState::Unknown => "unknown",
        }
    }
}

/// Per-role record (mirrors `role_state_record` in the schema).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleStateRecord {
    pub role: RoleName,
    pub state: RoleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_on: Option<crate::autopilot::session::WorkingOn>,
}

/// Outcome of a [`transition`] call.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionOutcome {
    /// Transition was applied. The new role-state record is included
    /// so callers can echo it in CLI output.
    Applied(RoleStateRecord),
    /// Transition was a no-op (already in `next`); record echoed.
    NoChange(RoleStateRecord),
}

impl TransitionOutcome {
    pub fn record(&self) -> &RoleStateRecord {
        match self {
            TransitionOutcome::Applied(r) | TransitionOutcome::NoChange(r) => r,
        }
    }

    pub fn was_applied(&self) -> bool {
        matches!(self, TransitionOutcome::Applied(_))
    }
}

/// Errors raised by invalid transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// State is unknown / not a real role.
    InvalidState(String),
    /// From → to pair is not in the transition table.
    InvalidTransition { from: RoleState, to: RoleState },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::InvalidState(s) => write!(f, "invalid role state {s:?}"),
            TransitionError::InvalidTransition { from, to } => {
                write!(f, "invalid transition {} -> {}", from.as_str(), to.as_str())
            }
        }
    }
}

impl std::str::FromStr for RoleState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "idle" => Ok(RoleState::Idle),
            "starting" => Ok(RoleState::Starting),
            "working" => Ok(RoleState::Working),
            "blocked" => Ok(RoleState::Blocked),
            "done" => Ok(RoleState::Done),
            "unknown" => Ok(RoleState::Unknown),
            other => Err(format!("unknown role state {other:?}")),
        }
    }
}

impl std::error::Error for TransitionError {}

/// Transition table. The mapping is intentional: an agent can
/// advance monotonically (`idle -> starting -> working -> done`),
/// reverse (`working -> blocked` for self-reported blockers),
/// resume (`blocked -> working`), and reset on completion
/// (`done -> idle`). Anything else is rejected — agents don't
/// jump from `idle` straight to `done` without `working` first.
pub fn is_valid(from: RoleState, to: RoleState) -> bool {
    use RoleState::*;
    matches!(
        (from, to),
        (Idle, Starting)
            | (Idle, Working)
            | (Idle, Blocked)
            | (Starting, Working)
            | (Starting, Blocked)
            | (Starting, Idle)
            | (Working, Done)
            | (Working, Blocked)
            | (Working, Working) // state-self: actor refresh, allowed
            | (Working, Idle)
            | (Blocked, Working)
            | (Blocked, Idle)
            | (Done, Idle)
            | (Done, Working)
            | (Unknown, Idle)
            | (Unknown, Starting)
            | (Unknown, Working)
    )
}

/// Apply a transition to the session. Mutates `session.role_state`
/// (and `session.last_state_change_at`) on success. The new record
/// echoes the role, state, `since = now`, and the actor.
///
/// `working_on` is optional — only relevant for transitions into
/// `working` (the agent is now driving a specific milestone + cycle).
/// For other targets, callers should pass `None`.
pub fn transition(
    session: &mut AutopilotSession,
    role: RoleName,
    next: RoleState,
    actor: &str,
    working_on: Option<crate::autopilot::session::WorkingOn>,
) -> Result<TransitionOutcome, TransitionError> {
    let current = session
        .role_state
        .as_ref()
        .and_then(|m| match role {
            RoleName::Orchestrator => m.orchestrator.clone(),
            RoleName::Runner => m.runner.clone(),
            RoleName::Reviewer => m.reviewer.clone(),
        })
        .map(|r| r.state)
        .unwrap_or(RoleState::Idle);

    if next == current && working_on.is_none() {
        // No-op: same state, no new working_on.
        let existing_since = session
            .role_state
            .as_ref()
            .and_then(|m| match role {
                RoleName::Orchestrator => m.orchestrator.as_ref().and_then(|r| r.since.clone()),
                RoleName::Runner => m.runner.as_ref().and_then(|r| r.since.clone()),
                RoleName::Reviewer => m.reviewer.as_ref().and_then(|r| r.since.clone()),
            });
        let record = RoleStateRecord {
            role,
            state: next,
            since: Some(existing_since.unwrap_or_else(now_rfc3339)),
            actor: Some(actor.to_string()),
            working_on,
        };
        return Ok(TransitionOutcome::NoChange(record));
    }

    if !is_valid(current, next) {
        return Err(TransitionError::InvalidTransition { from: current, to: next });
    }

    let now = now_rfc3339();
    let record = RoleStateRecord {
        role,
        state: next,
        since: Some(now.clone()),
        actor: Some(actor.to_string()),
        working_on,
    };

    // Ensure the role_state envelope exists.
    let map = session.role_state.get_or_insert_with(Default::default);
    match role {
        RoleName::Orchestrator => map.orchestrator = Some(record.clone()),
        RoleName::Runner => map.runner = Some(record.clone()),
        RoleName::Reviewer => map.reviewer = Some(record.clone()),
    }
    session.last_state_change_at = Some(now.clone());

    // While a role is in `working`, the session `working_on` mirrors
    // the role's current milestone/cycle. On `idle` / `done` we clear
    // it so the next note doesn't derive a stale cycle.
    if matches!(next, RoleState::Working) {
        session.working_on = record.working_on.clone();
    } else if matches!(next, RoleState::Idle | RoleState::Done) {
        session.working_on = None;
    }

    Ok(TransitionOutcome::Applied(record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::session::{AutopilotSession, RoleName};

    fn empty() -> AutopilotSession {
        AutopilotSession::blank("s1")
    }

    #[test]
    fn valid_transition_table_covers_happy_paths() {
        assert!(is_valid(RoleState::Idle, RoleState::Starting));
        assert!(is_valid(RoleState::Starting, RoleState::Working));
        assert!(is_valid(RoleState::Working, RoleState::Done));
        assert!(is_valid(RoleState::Working, RoleState::Blocked));
        assert!(is_valid(RoleState::Blocked, RoleState::Working));
        assert!(is_valid(RoleState::Done, RoleState::Idle));
    }

    #[test]
    fn invalid_transitions_are_rejected() {
        assert!(!is_valid(RoleState::Idle, RoleState::Done));
        assert!(!is_valid(RoleState::Done, RoleState::Starting));
        assert!(!is_valid(RoleState::Idle, RoleState::Unknown));
    }

    #[test]
    fn transition_stamps_since_and_actor() {
        let mut s = empty();
        let out = transition(&mut s, RoleName::Runner, RoleState::Starting, "test", None)
            .unwrap();
        assert!(out.was_applied());
        let record = out.record();
        assert_eq!(record.role, RoleName::Runner);
        assert_eq!(record.state, RoleState::Starting);
        assert_eq!(record.actor.as_deref(), Some("test"));
        assert!(record.since.is_some());
        assert!(s.last_state_change_at.as_deref().unwrap().contains('T'));
    }

    #[test]
    fn noop_transition_returns_no_change() {
        let mut s = empty();
        transition(&mut s, RoleName::Runner, RoleState::Starting, "test", None)
            .unwrap();
        let out = transition(&mut s, RoleName::Runner, RoleState::Starting, "test", None)
            .unwrap();
        assert!(!out.was_applied());
    }

    #[test]
    fn invalid_transition_returns_error() {
        let mut s = empty();
        // Idle -> Done is not in the table.
        let err = transition(&mut s, RoleName::Runner, RoleState::Done, "test", None)
            .unwrap_err();
        match err {
            TransitionError::InvalidTransition { from, to } => {
                assert_eq!(from, RoleState::Idle);
                assert_eq!(to, RoleState::Done);
            }
            other => panic!("expected InvalidTransition, got {other:?}"),
        }
    }

    #[test]
    fn working_transition_sets_session_working_on() {
        let mut s = empty();
        let wo = crate::autopilot::session::WorkingOn {
            milestone_id: "M207".into(),
            cycle: 1,
            role: Some(RoleName::Runner),
        };
        transition(&mut s, RoleName::Runner, RoleState::Working, "test", Some(wo.clone()))
            .unwrap();
        assert_eq!(s.working_on, Some(wo));
    }

    #[test]
    fn idle_transition_clears_session_working_on() {
        let mut s = empty();
        let wo = crate::autopilot::session::WorkingOn {
            milestone_id: "M207".into(),
            cycle: 1,
            role: Some(RoleName::Runner),
        };
        transition(&mut s, RoleName::Runner, RoleState::Working, "test", Some(wo))
            .unwrap();
        transition(&mut s, RoleName::Runner, RoleState::Idle, "test", None)
            .unwrap();
        assert!(s.working_on.is_none());
    }

    #[test]
    fn role_state_round_trips_via_serde() {
        for state in [
            RoleState::Idle,
            RoleState::Starting,
            RoleState::Working,
            RoleState::Blocked,
            RoleState::Done,
            RoleState::Unknown,
        ] {
            let s = serde_json::to_string(&state).unwrap();
            let back: RoleState = serde_json::from_str(&s).unwrap();
            assert_eq!(back, state);
        }
    }
}