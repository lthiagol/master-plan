//! M209 / AC-01: the three-role autopilot model with topology-based
//! pane placement.
//!
//! This module is the typed foundation for M209's scope:
//!
//! - [`Role`] — the three autopilot roles (orchestrator, runner,
//!   reviewer). Distinct from `crate::watch::herdr::Role`, the legacy
//!   two-role model that mp watch still uses; the migration lives in
//!   A2 (out of scope here).
//! - [`Topology`] — the one / two / three-pane pane count that the
//!   session declares.
//! - [`role_pane_slots`] — pure function: maps a [`Topology`] to the
//!   list of role-sets per pane (a slot is the set of roles that share
//!   a single pane in that topology).
//!
//! ## Why a separate module
//!
//! M207's `session.rs` ships a per-session `Topology` struct (per-role
//! `PaneRef`s) and a `RoleName` enum; that pair represents *what is
//! currently saved on disk*. M209 introduces the canonical [`Role`]
//! enum and the [`Topology`] count enum used by the rest of the code
//! path. To avoid name collisions and keep a single source of truth,
//! the M207 per-session struct is renamed to [`crate::autopilot::
//! session::PaneLayout`]; callers that previously read
//! `session.json.topology.orchestrator` now read
//! `session.topology.orchestrator` against the renamed struct (the
//! on-disk shape is unchanged).
//!
//! ## Resolution chain
//!
//! Every other module that needs a per-role config goes through
//! [`resolve_role_config`] (S2) — never reads the session/config
//! directly. Topology-driven decision-matrix tightening lives in
//! [`topology_policy`] (S4).

use std::fmt;

use serde::{Deserialize, Serialize};

/// The three autopilot roles that drive a session's cycle.
///
/// Each role has its own slot in the pane topology ([`role_pane_slots`])
/// and its own config ([`resolve_role_config`]). The legacy
/// `coordinator` role from M149 / [`crate::watch::herdr::Role`] has no
/// direct mapping here — its work is split between `Orchestrator`
/// (cycle decisions) and `Reviewer` (independent verification).
///
/// Serialized as a kebab-case string so `autopilot.roles.<role>.*`
/// config paths and JSON round-trips align with the rest of mp's
/// dotted-key convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Orchestrator,
    Runner,
    Reviewer,
}

impl Role {
    /// Stable kebab-case label (matches the serde representation).
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Orchestrator => "orchestrator",
            Role::Runner => "runner",
            Role::Reviewer => "reviewer",
        }
    }

    /// All three variants in canonical declaration order. Useful when
    /// iterating / exhausting without missing a variant.
    pub const ALL: [Role; 3] = [Role::Orchestrator, Role::Runner, Role::Reviewer];
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "orchestrator" => Ok(Role::Orchestrator),
            "runner" => Ok(Role::Runner),
            "reviewer" => Ok(Role::Reviewer),
            other => Err(format!("unknown autopilot role {other:?}")),
        }
    }
}

/// One slot per pane. A slot is the *set* of [`Role`]s that share a
/// single pane: in 1-pane topology every role lands in the single pane
/// (one slot of three roles); in 3-pane topology each role is alone in
/// its pane (three slots of one role each).
pub type RoleSlot = Vec<Role>;

/// Pane-slot assignment for a [`Topology`]. Index = pane ordinal; slot
/// = roles sharing that pane.
pub type PaneSlots = Vec<RoleSlot>;

/// The pane count declared by a session.
///
/// Serialized kebab-case so the CLI surface
/// (`autopilot.topology = three-agent` in config.json) matches the
/// Rust enum naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Topology {
    /// Single pane — every role collapsed into one agent (tracks and
    /// trivial work only; full milestones rejected).
    OneAgent,
    /// Two panes — orchestrator+reviewer share a supervisor pane,
    /// runner has its own. No ship-with-backlog path because there is
    /// no independent review channel.
    TwoAgent,
    /// Three panes — one pane per role; the canonical topology with
    /// independent review.
    ThreeAgent,
}

impl Topology {
    /// Stable kebab-case label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Topology::OneAgent => "one-agent",
            Topology::TwoAgent => "two-agent",
            Topology::ThreeAgent => "three-agent",
        }
    }

    /// Number of panes occupied by this topology.
    pub const fn pane_count(self) -> usize {
        match self {
            Topology::OneAgent => 1,
            Topology::TwoAgent => 2,
            Topology::ThreeAgent => 3,
        }
    }
}

impl Default for Topology {
    fn default() -> Self {
        Topology::ThreeAgent
    }
}

impl fmt::Display for Topology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Topology {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "one-agent" => Ok(Topology::OneAgent),
            "two-agent" => Ok(Topology::TwoAgent),
            "three-agent" => Ok(Topology::ThreeAgent),
            other => Err(format!("unknown topology {other:?}")),
        }
    }
}

/// Map a [`Topology`] to its pane-slot assignment.
///
/// Pure function — no I/O, no globals, fully testable in isolation.
/// Per spec:
/// - `ThreeAgent` -> `[[Orchestrator], [Runner], [Reviewer]]`
/// - `TwoAgent`   -> `[[Orchestrator, Reviewer], [Runner]]`
/// - `OneAgent`   -> `[[Orchestrator, Runner, Reviewer]]`
///
/// The order within each slot matches the declaration order in
/// [`Role::ALL`]; the order of slots is also declaration order. Both
/// are stable and asserted by tests.
pub fn role_pane_slots(topology: Topology) -> PaneSlots {
    match topology {
        Topology::ThreeAgent => vec![
            vec![Role::Orchestrator],
            vec![Role::Runner],
            vec![Role::Reviewer],
        ],
        Topology::TwoAgent => vec![
            vec![Role::Orchestrator, Role::Reviewer],
            vec![Role::Runner],
        ],
        Topology::OneAgent => vec![vec![Role::Orchestrator, Role::Runner, Role::Reviewer]],
    }
}

/// Returns the pane ordinal (index into [`role_pane_slots`]'s return)
/// that owns `role` under `topology`. Used to look up the matching
/// pane config when only the role is known.
pub fn pane_index_for(topology: Topology, role: Role) -> usize {
    role_pane_slots(topology)
        .into_iter()
        .position(|slot| slot.contains(&role))
        .unwrap_or_else(|| {
            // role_pane_slots always covers every role; this is a
            // debug-only guard so a future topology variant doesn't
            // silently drop a role.
            debug_assert!(false, "topology {topology} has no slot for role {role}");
            0
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_as_str_and_from_str_round_trip() {
        for r in Role::ALL {
            let s = r.as_str();
            let parsed: Role = s.parse().unwrap();
            assert_eq!(parsed, r);
            // Serde representation matches the as_str wire form.
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(json.trim_matches('"'), s);
        }
    }

    #[test]
    fn role_from_str_rejects_unknown() {
        let err = "coordinator".parse::<Role>().unwrap_err();
        assert!(err.contains("coordinator"), "{err}");
    }

    #[test]
    fn topology_three_agent_has_three_isolated_slots() {
        let slots = role_pane_slots(Topology::ThreeAgent);
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0], vec![Role::Orchestrator]);
        assert_eq!(slots[1], vec![Role::Runner]);
        assert_eq!(slots[2], vec![Role::Reviewer]);
    }

    #[test]
    fn topology_two_agent_combines_orchestrator_and_reviewer() {
        let slots = role_pane_slots(Topology::TwoAgent);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0], vec![Role::Orchestrator, Role::Reviewer]);
        assert_eq!(slots[1], vec![Role::Runner]);
    }

    #[test]
    fn topology_one_agent_collapses_every_role_into_one_slot() {
        let slots = role_pane_slots(Topology::OneAgent);
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0], vec![Role::Orchestrator, Role::Runner, Role::Reviewer]);
    }

    #[test]
    fn every_role_is_assigned_to_exactly_one_pane() {
        // No role may be dropped or duplicated across slots — this
        // guards a future topology variant from silently losing a
        // role or accidentally slotting it twice.
        for topology in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
            let slots = role_pane_slots(topology);
            let total: usize = slots.iter().map(|s| s.len()).sum();
            assert_eq!(
                total,
                Role::ALL.len(),
                "{topology} dropped or duplicated a role: {slots:?}"
            );
            let mut seen = Vec::new();
            for slot in &slots {
                for r in slot {
                    assert!(!seen.contains(r), "{topology} assigned {r} twice");
                    seen.push(*r);
                }
            }
            // Every declared role appears exactly once.
            for r in Role::ALL {
                assert!(seen.contains(&r), "{topology} missing role {r}");
            }
        }
    }

    #[test]
    fn topology_as_str_and_from_str_round_trip() {
        for t in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
            let parsed: Topology = t.as_str().parse().unwrap();
            assert_eq!(parsed, t);
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json.trim_matches('"'), t.as_str());
        }
    }

    #[test]
    fn topology_default_is_three_agent() {
        assert_eq!(Topology::default(), Topology::ThreeAgent);
    }

    #[test]
    fn topology_pane_count_matches_slot_count() {
        for t in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
            assert_eq!(role_pane_slots(t).len(), t.pane_count());
        }
    }

    #[test]
    fn pane_index_for_returns_orchestrator_index_in_three_agent() {
        assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Orchestrator), 0);
        assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Runner), 1);
        assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Reviewer), 2);
    }

    #[test]
    fn pane_index_for_returns_supervisor_slot_for_o_and_v_in_two_agent() {
        assert_eq!(pane_index_for(Topology::TwoAgent, Role::Orchestrator), 0);
        assert_eq!(pane_index_for(Topology::TwoAgent, Role::Reviewer), 0);
        assert_eq!(pane_index_for(Topology::TwoAgent, Role::Runner), 1);
    }

    #[test]
    fn pane_index_for_returns_zero_for_one_agent() {
        for r in Role::ALL {
            assert_eq!(pane_index_for(Topology::OneAgent, r), 0);
        }
    }
}
