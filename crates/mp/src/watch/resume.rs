//! M152 S2: pane reconciliation — query herdr agent list, match
//! `role-runner-N` / `role-coordinator-N` labels against recorded
//! state, classify each role's pane as `Live` / `Dead` / `Missing`.
//!
//! This is the pure-function core `--resume` builds on top of:
//! the resume path decides whether to re-attach (Live) or
//! re-spawn (Dead / Missing) per role. The classifier is decoupled
//! from I/O so tests can drive it with synthesized JSON fixtures
//! (see `crates/mp/tests/watch_resume.rs`).
//!
//! ## Layering
//!
//! - [`PaneStatus`] — the per-role classification result.
//! - [`Reconciliation`] — both roles' statuses at once.
//! - [`reconcile`] — pure function: state file (or None) +
//!   herdr-list JSON → Reconciliation. No subprocesses, no I/O.
//!
//! ## Where the live list comes from
//!
//! [`crate::watch::herdr::list_panes`] runs `herdr agent list
//! --format json` and returns the raw JSON. The same JSON shape
//! the prior-step [`find_existing_pane`] helper already parses
//! (`{"agents":[...]}` envelope or bare `[...]`); reconciliation
//! reuses that envelope knowledge so the resume path stays
//! consistent with the existing `ensure_pane` reuse logic.

use serde::Serialize;

use crate::watch::herdr::{find_existing_pane, pane_label_for, Role, DEFAULT_PANE_N};
use crate::watch::state::WatchState;

/// Per-role classification of the pane state at resume time.
///
/// The variant captures both *whether the pane exists* and, when
/// it does, what herdr says about it (so the resume path can
/// decide whether to send a fresh prompt or just observe a
/// mid-execute pane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PaneStatus {
    /// Live pane matched in herdr's agent list. `pane_id` is the
    /// target handle herdr returned; `status` is the latest
    /// `agent-status` known at reconcile time (often `None` when
    /// the resume path doesn't pay the extra round-trip cost).
    Live {
        pane_id: String,
        status: Option<String>,
    },
    /// Pane was tracked in `watch.state.json` but does not appear
    /// in the live herdr list. Resume must re-spawn. Carries the
    /// stable label so the spawn path doesn't have to recompute it.
    Dead { label: String },
    /// No recorded pane for this role. Either a fresh project
    /// (state file absent) or a state file that pre-dates this
    /// milestone's first pane. Resume must spawn.
    Missing,
}

impl PaneStatus {
    /// True when a pane is ready to be addressed without spawning.
    pub fn is_live(&self) -> bool {
        matches!(self, PaneStatus::Live { .. })
    }

    /// True when the resume path must call `ensure_pane` (and
    /// therefore spawn a new pane if herdr doesn't already have
    /// one). Used by the double-spawn-default and force-override
    /// path to decide the next action.
    pub fn needs_spawn(&self) -> bool {
        matches!(self, PaneStatus::Dead { .. } | PaneStatus::Missing)
    }
}

/// Both roles' classifications at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reconciliation {
    pub runner: PaneStatus,
    pub coordinator: PaneStatus,
}

impl Reconciliation {
    /// True if any role needs a pane spawned (Dead or Missing).
    /// Used by `--resume` to know whether to call `ensure_pane`.
    pub fn any_needs_spawn(&self) -> bool {
        self.runner.needs_spawn() || self.coordinator.needs_spawn()
    }
}

/// Pure reconcile: classify the runner + coordinator panes from a
/// herdr-list payload and the (optional) recorded watch state.
///
/// The pane counter defaults to [`DEFAULT_PANE_N`] (1) — the
/// sequencer-owned counter for "this is the Nth pane this role has
/// owned this session". Future enhancements can thread an explicit
/// `pane_n` through if a multi-counter scenario lands.
///
/// `find_existing_pane` already handles both `{"agents": [...]}` and
/// bare `[...]` envelopes from herdr, so the resume path stays
/// resilient across herdr versions.
pub fn reconcile(state: Option<&WatchState>, herdr_list_json: &str) -> Reconciliation {
    Reconciliation {
        runner: classify_role(
            state,
            herdr_list_json,
            Role::Runner,
            pane_label_for(Role::Runner, DEFAULT_PANE_N),
        ),
        coordinator: classify_role(
            state,
            herdr_list_json,
            Role::Coordinator,
            pane_label_for(Role::Coordinator, DEFAULT_PANE_N),
        ),
    }
}

fn classify_role(
    state: Option<&WatchState>,
    herdr_list_json: &str,
    role: Role,
    label: String,
) -> PaneStatus {
    // Step 1: is the pane alive in herdr's agent list now?
    if let Some(pane_id) = find_existing_pane(&label, herdr_list_json) {
        return PaneStatus::Live {
            pane_id,
            status: None, // status is read on demand by read_agent_status
        };
    }
    // Step 2: was it ever recorded? If yes → Dead (re-spawn).
    if let Some(state) = state {
        if state.pane_for(role).is_some() {
            return PaneStatus::Dead { label };
        }
    }
    // Step 3: never tracked → Missing (first spawn).
    PaneStatus::Missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::state::{PaneState, WatchState};

    fn env(label: &str, pane_id: &str) -> String {
        format!(r#"{{"agents":[{{"name":"{label}","pane_id":"{pane_id}"}}]}}"#)
    }

    #[test]
    fn empty_herdr_list_with_no_state_is_missing_both_roles() {
        let r = reconcile(None, r#"{"agents":[]}"#);
        assert_eq!(r.runner, PaneStatus::Missing);
        assert_eq!(r.coordinator, PaneStatus::Missing);
        assert!(r.any_needs_spawn());
    }

    #[test]
    fn herdr_list_with_runner_alive_classifies_runner_live() {
        let r = reconcile(None, &env("role-runner-1", "%5"));
        match r.runner {
            PaneStatus::Live { pane_id, .. } => assert_eq!(pane_id, "%5"),
            other => panic!("expected Live, got {other:?}"),
        }
        assert_eq!(r.coordinator, PaneStatus::Missing);
    }

    #[test]
    fn herdr_list_with_coordinator_alive_classifies_coordinator_live() {
        let r = reconcile(None, &env("role-coordinator-1", "%7"));
        assert_eq!(r.runner, PaneStatus::Missing);
        match r.coordinator {
            PaneStatus::Live { pane_id, .. } => assert_eq!(pane_id, "%7"),
            other => panic!("expected Live, got {other:?}"),
        }
    }

    #[test]
    fn both_alive_is_no_respawn_needed() {
        let r = reconcile(
            None,
            // `format!` here is intentional: `{{` and `}}` are the
            // Rust `format!` escape for `{` and `}` literals in
            // raw-string heredocs, so the resulting JSON string is
            // `{"agents":[{...}, {...}]}` (real JSON, not Rust's
            // doubled braces). clippy's "useless format" lint
            // recommends `.to_string()` on the literal — suppressed
            // inline because the escape processing IS load-bearing.
            #[allow(clippy::useless_format)]
            &format!(
                r#"{{"agents":[{{"name":"role-runner-1","pane_id":"%5"}},{{"name":"role-coordinator-1","pane_id":"%7"}}]}}"#
            ),
        );
        assert!(!r.any_needs_spawn());
        assert!(matches!(r.runner, PaneStatus::Live { .. }));
        assert!(matches!(r.coordinator, PaneStatus::Live { .. }));
    }

    #[test]
    fn recorded_pane_absent_from_herdr_list_is_dead() {
        let mut s = WatchState::fresh(&[]);
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%5".into(),
            spawned_at: "t".into(),
            last_status: None,
        });
        // State says runner is alive; herdr says no.
        let r = reconcile(Some(&s), r#"{"agents":[]}"#);
        match r.runner {
            PaneStatus::Dead { label } => {
                assert_eq!(label, "role-runner-1");
            }
            other => panic!("expected Dead, got {other:?}"),
        }
    }

    #[test]
    fn recorded_pane_present_in_herdr_list_is_live_with_id_from_herdr() {
        // When both the state and herdr list are populated, herdr is
        // authoritative (the pane may have been recreated with a
        // fresh id since the state was written).
        let mut s = WatchState::fresh(&[]);
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%OLD".into(),
            spawned_at: "t".into(),
            last_status: None,
        });
        let r = reconcile(Some(&s), &env("role-runner-1", "%NEW"));
        match r.runner {
            PaneStatus::Live { pane_id, .. } => assert_eq!(pane_id, "%NEW"),
            other => panic!("expected Live, got {other:?}"),
        }
    }

    #[test]
    fn herdr_list_with_bare_array_envelope_also_resolves() {
        // find_existing_pane handles both shapes; resume inherits
        // the same resilience.
        let raw = r#"[{"label":"role-runner-1","target":"%9"}]"#;
        let r = reconcile(None, raw);
        match r.runner {
            PaneStatus::Live { pane_id, .. } => assert_eq!(pane_id, "%9"),
            other => panic!("expected Live, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_herdr_list_classifies_everything_as_missing() {
        // A herdr list that doesn't parse must NOT panic — the
        // resume path falls through to "no live pane known" for
        // every role.
        let r = reconcile(None, "not json");
        assert_eq!(r.runner, PaneStatus::Missing);
        assert_eq!(r.coordinator, PaneStatus::Missing);
    }

    #[test]
    fn dead_classifier_carries_label_for_spawn_path() {
        // The spawn path should not need to recompute the label;
        // Dead carries it explicitly so callers can rely on a
        // single source of truth (pane_label_for).
        let mut s = WatchState::fresh(&[]);
        s.upsert_pane(PaneState {
            role: Role::Coordinator,
            label: "role-coordinator-1".into(),
            pane_id: "%7".into(),
            spawned_at: "t".into(),
            last_status: None,
        });
        let r = reconcile(Some(&s), r#"{"agents":[]}"#);
        assert!(matches!(
            r.coordinator,
            PaneStatus::Dead { ref label } if label == "role-coordinator-1"
        ));
    }

    #[test]
    fn needs_spawn_and_is_live_are_disjoint() {
        let cases = [
            PaneStatus::Missing,
            PaneStatus::Dead { label: "x".into() },
            PaneStatus::Live {
                pane_id: "%5".into(),
                status: None,
            },
        ];
        for c in &cases {
            assert!(
                !(c.needs_spawn() && c.is_live()),
                "{c:?} cannot need spawn AND be live"
            );
        }
        // Live: not needs_spawn, is_live.
        // Dead/Missing: needs_spawn, not is_live.
        match cases[2] {
            PaneStatus::Live { .. } => {
                assert!(!cases[2].needs_spawn());
                assert!(cases[2].is_live());
            }
            _ => unreachable!(),
        }
        match cases[0] {
            PaneStatus::Missing => {
                assert!(cases[0].needs_spawn());
                assert!(!cases[0].is_live());
            }
            _ => unreachable!(),
        }
        match cases[1] {
            PaneStatus::Dead { .. } => {
                assert!(cases[1].needs_spawn());
                assert!(!cases[1].is_live());
            }
            _ => unreachable!(),
        }
    }
}
