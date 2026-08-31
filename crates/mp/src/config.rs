use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub workflow: WorkflowConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub archive: ArchiveConfig,
    #[serde(default)]
    pub next: NextConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub planning: PlanningConfig,
    /// M149: per-role agent configuration consumed by `mp watch`. Optional
    /// end-to-end — `mp` and `raul` build and run with no `[agent]`
    /// section present; only `mp watch` startup requires it.
    #[serde(default)]
    pub agent: AgentConfig,
    /// M154: review-side integrations. Default-empty; the section is
    /// opt-in per project. When `review.hunk = true`, `mp reviews hunk`
    /// emits hunk-compatible JSON (live batch + agent-context sidecar)
    /// and the `mp-coordinator` skill applies findings with `--file` /
    /// `--line` anchoring at the stage-8 review boundary.
    #[serde(default)]
    pub review: ReviewConfig,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub keybinds: std::collections::BTreeMap<String, String>,
    /// M182 S4: raul sort-rebind per-lane preferences. The section is
    /// written by raul's `mp config set sort.<lane> <sortkey>` flow on
    /// confirm; `mp` itself never reads it (the TUI surface is the
    /// only consumer). `BTreeMap<String, String>` so the section
    /// serializes deterministically across writes — the
    /// golden-fixture tests assert the exact `{"lane": "sortkey"}`
    /// shape.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sort: std::collections::BTreeMap<String, String>,
}

/// M154: review-side integrations. The two knobs here gate the hunk
/// export pipeline (`mp reviews hunk <M>`):
///
/// - `hunk: bool` (default `false`) — opt-in flag. When `true`,
///   `mp reviews hunk` emits both the live batch (stdout) and the
///   agent-context sidecar (`--file <path>`); without the flag,
///   `mp reviews hunk` errors with "review.hunk is false in config —
///   set `[review] hunk` = true to export".
/// - `hunk_author: String` (default `"mp"`) — the author string baked
///   into every exported annotation. hunk renders this as the comment
///   author; project agents can override (e.g. `"mp-coordinator"`,
///   `"reviewer:alice"`) for traceability.
///
/// Both fields are advisory — they don't gate any other mp behavior,
/// they only enable the hunk export. The on-disk shape is
/// `[review] hunk = true` and `[review] hunk_author = "..."`; the
/// dotted-key access (`review.hunk`, `review.hunk_author`) follows
/// the same convention as `agent.automation.*`.
///
/// The custom `Default` impl populates `hunk_author = "mp"` so the
/// in-memory struct matches the on-disk default (otherwise serde's
/// `#[serde(default = "...")]` only fires on deserialization, leaving
/// a freshly-constructed `ProjectConfig::default()` with an empty
/// `hunk_author`). `mp config show` round-trips through the latter
/// path, and the golden json-shape fixture must show `"mp"` not `""`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewConfig {
    #[serde(default)]
    pub hunk: bool,
    #[serde(default = "default_review_hunk_author")]
    pub hunk_author: String,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            hunk: false,
            hunk_author: default_review_hunk_author(),
        }
    }
}

fn default_review_hunk_author() -> String {
    "mp".to_string()
}

/// M149 + M147: per-role agent configuration consumed by `mp watch`
/// (M149) and project-level automation policy consulted at handoff
/// boundaries (M147). The three sub-sections live in the same struct
/// so the on-disk `[agent]` section carries every agent knob — `runner`
/// and `coordinator` for M149, `automation` for M147 — and the
/// user-facing dotted-key set is `agent.<role>.<field>` /
/// `agent.automation.<field>`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub runner: RoleConfig,
    #[serde(default)]
    pub coordinator: RoleConfig,
    /// M147: project-level automation knobs. Each field has a sensible
    /// default that produces the legacy "ad-hoc per-session" behavior,
    /// so a freshly initialized project sees no behavior change. Agents
    /// consult these knobs at handoff boundaries (`mp-runner` after
    /// execute, `mp-coordinator` at review) and branch on the values —
    /// the CLI records the policy but does NOT gate commands on it
    /// (enforcement lives in the skills, not in `mp`).
    #[serde(default)]
    pub automation: AgentAutomationConfig,
}

/// M147: project-level automation knobs. See [`AgentConfig`] for the
/// owning `[agent]` section. The four fields are independent booleans
/// or enums — defaults reproduce today's per-session behavior; opt in
/// by setting any combination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentAutomationConfig {
    /// M147 default `false`: runner commits after `mp milestone complete`
    /// when `true`. The runner skill reads this at the (b) hand-off.
    pub commit_after_execute: Option<bool>,
    /// M147 default `false`: coordinator pushes the runner's branch at
    /// the (c) hand-off (stage 8 → 9) when `true`.
    pub push_after_review: Option<bool>,
    /// M147 default `"current"` (no branch cut): one of
    /// `per-milestone|current|none`. The runner skill honors this at
    /// claim time (stage 5).
    pub branch_strategy: Option<String>,
    /// M147 default `"none"` (record only): one of
    /// `none|low|medium|high|all`. Threshold semantics — see
    /// [`SeverityRank`] and [`AgentAutomationConfig::should_remediate`].
    /// The coordinator skill applies the threshold at stage 8 review.
    pub auto_remediate: Option<String>,
}

/// M147: known `automation.branch_strategy` values. Mirroring
/// `UI_THEMES` / `WATCH_HARNESSES` consts so the apply-time gate and
/// `mp config validate` reject typos uniformly. `current` is the
/// default — agents work on whichever branch they're already on.
pub const BRANCH_STRATEGIES: &[&str] = &["per-milestone", "current", "none"];

/// M147: known `automation.auto_remediate` values. `all` is an alias
/// for `low` (every severity); kept distinct so configs read clearly.
pub const AUTO_REMEDIATE_VALUES: &[&str] = &["none", "low", "medium", "high", "all"];

/// M147: finding-severity rank used by
/// [`AgentAutomationConfig::should_remediate`] and the coordinator
/// skill's threshold policy. The ordering is
/// `none < low < medium < high` — `none` is the sentinel "do not
/// remediate at all (record only)" and the three real findings
/// severities ([`mp-model`](crates/mp-model)'s `Finding::severity`)
/// sort above it. `auto_remediate` names the MINIMUM severity to
/// remediate, so a config of `medium` covers `medium` and `high`
/// findings but leaves `low` findings as record-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SeverityRank {
    /// Sentinel — never auto-remediate. Use for `auto_remediate = "none"`
    /// or when the policy is "record findings, do not act".
    None = 0,
    /// `Finding::severity = "low"` (alias for the `auto_remediate = "all"`
    /// value, which the config surface exposes for readability).
    Low = 1,
    /// `Finding::severity = "medium"` — also auto-remediates `high`.
    Medium = 2,
    /// `Finding::severity = "high"` — the strongest setting; only the
    /// most severe findings trigger auto-remediation.
    High = 3,
}

impl SeverityRank {
    /// M147: parse the `automation.auto_remediate` config value
    /// (or any finding severity string) into a [`SeverityRank`].
    /// Unknown values map to `None` — the apply-time gate already
    /// rejects bad values via `set`, but `should_remediate` is also
    /// called on raw `Finding::severity` strings from the review
    /// log, where the same forgiving default keeps a typo from
    /// incorrectly triggering remediation.
    pub fn from_config_value(value: &str) -> Self {
        match value {
            "none" => SeverityRank::None,
            "low" | "all" => SeverityRank::Low,
            "medium" => SeverityRank::Medium,
            "high" => SeverityRank::High,
            // Unknown threshold or finding severity — default to
            // "do not auto-remediate". The CLI has already validated
            // the config-side value via `AUTO_REMEDIATE_VALUES`, so
            // this branch is only hit for finding-side strings that
            // don't match the documented set.
            _ => SeverityRank::None,
        }
    }
}

impl AgentAutomationConfig {
    /// M147: the current `auto_remediate` threshold, parsed as a
    /// [`SeverityRank`]. Defaults to [`SeverityRank::None`] for
    /// "record only" when the config field is unset.
    pub fn auto_remediate_threshold(&self) -> SeverityRank {
        SeverityRank::from_config_value(self.auto_remediate.as_deref().unwrap_or("none"))
    }

    /// M147: should a finding at `severity` be auto-remediated given
    /// the current `auto_remediate` threshold? Threshold semantics:
    /// the threshold names the MINIMUM severity to remediate, so
    /// `medium` covers `medium` and `high` findings.
    ///
    /// `severity` accepts the same strings as
    /// `Finding::severity` (`"low" | "medium" | "high"`); unknown
    /// values are treated as `None` (record only) so a finding log
    /// with a stale label can never silently trigger remediation.
    pub fn should_remediate(&self, severity: &str) -> bool {
        let threshold = self.auto_remediate_threshold();
        let finding = SeverityRank::from_config_value(severity);
        threshold != SeverityRank::None && finding >= threshold
    }
}

/// Configuration for a single agent role (runner or coordinator). All
/// fields optional: a freshly initialized project has no `[agent]`
/// section and `mp watch` will surface the missing fields at startup
/// precondition time (S0).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleConfig {
    /// Harness id (opencode|pi|cursor in v1). Validated against
    /// [`WATCH_HARNESSES`] on `mp config set`.
    pub harness: Option<String>,
    /// Full argv for `herdr agent start` (e.g. `["opencode"]`). When
    /// unset, `mp watch` derives the command from `harness` via the
    /// harness registry (see M151).
    pub command: Option<Vec<String>>,
    pub model: Option<String>,
    /// Harness-specific thinking-level hint. No v1 consumer; stored as-is.
    pub thinking_level: Option<String>,
}

/// Harnesses with first-class `mp watch` support in v1. Re-export
/// of [`crate::harness::SUPPORTED_NAMES`] — the registry is the
/// single source of truth, this alias keeps `mp config set`'s
/// compile-time validation in lockstep without forcing every
/// consumer to depend on the `harness::registry` path. Add a new
/// harness by extending `SUPPORTED_NAMES` plus the matching
/// `HarnessEntry` in `harness/registry.rs`; the array is `const`
/// so both lists land together at compile time.
pub const WATCH_HARNESSES: &[&str] = crate::harness::SUPPORTED_NAMES;

/// M156 ext-review F-14: known `ui.theme` values. Mirrors the
/// `Palette::ALL` list in `raul::theme` (mp cannot depend on raul,
/// so the names are duplicated here deliberately — same rationale
/// as `KEYBIND_ACTIONS`). A typo like `moxha` would otherwise pass
/// validate and silently no-op at runtime via
/// `Palette::by_name(name)` returning `None`.
pub const UI_THEMES: &[&str] = &["mocha", "macchiato", "frappe", "latte", "dracula"];

/// The keybindable action names accepted under the `[keybinds]` section.
///
/// M138: this list mirrors `raul::tui::keybinds::Keybinds`. raul owns the
/// dispatch semantics; mp owns the config schema and does not depend on raul,
/// so the action names are duplicated here deliberately. The list is the
/// validation surface for `mp config set keybinds.<action>` and the shared
/// prerequisite for M140 (the Settings TUI writes the same section).
///
/// M200: `focus_content` is no longer user-rebindable — it is a TUI-internal
/// reserved action. The list mirrors only the user-rebindable subset of
/// `Keybinds`; the `focus_content` field stays on the struct but cannot be
/// overridden through config (deprecated `keybinds.focus_content` lines in
/// a project config surface a non-blocking deprecation warning).
pub const KEYBIND_ACTIONS: &[&str] = &[
    "quit",
    "up",
    "down",
    "page_up",
    "page_down",
    "enter",
    "escape",
    "help",
    "filter",
    "hide_done",
    "create_annotation",
    "resolve",
    "reopen",
    "approve",
    "review_menu",
    "open_settings",
    "toggle_watch",
    "toggle_tab_focus",
    "previous_lane",
    "next_lane",
    "refresh",
];

/// Canonical default values for user-rebindable keybinds, paired with the
/// matching `KEYBIND_ACTIONS` entry. Each value is the comma-separated
/// canonical form (`"Ctrl-R"`, `"Left, BackTab"`) that `mp config get`
/// surfaces when the user has not customized the action — matching what
/// raul would apply at runtime via `Keybinds::default()`.
///
/// mp cannot depend on `raul` (where `Keybinds::default()` lives), so the
/// canonical strings are duplicated here. The format matches the human-
/// readable form documented in CHANGELOG.md; the on-disk storage form is
/// the raw combo string the user typed (`parse_key_combo` parses both).
///
/// M200: `refresh` moved from `r` to `Ctrl-R`; `previous_lane` dropped the
/// `h` alias (kept `Left` and `BackTab`). The list excludes `focus_content`,
/// which is no longer user-rebindable.
pub const KEYBIND_DEFAULTS: &[(&str, &str)] = &[
    ("quit", "q, Q"),
    ("up", "Up, k"),
    ("down", "Down, j"),
    ("page_up", "PageUp"),
    ("page_down", "PageDown"),
    ("enter", "Enter"),
    ("escape", "Esc"),
    ("help", "?"),
    ("filter", "f"),
    ("hide_done", "h"),
    ("create_annotation", "A"),
    ("resolve", "r"),
    ("reopen", "R"),
    ("approve", "p"),
    ("review_menu", "m"),
    ("open_settings", "Ctrl-O"),
    // M200: dropped the `h` alias (vim-style conflict with `hide_done`).
    ("previous_lane", "Left, BackTab"),
    ("next_lane", "Right, l, Tab"),
    // M200: refresh default moved from `r` to `Ctrl-R` (no longer collides
    // with `resolve` at the default layer).
    ("refresh", "Ctrl-R"),
    ("next_section", "]"),
    ("prev_section", "["),
    ("next_item", "n"),
    ("prev_item", "p"),
    ("lifecycle_filter", "F"),
    ("grooming_preset", "g"),
    ("search", "/"),
    ("cycle_sort", "o"),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub artifacts: ArtifactsConfig,
    #[serde(default)]
    pub plan: PlanLocationConfig,
    #[serde(default)]
    pub gates: GatesConfig,
    #[serde(default)]
    pub session: SessionWorkflowConfig,
    #[serde(default)]
    pub steps: StepsConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepsConfig {
    pub code_review: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactsConfig {
    pub brief: Option<bool>,
    pub backlog: Option<bool>,
    #[serde(default)]
    pub milestones: MilestonesArtifact,
    pub tracks: Option<bool>,
    pub ideas: Option<bool>,
    pub decisions: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MilestonesArtifact {
    Flag(bool),
    Mode(String),
}

impl Default for MilestonesArtifact {
    fn default() -> Self {
        MilestonesArtifact::Flag(true)
    }
}

impl MilestonesArtifact {
    pub fn is_session(&self) -> bool {
        matches!(self, MilestonesArtifact::Mode(s) if s == "session")
    }

    pub fn enabled(&self) -> bool {
        match self {
            MilestonesArtifact::Flag(b) => *b,
            MilestonesArtifact::Mode(s) => s == "session" || s == "true",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanLocationConfig {
    pub in_repo: Option<bool>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatesConfig {
    pub strictness: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionWorkflowConfig {
    pub auto_bind_branch: Option<bool>,
    pub archive_on_merge: Option<bool>,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub milestone_prefix: Option<String>,
}

/// raul UI preferences. Owned by mp config (raul is read-only) so they persist
/// without raul writing files. All fields optional; raul applies its own defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    pub color: Option<bool>,
    pub icons: Option<String>,
    pub theme: Option<String>,
    pub hide_done: Option<bool>,
    /// M198 WP1: when true, raul's tab bar includes a Watch lane.
    /// Default `false` so the operator has to opt in to the
    /// `mp watch` TUI surface (the agent side stays untouched —
    /// the `mp` binary's `mp watch` command is independent of
    /// this flag). Stored as `Option<bool>` so we can tell
    /// "never set" from "explicitly set to false"; the operator
    /// default is `false`.
    pub show_watch_tab: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub auto_purge_days: Option<u32>,
    pub archive_on_milestone_delete: Option<bool>,
    pub archive_on_track_cancel: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NextConfig {
    pub prefer: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitConfig {
    pub auto_commit: Option<bool>,
    pub auto_push: Option<bool>,
    pub commit_on_milestone_complete: Option<bool>,
    pub commit_message_template: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanningConfig {
    pub require_min_out_of_scope: Option<u32>,
    pub require_min_acceptance_criteria: Option<u32>,
}

impl ProjectConfig {
    pub fn archive_on_milestone_delete(&self) -> bool {
        self.archive.archive_on_milestone_delete.unwrap_or(true)
    }

    pub fn archive_on_track_cancel(&self) -> bool {
        self.archive.archive_on_track_cancel.unwrap_or(true)
    }

    pub fn next_prefer(&self) -> &str {
        self.next.prefer.as_deref().unwrap_or("milestone")
    }

    pub fn strictness(&self) -> &str {
        self.workflow
            .gates
            .strictness
            .as_deref()
            .unwrap_or("relaxed")
    }

    pub fn auto_bind_branch(&self) -> bool {
        self.workflow.session.auto_bind_branch.unwrap_or(true)
    }

    pub fn focus_session(&self) -> Option<&str> {
        self.workflow
            .session
            .focus
            .as_deref()
            .filter(|s| !s.is_empty())
    }

    pub fn min_out_of_scope(&self) -> usize {
        self.planning.require_min_out_of_scope.unwrap_or(2) as usize
    }

    pub fn plan_location(&self) -> &str {
        self.workflow
            .plan
            .location
            .as_deref()
            .unwrap_or("master-plan")
    }

    pub fn profile(&self) -> &str {
        if self.workflow.profile.is_empty() {
            "full"
        } else {
            &self.workflow.profile
        }
    }

    pub fn git_auto_commit(&self) -> bool {
        self.git.auto_commit.unwrap_or(false)
    }

    pub fn git_commit_on_milestone_complete(&self) -> bool {
        self.git.commit_on_milestone_complete.unwrap_or(false)
    }

    pub fn should_git_commit_on_milestone_complete(&self) -> bool {
        self.git_auto_commit() || self.git_commit_on_milestone_complete()
    }

    pub fn git_auto_push(&self) -> bool {
        self.git.auto_push.unwrap_or(false)
    }

    pub fn code_review_enabled(&self) -> bool {
        self.workflow.steps.code_review.unwrap_or(false)
    }

    // --- M147: [agent.automation] accessors -------------------------------
    // Defaults intentionally mirror the legacy ad-hoc behavior so a fresh
    // config is a no-op; agents opt INTO automation by setting the knobs.

    /// M147: should the runner commit at the (b) hand-off (after `complete`)?
    pub fn commit_after_execute(&self) -> bool {
        self.agent.automation.commit_after_execute.unwrap_or(false)
    }

    /// M147: should the coordinator push at the (c) hand-off (stage 8 → 9)?
    pub fn push_after_review(&self) -> bool {
        self.agent.automation.push_after_review.unwrap_or(false)
    }

    /// M147: branch strategy at claim time. Default `current` — work on
    /// the branch the session is already on; no `git checkout -b`.
    pub fn automation_branch_strategy(&self) -> &str {
        self.agent
            .automation
            .branch_strategy
            .as_deref()
            .unwrap_or("current")
    }

    /// M147: auto-remediate threshold at stage 8 review. Default `none` —
    /// record findings via `mp reviews finding` without acting on them.
    pub fn automation_auto_remediate(&self) -> &str {
        self.agent
            .automation
            .auto_remediate
            .as_deref()
            .unwrap_or("none")
    }

    /// Returns the runner role config (or empty default when no `[agent]`
    /// section is present). Does not apply any defaults beyond what's
    /// stored on disk — `mp watch` startup resolves unset fields.
    pub fn runner_config(&self) -> &RoleConfig {
        &self.agent.runner
    }

    pub fn coordinator_config(&self) -> &RoleConfig {
        &self.agent.coordinator
    }

    // --- M154: [review] accessors ------------------------------------------

    /// M154: should `mp reviews hunk <M>` emit hunk-compatible output?
    /// Default `false` — hunk export is opt-in per project (the
    /// design_decisions note: "hunk is an external install and an
    /// extra step in the review flow"). Projects set
    /// `[review] hunk = true` to enable; everything else reads `false`
    /// and the export command errors with a clear hint.
    pub fn review_hunk_enabled(&self) -> bool {
        self.review.hunk
    }

    /// M154: the author string baked into hunk-exported annotations.
    /// Default `"mp"` so a project that opts in without setting
    /// `hunk_author` still gets a stable identifier; project agents
    /// override (e.g. `"mp-coordinator"`, `"reviewer:alice"`) when
    /// they want traceability back to a specific reviewer.
    pub fn review_hunk_author(&self) -> &str {
        if self.review.hunk_author.is_empty() {
            "mp"
        } else {
            &self.review.hunk_author
        }
    }
}

pub fn default_project_config_toml() -> String {
    r#"[archive]
auto_purge_days = 0
archive_on_milestone_delete = true
archive_on_track_cancel = true

[next]
prefer = "milestone"

# M149 mp watch agent configuration. All fields optional; uncomment and
# fill to drive `mp watch <ids...>`. Harness must be one of opencode|pi|cursor.
#
# [agent.runner]
# harness = "opencode"
# command = ["opencode"]
# model = "claude-opus-4"
# thinking_level = "medium"
#
# [agent.coordinator]
# harness = "opencode"
# command = ["opencode"]

# M147 agent automation policy. Consulted by `mp-runner` (post-execute)
# and `mp-coordinator` (review) at handoff boundaries — see the
# `mp-flow` skill for hand-off points. Defaults reproduce today's
# ad-hoc per-session behavior; opt in by uncommenting.
#
# [agent.automation]
# commit_after_execute = false
# push_after_review = false
# branch_strategy = "current"        # per-milestone | current | none
# auto_remediate = "none"            # none | low | medium | high | all

# M154 review-side integrations. The hunk export is opt-in (default
# off) — projects that have hunk installed on PATH and want inline
# agent annotations at the review boundary set
# `[review] hunk = true`. `hunk_author` overrides the default "mp"
# identifier baked into every exported annotation.
#
# [review]
# hunk = false
# hunk_author = "mp"
"#
    .to_string()
}

pub fn profile_config_json(profile: &str) -> anyhow::Result<String> {
    let rel = match profile {
        "full" => "templates/defaults/config.full.json",
        "hybrid" => "templates/defaults/config.hybrid.json",
        "session" => "templates/defaults/config.session.json",
        _ => anyhow::bail!("unknown profile: {profile} (expected full, hybrid, or session)"),
    };
    crate::assets::read_embedded(rel)
}
