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

// ─── M209 / AC-04: topology-mode tightening ─────────────────────────

/// Topology-mode decision policy. The decision matrix (C3 in the
/// spec) cannot offer the same paths in a 1-pane topology as it does
/// in 3-pane: a single pane means there is no independent reviewer
/// channel, so the agent cannot ship a milestone with backlog — only
/// trivial work / tracks is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopologyMode {
    /// All decision-matrix paths apply. Default for 3-pane.
    FullMatrix,
    /// Orchestrator+Reviewer share a supervisor pane, so there is no
    /// independent reviewer channel. The matrix forbids
    /// ship-with-backlog; only `cycle-next` or `escalate` is allowed.
    /// Default for 2-pane.
    NoShipWithBacklog,
    /// Every role collapsed into one pane. Only tracks / trivial
    /// work are supported; full milestones are rejected by preflight
    /// unless an explicit recorded review-bypass policy exists.
    /// Default for 1-pane.
    SingleAgentTrackOnly,
}

impl TopologyMode {
    /// Wire-string form (used by diagnostic surfaces and the future
    /// `mp autopilot config schema`).
    pub const fn as_str(self) -> &'static str {
        match self {
            TopologyMode::FullMatrix => "full_matrix",
            TopologyMode::NoShipWithBacklog => "no_ship_with_backlog",
            TopologyMode::SingleAgentTrackOnly => "single_agent_track_only",
        }
    }
}

impl fmt::Display for TopologyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-topology decision policy: the mode plus the cycle budget
/// (the maximum number of cycles a milestone may run through before
/// the driver escalates). Tightened budgets in smaller topologies
/// make the role collapse's quality trade-off explicit.
///
/// `three_agent -> FullMatrix / 4 cycles`
/// `two_agent   -> NoShipWithBacklog / 3 cycles`
/// `one_agent   -> SingleAgentTrackOnly / 2 cycles`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TopologyPolicy {
    pub mode: TopologyMode,
    pub cycle_budget: u32,
}

impl TopologyPolicy {
    /// Convenience: returns true when this policy allows a milestone
    /// to ship with backlog still open.
    pub fn allows_ship_with_backlog(&self) -> bool {
        self.mode == TopologyMode::FullMatrix
    }

    /// Convenience: returns true when this policy allows an
    /// independent external review pass.
    pub fn allows_external_review(&self) -> bool {
        // 2-pane combines orchestrator+reviewer on one pane — the
        // "review" surface still exists but is not independent of
        // the orchestrator. 1-pane collapses every role.
        matches!(self.mode, TopologyMode::FullMatrix)
    }
}

/// Tighten the decision matrix based on the topology. Pure mapping
/// per AC-04.
pub fn topology_policy(topology: Topology) -> TopologyPolicy {
    match topology {
        Topology::ThreeAgent => TopologyPolicy {
            mode: TopologyMode::FullMatrix,
            cycle_budget: 4,
        },
        Topology::TwoAgent => TopologyPolicy {
            mode: TopologyMode::NoShipWithBacklog,
            cycle_budget: 3,
        },
        Topology::OneAgent => TopologyPolicy {
            mode: TopologyMode::SingleAgentTrackOnly,
            cycle_budget: 2,
        },
    }
}

/// Preflight error returned when a topology / milestone-kind
/// combination is not allowed. The gate's job is to surface the
/// rejection loudly *before* the runner starts, with the policy
/// attached so the operator can adapt (switch topology, change
/// milestone kind, or record a bypass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyPreflightError {
    /// 1-pane topology was selected for a full milestone; the
    /// caller must either switch topology or record a review-bypass
    /// policy.
    FullMilestoneRequiresReviewer { policy: TopologyPolicy },
    /// 2-pane topology was asked for a ship-with-backlog path; the
    /// resolver must pick `cycle-next` or `escalate` instead.
    /// (Reserved — the matrix C-layer surfaces this; the preflight
    /// is a structural check, not a path decision. Surfaced here so
    /// a future test can pin the matrix-side wiring.)
    ShipWithBacklogDisabled { policy: TopologyPolicy },
}

impl fmt::Display for TopologyPreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopologyPreflightError::FullMilestoneRequiresReviewer { policy } => write!(
                f,
                "full milestones require a topology with an independent reviewer (got {} topology, cycle budget={}); switch topology or record a review-bypass policy",
                policy.mode, policy.cycle_budget
            ),
            TopologyPreflightError::ShipWithBacklogDisabled { policy } => write!(
                f,
                "ship-with-backlog path is disabled under {} topology (cycle budget={}); pick cycle-next or escalate instead",
                policy.mode, policy.cycle_budget
            ),
        }
    }
}

impl std::error::Error for TopologyPreflightError {}

/// Outcome class the preflight gate inspects. Tracks and full
/// milestones differ: tracks are trivial and allowed in 1-pane; full
/// milestones are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MilestoneKind {
    /// Full milestone — the standard reviewer / orchestrator gated
    /// flow applies.
    Full,
    /// Track / trivial work — the runner executes inline; reviewer
    /// is not invoked.
    Track,
}

/// Recorded / unrecorded review-bypass override. The preflight gate
/// looks for an on-disk record (a future milestone will own
/// `AutopilotConfig::review_bypass`); for M209 we model the gate's
/// input as a structured value so callers can plumb the disk check
/// later without changing the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewBypassPolicy {
    /// No bypass requested. Default.
    #[default]
    None,
    /// Bypass requested via an unrecorded mechanism (CLI flag,
    /// runtime override). Not honored for 1-pane full milestones.
    Unrecorded,
    /// Bypass recorded on disk in the project's autopilot config.
    /// Honored for 1-pane full milestones.
    Recorded,
}

impl ReviewBypassPolicy {
    /// True when the bypass is on disk. The preflight gate accepts
    /// only recorded bypasses for the 1-pane full-milestone case.
    pub fn is_recorded(&self) -> bool {
        matches!(self, ReviewBypassPolicy::Recorded)
    }

    /// True when any bypass is set (recorded or unrecorded).
    pub fn is_set(&self) -> bool {
        !matches!(self, ReviewBypassPolicy::None)
    }
}

/// Preflight gate: validate a topology decision against the
/// would-be milestone kind and an explicit review-bypass override.
///
/// Per spec: "Starting a full milestone in 1-pane is rejected unless
/// an explicit recorded review-bypass policy exists." The 2-pane
/// ship-with-backlog path is a matrix-side decision (resolved
/// downstream); the structural preflight only enforces the
/// 1-pane/full-milestone rule.
pub fn topology_preflight(
    topology: Topology,
    kind: MilestoneKind,
    review_bypass: ReviewBypassPolicy,
) -> Result<TopologyPolicy, TopologyPreflightError> {
    let policy = topology_policy(topology);
    match (topology, kind) {
        (Topology::OneAgent, MilestoneKind::Full) => {
            // A1's carve-out: tracks are the 1-pane use case; full
            // milestones need an independent reviewer. A bypass is
            // honored only when recorded on disk — an unrecorded
            // override is a no-op here so a stray CLI flag cannot
            // quietly bypass review.
            if review_bypass.is_recorded() {
                Ok(policy)
            } else {
                Err(TopologyPreflightError::FullMilestoneRequiresReviewer { policy })
            }
        }
        // Tracks are accepted under every topology.
        (_, MilestoneKind::Track) => Ok(policy),
        // 2-pane + full + non-1-pane — accept the policy; the
        // ship-with-backlog restriction is matrix-side.
        _ => Ok(policy),
    }
}

/// Convenience: tighten the decision matrix and run the preflight
/// gate in one call. The two are split because future tests / agents
/// want to inspect the policy independently of the gate.
pub fn tighten(
    topology: Topology,
    kind: MilestoneKind,
    review_bypass: ReviewBypassPolicy,
) -> Result<TopologyPolicy, TopologyPreflightError> {
    topology_preflight(topology, kind, review_bypass)
}

// ─── M209 / AC-05: legacy role fallback ────────────────────────────

/// Typed diagnostic raised by [`resolve_with_legacy_fallback`].
/// The function refuses to silently invent a value when the
/// autopilot override is absent and the legacy role does not have a
/// compatible analog (the reviewer case) or when the two layers
/// disagree on a field both supply (a conflict that the operator
/// must resolve deliberately).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleResolutionError {
    /// `Role::Reviewer` has no legacy analog — `mp watch` predates
    /// the orchestrator/runner/reviewer split (it had
    /// `agent.coordinator` + `agent.runner`). The reviewer must be
    /// configured through the new `autopilot.roles.reviewer.*`
    /// surface; surfacing this as a typed error prevents a silent
    /// "fell through to empty" outcome.
    UnsupportedReviewerFallback,
    /// Autopilot override and legacy role disagree on `harness`.
    /// Both layers are populated, and the function refuses to
    /// silently pick one. The error carries both values so the
    /// operator can decide which source was the typo.
    ConflictingHarnessFallback {
        role: Role,
        autopilot: String,
        legacy: String,
    },
    /// Autopilot override and legacy role disagree on `model`. Same
    /// shape as the harness conflict.
    ConflictingModelFallback {
        role: Role,
        autopilot: String,
        legacy: String,
    },
}

impl fmt::Display for RoleResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoleResolutionError::UnsupportedReviewerFallback => write!(
                f,
                "no legacy fallback for autopilot role 'reviewer': the reviewer role \
                 must be configured via `mp autopilot config set \
                 autopilot.roles.reviewer.<field> <value>` (the legacy \
                 `agent.coordinator` is mapped to orchestrator, not reviewer)"
            ),
            RoleResolutionError::ConflictingHarnessFallback {
                role,
                autopilot,
                legacy,
            } => write!(
                f,
                "conflicting harness for autopilot role '{role}': \
                 autopilot override says {autopilot:?}, legacy {legacy_role} says {legacy:?}; \
                 pick one (clear the other to remove the conflict)",
                legacy_role = legacy_role_label(*role),
            ),
            RoleResolutionError::ConflictingModelFallback {
                role,
                autopilot,
                legacy,
            } => write!(
                f,
                "conflicting model for autopilot role '{role}': \
                 autopilot override says {autopilot:?}, legacy {legacy_role} says {legacy:?}; \
                 pick one (clear the other to remove the conflict)",
                legacy_role = legacy_role_label(*role),
            ),
        }
    }
}

impl std::error::Error for RoleResolutionError {}

/// Map an autopilot `Role` to the legacy `agent.<role>` label.
/// - Orchestrator -> "agent.coordinator" (the legacy role whose
///   responsibilities split into orchestrator + reviewer).
/// - Runner -> "agent.runner".
/// - Reviewer -> no analog.
fn legacy_role_label(role: Role) -> &'static str {
    match role {
        Role::Orchestrator => "agent.coordinator",
        Role::Runner => "agent.runner",
        Role::Reviewer => "<no legacy analog>",
    }
}

/// Pure legacy-to-autopilot role resolution.
///
/// Inputs:
/// - `role` — the autopilot role to resolve for.
/// - `autopilot_override` — `autopilot.roles.<role>.*` from mp config.json.
/// - `legacy_runner` — `agent.runner` (`crate::config::RoleConfig`).
/// - `legacy_coordinator` — `agent.coordinator`.
/// - `harness_default` — fallback when no layer supplies a harness
///   (e.g. the harness registry's opencode default).
///
/// Priority chain (per spec):
/// 1. Explicit autopilot override wins.
/// 2. Compatible legacy role fallback (Runner -> agent.runner;
///    Orchestrator -> agent.coordinator; Reviewer -> error).
/// 3. Harness default.
/// 4. Built-in default (filled by `resolve_role_config`).
///
/// Conflicts (autopilot set AND legacy set to a DIFFERENT value)
/// surface as typed errors. Same value (both "opencode") is fine —
/// the legacy just confirms the autopilot choice.
pub fn resolve_with_legacy_fallback(
    role: Role,
    autopilot_override: Option<&RoleConfigOverride>,
    legacy_runner: Option<&crate::config::RoleConfig>,
    legacy_coordinator: Option<&crate::config::RoleConfig>,
) -> Result<ResolvedRoleConfig, RoleResolutionError> {
    // Step 1: harvest the legacy fallback. Per role, only one
    // legacy section is compatible.
    let legacy_compat = match role {
        Role::Runner => legacy_runner,
        Role::Orchestrator => legacy_coordinator,
        Role::Reviewer => {
            // No legacy analog. Either an autopilot override
            // exists (and we proceed) or we surface the typed
            // error so the operator knows reviewer must be set
            // through the new surface.
            if autopilot_override.is_none() || autopilot_override.map_or(true, |o| o.is_empty()) {
                return Err(RoleResolutionError::UnsupportedReviewerFallback);
            }
            None
        }
    };

    // Step 2: detect conflicts on the overlapping fields
    // (harness, model). Equal values are fine — they're just a
    // confirmation, not a conflict.
    if let (Some(ovr), Some(legacy)) = (autopilot_override, legacy_compat) {
        if let (Some(h), Some(lh)) = (
            ovr.harness.as_deref().filter(|s| !s.is_empty()),
            legacy.harness.as_deref().filter(|s| !s.is_empty()),
        ) {
            if h != lh {
                return Err(RoleResolutionError::ConflictingHarnessFallback {
                    role,
                    autopilot: h.to_string(),
                    legacy: lh.to_string(),
                });
            }
        }
        if let (Some(m), Some(lm)) = (
            ovr.model.as_deref().filter(|s| !s.is_empty()),
            legacy.model.as_deref().filter(|s| !s.is_empty()),
        ) {
            if m != lm {
                return Err(RoleResolutionError::ConflictingModelFallback {
                    role,
                    autopilot: m.to_string(),
                    legacy: lm.to_string(),
                });
            }
        }
    }

    // Step 3: build the merged override. Start with the legacy
    // shape (so the legacy's defaults are honored) and overlay the
    // autopilot override on top.
    let mut merged: RoleConfigOverride = match legacy_compat {
        Some(legacy) => RoleConfigOverride {
            model: clone_non_empty(&legacy.model),
            harness: clone_non_empty(&legacy.harness),
            skill: None,
            extras: {
                let mut ex = BTreeMap::new();
                if let Some(cmd) = &legacy.command {
                    if !cmd.is_empty() {
                        ex.insert(
                            "legacy.command".to_string(),
                            serde_json::to_string(cmd).unwrap_or_default(),
                        );
                    }
                }
                if let Some(tl) = &legacy.thinking_level {
                    if !tl.is_empty() {
                        ex.insert("legacy.thinking_level".to_string(), tl.clone());
                    }
                }
                ex
            },
        },
        None => RoleConfigOverride::empty(),
    };

    // Overlay autopilot override on top — any non-empty field
    // wins. The conflict check above guarantees matching fields
    // either agree or only one layer set them.
    if let Some(ovr) = autopilot_override {
        if let Some(h) = ovr.harness.as_deref().filter(|s| !s.is_empty()) {
            merged.harness = Some(h.to_string());
        }
        if let Some(m) = ovr.model.as_deref().filter(|s| !s.is_empty()) {
            merged.model = Some(m.to_string());
        }
        if let Some(s) = ovr.skill.as_deref().filter(|s| !s.is_empty()) {
            merged.skill = Some(s.to_string());
        }
        for (k, v) in &ovr.extras {
            if !v.is_empty() {
                merged.extras.insert(k.clone(), v.clone());
            }
        }
    }

    // Step 4: resolve through the canonical chain. The autopilot
    // override is the merged value; the global default stays as
    // None (this function only operates on per-role layers).
    let builtin = builtin_role_default(role);
    Ok(resolve_role_config(Some(&merged), None, &builtin))
}

fn clone_non_empty(opt: &Option<String>) -> Option<String> {
    opt.as_ref().filter(|s| !s.is_empty()).cloned()
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

    // ─── AC-04: topology tightening ─────────────────────────────────

    #[test]
    fn topology_policy_three_agent_is_full_matrix_with_four_cycle_budget() {
        let p = topology_policy(Topology::ThreeAgent);
        assert_eq!(p.mode, TopologyMode::FullMatrix);
        assert_eq!(p.cycle_budget, 4);
        assert!(p.allows_ship_with_backlog());
        assert!(p.allows_external_review());
    }

    #[test]
    fn topology_policy_two_agent_is_no_ship_with_backlog_with_three_cycle_budget() {
        let p = topology_policy(Topology::TwoAgent);
        assert_eq!(p.mode, TopologyMode::NoShipWithBacklog);
        assert_eq!(p.cycle_budget, 3);
        assert!(!p.allows_ship_with_backlog());
        assert!(
            !p.allows_external_review(),
            "two_agent's reviewer is co-located with orchestrator"
        );
    }

    #[test]
    fn topology_policy_one_agent_is_single_agent_track_only_with_two_cycle_budget() {
        let p = topology_policy(Topology::OneAgent);
        assert_eq!(p.mode, TopologyMode::SingleAgentTrackOnly);
        assert_eq!(p.cycle_budget, 2);
        assert!(!p.allows_ship_with_backlog());
        assert!(!p.allows_external_review());
    }

    #[test]
    fn preflight_accepts_full_milestone_in_three_agent_without_bypass() {
        let policy = topology_preflight(Topology::ThreeAgent, MilestoneKind::Full, ReviewBypassPolicy::None).unwrap();
        assert_eq!(policy.mode, TopologyMode::FullMatrix);
    }

    #[test]
    fn preflight_accepts_full_milestone_in_two_agent_without_bypass() {
        // 2-pane + full milestone is allowed (the matrix handles the
        // ship-with-backlog restriction downstream).
        let policy = topology_preflight(Topology::TwoAgent, MilestoneKind::Full, ReviewBypassPolicy::None).unwrap();
        assert_eq!(policy.mode, TopologyMode::NoShipWithBacklog);
    }

    #[test]
    fn preflight_rejects_full_milestone_in_one_agent_without_recorded_bypass() {
        // The spec's headline rule.
        let err =
            topology_preflight(Topology::OneAgent, MilestoneKind::Full, ReviewBypassPolicy::None)
                .unwrap_err();
        match err {
            TopologyPreflightError::FullMilestoneRequiresReviewer { policy } => {
                assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
                assert_eq!(policy.cycle_budget, 2);
            }
            other => panic!("expected FullMilestoneRequiresReviewer, got {other:?}"),
        }
    }

    #[test]
    fn preflight_rejects_full_milestone_in_one_agent_with_unrecorded_bypass() {
        // A CLI flag / runtime override is not enough — the bypass
        // must be on disk to qualify.
        let err = topology_preflight(
            Topology::OneAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::Unrecorded,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TopologyPreflightError::FullMilestoneRequiresReviewer { .. }
        ));
    }

    #[test]
    fn preflight_accepts_full_milestone_in_one_agent_with_recorded_bypass() {
        // Recorded bypass is the one explicit override the gate
        // honors for the 1-pane/full-milestone case.
        let policy = topology_preflight(
            Topology::OneAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::Recorded,
        )
        .unwrap();
        assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
    }

    #[test]
    fn preflight_accepts_track_under_any_topology() {
        // Tracks are the carve-out's whole point — they must be
        // accepted under every topology.
        for topology in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
            let policy =
                topology_preflight(topology, MilestoneKind::Track, ReviewBypassPolicy::None)
                    .unwrap();
            assert_eq!(policy.mode, topology_policy(topology).mode);
        }
    }

    #[test]
    fn review_bypass_default_is_unset() {
        // Pin the default so a future change to `ReviewBypassPolicy`
        // doesn't silently start honoring bypasses.
        let b = ReviewBypassPolicy::default();
        assert!(!b.is_set());
        assert!(!b.is_recorded());
    }

    #[test]
    fn review_bypass_recorded_and_unrecorded_are_distinguished() {
        assert!(ReviewBypassPolicy::Recorded.is_recorded());
        assert!(!ReviewBypassPolicy::Unrecorded.is_recorded());
        assert!(ReviewBypassPolicy::Unrecorded.is_set());
        assert!(ReviewBypassPolicy::Recorded.is_set());
    }

    #[test]
    fn topology_mode_wire_strings() {
        // The as_str strings feed into `mp autopilot config schema`
        // (future) and into diagnostic JSON. Pin so a rename is a
        // deliberate test change.
        assert_eq!(TopologyMode::FullMatrix.as_str(), "full_matrix");
        assert_eq!(
            TopologyMode::NoShipWithBacklog.as_str(),
            "no_ship_with_backlog"
        );
        assert_eq!(
            TopologyMode::SingleAgentTrackOnly.as_str(),
            "single_agent_track_only"
        );
    }

    #[test]
    fn topology_preflight_error_displays_actionable_message() {
        let err =
            topology_preflight(Topology::OneAgent, MilestoneKind::Full, ReviewBypassPolicy::None)
                .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("record a review-bypass"),
            "display must hint at the bypass: {msg}"
        );
        assert!(
            msg.contains("single_agent_track_only") || msg.contains("SingleAgentTrackOnly"),
            "display must name the topology mode for diagnosis: {msg}"
        );
    }

    // ─── AC-05: legacy fallback ───────────────────────────────────

    use crate::config::RoleConfig as LegacyRoleConfig;

    fn legacy(harness: &str, model: &str) -> LegacyRoleConfig {
        LegacyRoleConfig {
            harness: Some(harness.to_string()),
            command: None,
            model: Some(model.to_string()),
            thinking_level: None,
        }
    }

    #[test]
    fn legacy_fallback_runner_picks_up_agent_runner() {
        let legacy_runner = legacy("opencode", "anthropic/claude-opus-4-1");
        let resolved = resolve_with_legacy_fallback(
            Role::Runner,
            None,
            Some(&legacy_runner),
            None,
        )
        .expect("runner + legacy fallback should succeed");
        assert_eq!(resolved.harness, "opencode");
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.skill, "mp-runner");
    }

    #[test]
    fn legacy_fallback_orchestrator_picks_up_agent_coordinator() {
        let legacy_coord = legacy("pi", "anthropic/claude-opus-4-1");
        let resolved = resolve_with_legacy_fallback(
            Role::Orchestrator,
            None,
            None,
            Some(&legacy_coord),
        )
        .expect("orchestrator + legacy fallback should succeed");
        assert_eq!(resolved.harness, "pi");
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        assert_eq!(resolved.skill, "mp-coordinator");
    }

    #[test]
    fn legacy_fallback_reviewer_without_override_is_typed_error() {
        let result = resolve_with_legacy_fallback(Role::Reviewer, None, None, None);
        assert!(matches!(
            result,
            Err(RoleResolutionError::UnsupportedReviewerFallback)
        ));
    }

    #[test]
    fn legacy_fallback_reviewer_with_override_succeeds() {
        let mut ovr = RoleConfigOverride::empty();
        ovr.harness = Some("opencode".to_string());
        ovr.skill = Some("mp-runner".to_string());
        ovr.model = Some("anthropic/claude-opus-4-1".to_string());
        let resolved = resolve_with_legacy_fallback(Role::Reviewer, Some(&ovr), None, None)
            .expect("reviewer + autopilot override should succeed without legacy");
        assert_eq!(resolved.harness, "opencode");
    }

    #[test]
    fn legacy_fallback_explicit_override_wins_when_legacy_field_absent() {
        // Autopilot override sets harness + model; legacy is absent
        // entirely. The override wins on every field.
        let mut ovr = RoleConfigOverride::empty();
        ovr.harness = Some("pi".to_string());
        ovr.model = Some("anthropic/claude-opus-4-1".to_string());
        let resolved = resolve_with_legacy_fallback(
            Role::Runner,
            Some(&ovr),
            None,
            None,
        )
        .expect("override without legacy fallback should succeed");
        assert_eq!(resolved.harness, "pi");
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"));
        // skill comes from the built-in (neither layer set it).
        assert_eq!(resolved.skill, "mp-runner");
    }

    #[test]
    fn legacy_fallback_matching_values_do_not_conflict() {
        // Both layers say "opencode" for harness — that's a
        // confirmation, not a conflict.
        let legacy_runner = legacy("opencode", "anthropic/claude-opus-4-1");
        let mut ovr = RoleConfigOverride::empty();
        ovr.harness = Some("opencode".to_string()); // matches legacy
        ovr.skill = Some("mp-runner".to_string());
        let resolved = resolve_with_legacy_fallback(
            Role::Runner,
            Some(&ovr),
            Some(&legacy_runner),
            None,
        )
        .expect("equal values must not conflict");
        assert_eq!(resolved.harness, "opencode");
    }

    #[test]
    fn legacy_fallback_overspecified_harness_conflict_is_typed_error() {
        // Autopilot says "pi", legacy says "opencode" — typed error.
        let legacy_runner = legacy("opencode", "anthropic/claude-opus-4-1");
        let mut ovr = RoleConfigOverride::empty();
        ovr.harness = Some("pi".to_string());
        let result = resolve_with_legacy_fallback(
            Role::Runner,
            Some(&ovr),
            Some(&legacy_runner),
            None,
        );
        match result {
            Err(RoleResolutionError::ConflictingHarnessFallback { role, autopilot, legacy }) => {
                assert_eq!(role, Role::Runner);
                assert_eq!(autopilot, "pi");
                assert_eq!(legacy, "opencode");
            }
            other => panic!("expected ConflictingHarnessFallback, got {other:?}"),
        }
    }

    #[test]
    fn legacy_fallback_overspecified_model_conflict_is_typed_error() {
        let legacy_runner = legacy("opencode", "anthropic/claude-sonnet-4-5");
        let mut ovr = RoleConfigOverride::empty();
        ovr.harness = Some("opencode".to_string()); // matches legacy
        ovr.model = Some("anthropic/claude-opus-4-1".to_string());
        let result = resolve_with_legacy_fallback(
            Role::Runner,
            Some(&ovr),
            Some(&legacy_runner),
            None,
        );
        assert!(matches!(
            result,
            Err(RoleResolutionError::ConflictingModelFallback { .. })
        ));
    }

    #[test]
    fn legacy_fallback_only_legacy_set_for_a_field_uses_legacy_value() {
        // Autopilot override only sets `skill`; harness and model
        // come from legacy. This is the most common migration
        // pattern.
        let legacy_runner = legacy("cursor", "anthropic/claude-opus-4-1");
        let mut ovr = RoleConfigOverride::empty();
        ovr.skill = Some("mp-runner".to_string());
        let resolved = resolve_with_legacy_fallback(
            Role::Runner,
            Some(&ovr),
            Some(&legacy_runner),
            None,
        )
        .expect("partial override + legacy should succeed");
        assert_eq!(resolved.harness, "cursor", "harness from legacy");
        assert_eq!(resolved.model.as_deref(), Some("anthropic/claude-opus-4-1"), "model from legacy");
        assert_eq!(resolved.skill, "mp-runner", "skill from override");
    }

    #[test]
    fn legacy_fallback_unsupported_reviewer_message_guides_operator() {
        let err = resolve_with_legacy_fallback(Role::Reviewer, None, None, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reviewer"),
            "error must name the role: {msg}"
        );
        assert!(
            msg.contains("autopilot.roles.reviewer"),
            "error must hint at the autopilot config path: {msg}"
        );
    }

    #[test]
    fn legacy_fallback_legacy_command_thinking_level_become_extras() {
        // The legacy `command` argv and `thinking_level` knobs are
        // not in the autopilot role schema — the function stuffs
        // them into `extras` under stable keys so a downstream
        // caller can pick them up.
        let mut legacy_runner = LegacyRoleConfig::default();
        legacy_runner.harness = Some("opencode".to_string());
        legacy_runner.command = Some(vec!["opencode".to_string(), "--flag".to_string()]);
        legacy_runner.thinking_level = Some("high".to_string());
        let resolved = resolve_with_legacy_fallback(
            Role::Runner,
            None,
            Some(&legacy_runner),
            None,
        )
        .expect("runner + legacy with command/thinking should succeed");
        assert_eq!(
            resolved.extras.get("legacy.command").map(String::as_str),
            Some(r#"["opencode","--flag"]"#),
            "command argv must be JSON-encoded under legacy.command"
        );
        assert_eq!(
            resolved.extras.get("legacy.thinking_level").map(String::as_str),
            Some("high")
        );
    }
}
