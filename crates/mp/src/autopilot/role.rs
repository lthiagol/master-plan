//! M209 / AC-01, AC-02: the three-role autopilot model, the topology
//! pane-slot mapping, and the per-role config resolution chain.
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
//! - [`RoleConfigOverride`] — the per-source role config shape
//!   (model / harness / skill / extras).
//! - [`ResolvedRoleConfig`] — the typed output of resolution: every
//!   field mandatory, no `Option`s.
//! - [`resolve_role_config`] — pure merger implementing the
//!   three-tier priority chain (session override → config default →
//!   built-in). The single read path every consumer must use.
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

use std::collections::BTreeMap;
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

// ─── M209 / AC-02: per-role config resolution ────────────────────────

/// Per-source role config shape: model / harness / skill /
/// extras. Every field is optional because each of the three sources
/// (session.json override, mp config.json default, built-in) can
/// leave any combination unset. The resolver walks the chain and
/// produces a fully populated [`ResolvedRoleConfig`].
///
/// `model` / `harness` / `skill` are optional strings (an empty
/// string is treated as "not set" by the resolver so a config-clear
/// pass behaves the same as a never-set field). `extras` is a key→value
/// bag for harness-specific knobs; BTreeMap so the resolved shape is
/// deterministically ordered (testable + serde-friendly).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleConfigOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extras: BTreeMap<String, String>,
}

impl RoleConfigOverride {
    /// Construct the empty starting state — every field is `None`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when every field is unset (handy when the resolver needs
    /// to short-circuit through layers).
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.harness.is_none() && self.skill.is_none() && self.extras.is_empty()
    }

    /// True when `value` is `Some` AND non-empty. Empty strings are
    /// treated as "not set" so a config-clear pass behaves the same
    /// as a never-set field.
    fn as_set(value: &Option<String>) -> Option<&str> {
        value.as_deref().filter(|s| !s.is_empty())
    }
}

/// Empty placeholder used when the caller didn't supply a layer in
/// the [`resolve_role_config`] / [`resolve_role_config_with_provenance`]
/// path. Lets the merge logic treat "absent" as "no-op" without
/// having to handle `Option<&T>` per layer (the type would not
/// otherwise be uniform — see the array-of-references pattern in
/// `resolve_role_config`).
static EMPTY_ROLE_CONFIG_OVERRIDE: RoleConfigOverride = RoleConfigOverride {
    model: None,
    harness: None,
    skill: None,
    extras: BTreeMap::new(),
};

/// Output of [`resolve_role_config`]: every required field is
/// populated. `model` is `Option<String>` because today the built-in
/// default does not pin a model — the harness registry supplies it
/// at spawn time. `harness` and `skill` are mandatory because the
/// built-in default always populates them (so the resolver can never
/// return an empty string for either field; an empty override is
/// treated as "not set" and falls through to the next layer).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRoleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub harness: String,
    pub skill: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extras: BTreeMap<String, String>,
}

impl ResolvedRoleConfig {
    /// Borrow-style access for callers that already hold a model
    /// string (avoids one allocation on the hot path).
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn harness(&self) -> &str {
        &self.harness
    }

    pub fn skill(&self) -> &str {
        &self.skill
    }

    pub fn extras(&self) -> &BTreeMap<String, String> {
        &self.extras
    }
}

/// Pure merger: apply the three-tier priority chain
///
/// 1. `session_override` — `session.json.roles.<role>.*`
/// 2. `global_default`   — `mp config.json autopilot.roles.<role>.*`
/// 3. `builtin_default`  — hard-coded per-role fallback
///
/// Per field, the first source whose value is *set and non-empty*
/// wins. Empty strings are treated as "not set" so a session-driven
/// `mp autopilot config set … ""` clear behaves identically to never
/// setting the field. Per-key `extras` are merged left-to-right
/// (session wins over global wins over built-in) so a session can
/// override a single key without losing the rest of the bag.
pub fn resolve_role_config(
    session_override: Option<&RoleConfigOverride>,
    global_default: Option<&RoleConfigOverride>,
    builtin_default: &RoleConfigOverride,
) -> ResolvedRoleConfig {
    // Pick a scalar field (or `None` if no layer supplied it). `model`
    // is the only field where `None` is a valid resolved result; for
    // `harness` / `skill` the contract requires the built-in to populate
    // the field, so the helper exits the loop with the value.
    let pick_optional = |key: &str| -> Option<String> {
        for layer in [session_override, global_default, Some(builtin_default)] {
            let Some(ovr) = layer else { continue };
            let candidate = match key {
                "model" => RoleConfigOverride::as_set(&ovr.model),
                "harness" => RoleConfigOverride::as_set(&ovr.harness),
                "skill" => RoleConfigOverride::as_set(&ovr.skill),
                _ => None,
            };
            if let Some(v) = candidate {
                return Some(v.to_string());
            }
        }
        None
    };

    let pick_mandatory = |key: &str| -> String {
        pick_optional(key).unwrap_or_else(|| {
            // Resolver contract: the built-in MUST populate harness +
            // skill. If a caller passes a built-in with a missing
            // field, surface the bug loud and early rather than
            // producing a half-empty ResolvedRoleConfig.
            panic!(
                "resolve_role_config: built-in default for {key} is unset \
                 (the resolver's contract requires harness + skill to be \
                 populated on the built-in layer; fix the call site or \
                 extend the built-in table)"
            );
        })
    };

    let model = pick_optional("model");
    let harness = pick_mandatory("harness");
    let skill = pick_mandatory("skill");

    // extras: per-key merge in priority order. Session keys override
    // global keys override built-in keys. Drop empty values so a
    // session-cleared key falls through to the next layer (parity
    // with the scalar "empty string = unset" rule).
    let mut extras: BTreeMap<String, String> = BTreeMap::new();
    let layers: [&RoleConfigOverride; 3] = [
        builtin_default,
        global_default.unwrap_or(&EMPTY_ROLE_CONFIG_OVERRIDE),
        session_override.unwrap_or(&EMPTY_ROLE_CONFIG_OVERRIDE),
    ];
    // Iterate low→high priority (built-in -> global -> session), letting
    // later iterations overwrite earlier ones so the higher-priority
    // layer wins on conflict.
    for layer in layers.iter() {
        for (k, v) in &layer.extras {
            if !v.is_empty() {
                extras.insert(k.clone(), v.clone());
            }
        }
    }

    ResolvedRoleConfig {
        model,
        harness,
        skill,
        extras,
    }
}

/// Built-in role config defaults — the floor of the resolution
/// chain. Per spec: orchestrator → `mp-coordinator`,
/// runner/reviewer → `mp-runner`. Harness defaults to `opencode`
/// (matches M149's `resolve_harness_kind` fallback); model is left
/// empty because the harness registry supplies it at spawn time.
pub fn builtin_role_default(role: Role) -> RoleConfigOverride {
    let mut extras = BTreeMap::new();
    // Per-role fallback extras — kept tiny so the resolver's
    // contract (every role has model/harness/skill) is satisfied.
    match role {
        Role::Orchestrator => {
            extras.insert("cycle_budget".to_string(), "4".to_string());
        }
        Role::Runner => {
            extras.insert("cycle_budget".to_string(), "4".to_string());
        }
        Role::Reviewer => {
            extras.insert("cycle_budget".to_string(), "4".to_string());
        }
    }
    RoleConfigOverride {
        model: None,
        harness: Some("opencode".to_string()),
        skill: Some(match role {
            Role::Orchestrator => "mp-coordinator".to_string(),
            Role::Runner => "mp-runner".to_string(),
            Role::Reviewer => "mp-runner".to_string(),
        }),
        extras,
    }
}

/// Convenience: resolve `role` against all three tiers (built-in is
/// filled automatically via [`builtin_role_default`]). This is the
/// single read path every consumer must use — direct layering in
/// callers is forbidden by the spec ("the resolution function is the
/// only path other code uses to read role config").
pub fn resolve_role_config_full(
    role: Role,
    session_override: Option<&RoleConfigOverride>,
    global_default: Option<&RoleConfigOverride>,
) -> ResolvedRoleConfig {
    let builtin = builtin_role_default(role);
    resolve_role_config(session_override, global_default, &builtin)
}

/// Diagnostic surface: report which source provided which field.
/// Used by `mp autopilot config show` and by S5's legacy-fallback
/// path so a conflict surfaces a typed diagnostic instead of
/// silently dropping one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleConfigSource {
    Session,
    Global,
    Builtin,
}

impl RoleConfigSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoleConfigSource::Session => "session",
            RoleConfigSource::Global => "global",
            RoleConfigSource::Builtin => "builtin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRoleConfigWithProvenance {
    pub resolved: ResolvedRoleConfig,
    pub model_source: Option<RoleConfigSource>,
    pub harness_source: Option<RoleConfigSource>,
    pub skill_source: Option<RoleConfigSource>,
}

/// Like [`resolve_role_config_full`] but reports, per scalar field,
/// which tier provided the value. `extras` provenance is a
/// per-key map (Key → RoleConfigSource) for diagnostics.
pub fn resolve_role_config_with_provenance(
    role: Role,
    session_override: Option<&RoleConfigOverride>,
    global_default: Option<&RoleConfigOverride>,
) -> ResolvedRoleConfigWithProvenance {
    let builtin = builtin_role_default(role);

    let pick_with_source = |key: &str| -> (Option<String>, Option<RoleConfigSource>) {
        for (layer, src) in [
            (session_override, Some(RoleConfigSource::Session)),
            (global_default, Some(RoleConfigSource::Global)),
            (Some(&builtin), Some(RoleConfigSource::Builtin)),
        ] {
            let Some(ovr) = layer else { continue };
            let candidate = match key {
                "model" => RoleConfigOverride::as_set(&ovr.model),
                "harness" => RoleConfigOverride::as_set(&ovr.harness),
                "skill" => RoleConfigOverride::as_set(&ovr.skill),
                _ => None,
            };
            if let Some(v) = candidate {
                return (Some(v.to_string()), src);
            }
        }
        (None, None)
    };

    let pick_mandatory_with_source = |key: &str| -> (String, Option<RoleConfigSource>) {
        let (v, src) = pick_with_source(key);
        (
            v.unwrap_or_else(|| {
                panic!(
                    "resolve_role_config_with_provenance: built-in \
                     default for {key} is unset for role {role}"
                )
            }),
            src,
        )
    };

    let (model, model_source) = pick_with_source("model");
    let (harness, harness_source) = pick_mandatory_with_source("harness");
    let (skill, skill_source) = pick_mandatory_with_source("skill");

    let mut extras = BTreeMap::new();
    let layers: [&RoleConfigOverride; 3] = [
        &builtin,
        global_default.unwrap_or(&EMPTY_ROLE_CONFIG_OVERRIDE),
        session_override.unwrap_or(&EMPTY_ROLE_CONFIG_OVERRIDE),
    ];
    // Iterate low→high priority (built-in -> global -> session), letting
    // later iterations overwrite earlier ones so the higher-priority
    // layer wins on conflict.
    for layer in layers.iter() {
        for (k, v) in &layer.extras {
            if !v.is_empty() {
                extras.insert(k.clone(), v.clone());
            }
        }
    }
    let resolved = ResolvedRoleConfig {
        model,
        harness,
        skill,
        extras,
    };
    ResolvedRoleConfigWithProvenance {
        resolved,
        model_source,
        harness_source,
        skill_source,
    }
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

    // ─── AC-02: resolution chain ──────────────────────────────────

    fn ovr(model: &str, harness: &str, skill: &str) -> RoleConfigOverride {
        RoleConfigOverride {
            model: Some(model.into()),
            harness: Some(harness.into()),
            skill: Some(skill.into()),
            extras: BTreeMap::new(),
        }
    }

    fn ovr_partial(
        model: Option<&str>,
        harness: Option<&str>,
        skill: Option<&str>,
    ) -> RoleConfigOverride {
        RoleConfigOverride {
            model: model.map(str::to_string),
            harness: harness.map(str::to_string),
            skill: skill.map(str::to_string),
            extras: BTreeMap::new(),
        }
    }

    #[test]
    fn builtin_default_provides_harness_and_skill_for_every_role() {
        // Resolver contract: every role's built-in must populate
        // harness and skill (model may be None — the harness
        // registry fills it at spawn time).
        for role in Role::ALL {
            let b = builtin_role_default(role);
            assert!(b.harness.is_some(), "role {role} has no built-in harness");
            assert!(b.skill.is_some(), "role {role} has no built-in skill");
        }
    }

    #[test]
    fn builtin_skill_mapping_matches_documented_role_names() {
        // Orchestrator -> mp-coordinator; Runner / Reviewer -> mp-runner.
        // Pin so a rename in one place breaks the test rather than
        // silently shipping a misaligned default.
        assert_eq!(builtin_role_default(Role::Orchestrator).skill.as_deref(), Some("mp-coordinator"));
        assert_eq!(builtin_role_default(Role::Runner).skill.as_deref(), Some("mp-runner"));
        assert_eq!(builtin_role_default(Role::Reviewer).skill.as_deref(), Some("mp-runner"));
    }

    #[test]
    fn resolve_with_no_overrides_yields_builtins() {
        let resolved = resolve_role_config(None, None, &builtin_role_default(Role::Orchestrator));
        assert_eq!(resolved.skill, "mp-coordinator");
        assert_eq!(resolved.harness, "opencode");
        // Model has no built-in today (the harness registry fills it
        // at spawn time) — resolver returns None cleanly.
        assert_eq!(resolved.model, None);
    }

    #[test]
    fn resolve_session_override_beats_global_default() {
        // Per spec: session.json override wins.
        let global = ovr("anthropic/claude-sonnet-4-5", "opencode", "mp-coordinator");
        let session = ovr("anthropic/claude-opus-4-1", "pi", "mp-coordinator");
        let resolved = resolve_role_config_full(
            Role::Orchestrator,
            Some(&session),
            Some(&global),
        );
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.harness, "pi");
    }

    #[test]
    fn resolve_global_default_beats_builtin() {
        // When session override is absent, the mp config.json
        // default fills in.
        let global = ovr("anthropic/claude-opus-4-1", "pi", "mp-coordinator");
        let resolved = resolve_role_config_full(Role::Orchestrator, None, Some(&global));
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.harness, "pi");
        // Skill still comes from the built-in (no global override).
        assert_eq!(resolved.skill, "mp-coordinator");
    }

    #[test]
    fn empty_string_override_falls_through_to_next_layer() {
        // Spec: "Empty string overrides are treated as 'not set'".
        // A session-driven `config set … ""` must behave like a
        // never-set field.
        let session = ovr_partial(Some(""), Some(""), Some(""));
        let global = ovr("anthropic/claude-opus-4-1", "opencode", "mp-coordinator");
        let resolved = resolve_role_config_full(
            Role::Orchestrator,
            Some(&session),
            Some(&global),
        );
        // Every field was an empty string on session — fall through
        // to the global layer.
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.harness, "opencode");
        assert_eq!(resolved.skill, "mp-coordinator");
    }

    #[test]
    fn partial_session_override_merges_per_field() {
        // Session sets only `harness`; model and skill fall through.
        let session = ovr_partial(None, Some("cursor"), None);
        let global = ovr("anthropic/claude-opus-4-1", "opencode", "mp-runner");
        let resolved = resolve_role_config_full(
            Role::Runner,
            Some(&session),
            Some(&global),
        );
        assert_eq!(resolved.harness, "cursor");
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.skill, "mp-runner");
    }

    #[test]
    fn extras_merge_per_key_with_session_winning() {
        // Per-key merge: built-in -> global -> session. Session's
        // `cap` overrides global's `cap`; global's `flag` survives
        // because session didn't set it.
        let mut builtin = builtin_role_default(Role::Runner);
        builtin.extras.insert("cap".to_string(), "builtin-cap".to_string());
        builtin.extras.insert("flag".to_string(), "builtin-flag".to_string());

        let mut global = RoleConfigOverride::empty();
        global.extras.insert("cap".to_string(), "global-cap".to_string());
        global.extras.insert("flag".to_string(), "global-flag".to_string());

        let mut session = RoleConfigOverride::empty();
        session.extras.insert("cap".to_string(), "session-cap".to_string());

        let resolved = resolve_role_config(Some(&session), Some(&global), &builtin);
        assert_eq!(resolved.extras.get("cap").map(String::as_str), Some("session-cap"));
        assert_eq!(
            resolved.extras.get("flag").map(String::as_str),
            Some("global-flag"),
            "session didn't set flag, so global's value must survive"
        );
    }

    #[test]
    fn extras_empty_value_falls_through_to_next_layer() {
        // Empty-string extras parity with the scalar fields: a
        // session-driven empty value clears the key, falling back
        // to the next layer.
        let builtin = {
            let mut b = builtin_role_default(Role::Runner);
            b.extras.insert("flag".to_string(), "builtin-flag".to_string());
            b
        };
        let global = RoleConfigOverride::empty();
        let mut session = RoleConfigOverride::empty();
        session.extras.insert("flag".to_string(), String::new());

        let resolved = resolve_role_config(Some(&session), Some(&global), &builtin);
        assert_eq!(
            resolved.extras.get("flag").map(String::as_str),
            Some("builtin-flag"),
            "empty session value must fall through to the built-in"
        );
    }

    #[test]
    fn resolve_with_provenance_reports_source_per_field() {
        // Session sets model; global sets harness; built-in fills skill.
        let session = ovr_partial(Some("anthropic/claude-opus-4-1"), None, None);
        let global = ovr_partial(None, Some("pi"), None);
        let out = resolve_role_config_with_provenance(
            Role::Orchestrator,
            Some(&session),
            Some(&global),
        );
        assert_eq!(out.model_source, Some(RoleConfigSource::Session));
        assert_eq!(out.harness_source, Some(RoleConfigSource::Global));
        assert_eq!(out.skill_source, Some(RoleConfigSource::Builtin));
        assert_eq!(out.resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(out.resolved.harness, "pi");
        assert_eq!(out.resolved.skill, "mp-coordinator");
    }

    #[test]
    fn role_config_override_is_empty_when_unset() {
        let o = RoleConfigOverride::empty();
        assert!(o.is_empty());
        let mut o = RoleConfigOverride::empty();
        o.model = Some("x".to_string());
        assert!(!o.is_empty());
    }

    #[test]
    fn resolve_role_config_full_is_canonical_for_three_callers() {
        // All three roles resolve cleanly through the convenience
        // helper without any override — pins the public surface
        // for S5 + raul Settings lane consumers.
        for role in Role::ALL {
            let resolved = resolve_role_config_full(role, None, None);
            // model may be None (built-in doesn't define one — the
            // harness registry fills it at spawn time). Harness and
            // skill are mandatory and come from the built-in.
            assert!(resolved.model().is_none(), "{role} model should be None");
            assert!(!resolved.harness.is_empty(), "{role} harness has built-in");
            assert!(!resolved.skill.is_empty(), "{role} skill has built-in");
        }
    }
}
