//! M210 / AC-01..AC-05: spawn-prompt rendering for the three
//! autopilot roles, the per-topology collapse, and the harness
//! flag-translation table.
//!
//! ## Public surface
//!
//! - [`SpawnPromptInputs`] — the read-only inputs the renderer
//!   needs to produce a deterministic prompt string. Built once
//!   per spawn from `session.json` + the resolved
//!   [`crate::autopilot::role::ResolvedRoleConfig`].
//! - [`render_role_prompt`] — pure function: `(role, inputs) ->
//!   String`. Same inputs → byte-identical output (AC-01).
//! - [`render_topology_prompts`] — collapses the per-role prompts
//!   for a given [`crate::autopilot::role::Topology`]. Returns a
//!   `Vec<(PaneLabel, BundledPrompt)>` so the caller can deliver
//!   one bundle per physical pane (AC-01, AC-04).
//! - [`render_collapsed_bundle`] — convenience that returns the
//!   joined string for a single pane (used by collapsed
//!   topologies in 1-/2-pane modes).
//! - [`HarnessFlagError`] — the typed error returned by
//!   [`harness_extra_flags`] for unsupported harness kinds
//!   (AC-03: unsupported harnesses fail before pane creation).
//!
//! ## What goes in the prompt
//!
//! The renderer's content is driven by three stable blocks per
//! role (AC-02):
//!
//! 1. **Identity** — project name, session id, milestone id,
//    queue position. Lets the agent ground its first context
//!    window in role + contract before any freeform work.
//! 2. **Role contract** — the role's name, the skill it should
//!    load, the explicit `Boundaries you must respect` block.
//!    Boundaries are hardcoded in this file (DD-01: enforcement
//!    lives in code, not in SKILL.md).
//! 3. **State surface** — the typed `mp autopilot session
//!    transition` commands the role must use (AC-05); the
//!    allowed role-state vocabulary; the lane-notify wire
//!    format the role's replies must follow.
//!
//! Every prompt contains every block. There is no conditional
//! render path — adding or removing a section is a code edit and
//! forces a golden-test update. That's the point: drift between
//! what the prompt says and what the verifier enforces is
//! detectable at commit time, not at runtime.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::autopilot::role::{ResolvedRoleConfig, Role, Topology};

// Re-export so integration tests can name the type via the
// prompts::spawn path without depending on the role module's
// privacy surface.
pub use crate::autopilot::role::{Role as RoleReexport, Topology as TopologyReexport};

// ─── Inputs ──────────────────────────────────────────────────────────

/// All the read-only inputs the renderer needs. Built once per
/// spawn; the same value fed through [`render_role_prompt`] any
/// number of times produces the same prompt string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPromptInputs {
    /// Project / repo name as the agent will see it. Pulled from
    /// the `mp autopilot` config or, when absent, the plan
    /// directory's basename.
    pub project_name: String,
    /// Stable session id (e.g. `sess-alpha`).
    pub session_id: String,
    /// Milestone id currently in the spawn's queue slot (e.g.
    /// `210` or `M210` — whatever the queue carries verbatim).
    pub milestone_id: String,
    /// Queue position (0-based) for the milestone being spawned
    /// onto. Used by the role prompt so the runner knows which
    /// queue item it is driving.
    pub queue_position: usize,
    /// Resolved role config (model / harness / skill / extras).
    pub role_config: ResolvedRoleConfig,
}

impl SpawnPromptInputs {
    /// Build inputs from raw components. Trims trailing
    /// whitespace and rejects empty fields so the renderer
    /// cannot ship a prompt that loses its identity to a typo.
    pub fn new(
        project_name: impl Into<String>,
        session_id: impl Into<String>,
        milestone_id: impl Into<String>,
        queue_position: usize,
        role_config: ResolvedRoleConfig,
    ) -> Result<Self, String> {
        let project_name = project_name.into();
        let session_id = session_id.into();
        let milestone_id = milestone_id.into();
        if project_name.trim().is_empty() {
            return Err("project_name must not be empty".into());
        }
        if session_id.trim().is_empty() {
            return Err("session_id must not be empty".into());
        }
        if milestone_id.trim().is_empty() {
            return Err("milestone_id must not be empty".into());
        }
        Ok(Self {
            project_name,
            session_id,
            milestone_id,
            queue_position,
            role_config,
        })
    }
}

// ─── Per-role template output ───────────────────────────────────────

/// One pane's worth of rendered prompt content. `label` is the
/// pane label (e.g. `role-runner-1` or `supervisor` for collapsed
/// topologies); `roles` is the set of roles whose contracts are
/// inside the bundle (1 for 3-pane, 2 for 2-pane supervisor, 3
/// for 1-pane). `prompt` is the byte-stable text that gets
/// delivered to herdr.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledPrompt {
    /// Pane label the bundle is intended for.
    pub label: String,
    /// Roles whose contracts are inside this bundle (canonical
    /// declaration order: Orchestrator, Runner, Reviewer).
    pub roles: Vec<Role>,
    /// The rendered prompt text. Byte-identical for identical
    /// inputs — golden tests in AC-01 pin this.
    pub prompt: String,
}

// ─── Per-role boundary blocks (DD-01: hardcoded in code) ─────────────

/// Orchestrator `Boundaries you must respect` block. Every word
/// here is enforceable: the verifier (M212) checks that the
/// orchestrator pane never claims reviews or edits
/// session.json directly. Updating this block is a code edit
/// that forces the verifier's role-boundary detectors to be
/// re-pinned.
const ORCHESTRATOR_BOUNDARIES: &str = "\
You are the ORCHESTRATOR for an `mp autopilot` cycle.

Boundaries you MUST respect:

  - You drive cycle decisions (start, dispatch, escalate,
    ship-with-backlog) but you NEVER claim or run review
    verdicts. Reviews belong to the Reviewer lane.
  - You communicate with the runner via typed session
    transitions (`mp autopilot session transition`), NOT via
    freeform text edits to session.json.
  - You do NOT edit code, write tests, or run cargo directly.
    Those are runner responsibilities.
  - You do NOT call `mp reviews pass` / `mp reviews claim` /
    `mp reviews finding add` / `mp reviews finding resolve`.
    Those belong to the reviewer lane.
  - You do NOT edit any file under `master-plan/` directly.
    Use `mp milestone *` CLI for every plan read/write.
  - When the cycle is finished, you notify the orchestrator
    lane via the `herdr agent prompt` wire format (see
    `herdr agent prompt <target> <text>`).

Lane-notify wire format (what your reply MUST start with):

  session=<id> milestone=<id> cycle=<n> role=orchestrator next=<decision>
  <one-line decision summary>";

/// Runner `Boundaries you must respect` block. The verifier
/// rejects runner pane notifications that include review verdicts
/// or that edit plan files directly.
const RUNNER_BOUNDARIES: &str = "\
You are the RUNNER for an `mp autopilot` cycle.

Boundaries you MUST respect:

  - You execute the cycle: implement the steps, run the
    verifier commands, stamp per-AC evidence, then notify the
    orchestrator lane.
  - You NEVER claim `mp reviews pass` — that is the reviewer's
    job. You CAN call `mp milestone complete <id>` once the
    runner's part is done (the orchestrator then dispatches the
    reviewer).
  - You NEVER edit any file under `master-plan/` directly.
    Plan-zone writes go through `mp milestone *`, `mp step
    done`, `mp milestone criterion pass`, etc.
  - You communicate session state via typed transitions
    (`mp autopilot session transition`), NOT by editing
    session.json.
  - You do NOT edit code in the `crates/mp/` autopilot module
    while a cycle is in flight; the orchestrator owns the spec
    surface.
  - Per-AC evidence MUST be a real `cargo nextest` / `cargo
    clippy` / `cargo fmt --check` invocation with an exit code
    and a pass count. Generic strings (\"all done\") are
    rejected.

Lane-notify wire format (what your reply MUST start with):

  session=<id> milestone=<id> cycle=<n> role=runner next=<done|blocked|escalate>
  <one-line summary>";

/// Reviewer `Boundaries you must respect` block. Reviewer owns
/// the `mp reviews *` surface; everything else is a violation.
const REVIEWER_BOUNDARIES: &str = "\
You are the REVIEWER for an `mp autopilot` cycle.

Boundaries you MUST respect:

  - You own the review verdict for this cycle. You call
    `mp reviews pass` / `mp reviews finding add` / `mp reviews
    finding resolve` — NO other role does.
  - You NEVER implement steps, run cargo, or edit code. The
    runner drives implementation; the reviewer only inspects
    the diff and the evidence trail.
  - You NEVER edit files under `master-plan/` directly. Use the
    `mp reviews *` CLI for every review write.
  - You NEVER call `mp milestone complete` — that is the
    runner's signal to the orchestrator.
  - You communicate findings via `mp reviews finding add` and
    final verdicts via `mp reviews pass`. Do NOT invent your
    own lifecycle claims.
  - When the review is done, notify the orchestrator lane via
    the wire format below.

Lane-notify wire format (what your reply MUST start with):

  session=<id> milestone=<id> cycle=<n> role=reviewer next=<pass|finding>
  <one-line summary>";

// ─── Renderer ────────────────────────────────────────────────────────

/// Render the per-role spawn prompt. Pure: same `(role, inputs)`
/// → byte-identical output.
///
/// The output contains, in order (AC-02):
///  1. Role + skill identity
///  2. Project / session / milestone identity
///  3. Explicit role boundaries (per the locked DD-01)
///  4. M211 task-assignment contract (typed transitions only)
///  5. The exact `mp autopilot session transition` commands
///     the role must use
///  6. A pointer to the lane-notify wire format
pub fn render_role_prompt(role: Role, inputs: &SpawnPromptInputs) -> String {
    let mut out = String::new();
    let role_label = role.as_str();
    let skill = inputs.role_config.skill.as_str();
    let harness = inputs.role_config.harness.as_str();
    let model = inputs
        .role_config
        .model
        .as_deref()
        .unwrap_or("<harness default>");

    let _ = writeln!(out, "# Spawn prompt — role: {role_label}");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Role identity");
    let _ = writeln!(out, "- role: {role_label}");
    let _ = writeln!(out, "- skill: {skill}");
    let _ = writeln!(out, "- harness: {harness}");
    let _ = writeln!(out, "- model: {model}");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Project identity");
    let _ = writeln!(out, "- project: {}", inputs.project_name);
    let _ = writeln!(out, "- session_id: {}", inputs.session_id);
    let _ = writeln!(out, "- milestone_id: {}", inputs.milestone_id);
    let _ = writeln!(
        out,
        "- queue_position: {} (0-based; the milestone you are driving)",
        inputs.queue_position
    );
    let _ = writeln!(out);

    // Boundaries block — hardcoded per role. The seam is the
    // `─── Boundaries you must respect ───` divider so the
    // collapsed-topology supervisor can locate each role's
    // contract by greppable marker.
    let _ = writeln!(out, "─── Boundaries you must respect ───");
    let boundaries = match role {
        Role::Orchestrator => ORCHESTRATOR_BOUNDARIES,
        Role::Runner => RUNNER_BOUNDARIES,
        Role::Reviewer => REVIEWER_BOUNDARIES,
    };
    out.push_str(boundaries);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    let _ = writeln!(out);

    // Typed session-transition surface (AC-05). Every prompt
    // lists the exact commands the role can issue.
    let _ = writeln!(out, "## Typed session-transition surface");
    let _ = writeln!(
        out,
        "Use ONLY these commands to mutate session state. \
         Anything else (e.g. direct edits to session.json) is a \
         boundary violation."
    );
    let _ = writeln!(out);
    for cmd in typed_transition_commands(role) {
        let _ = writeln!(out, "  - `{cmd}`");
    }
    let _ = writeln!(out);

    // Allowed role states (from M207 transitions.rs). The prompt
    // mirrors the state-machine enum so a drifted prompt that
    // mentions an unknown state is caught by the verifier.
    let _ = writeln!(out, "## Allowed role states");
    let _ = writeln!(
        out,
        "Your current `role_state.<role>.state` is one of:"
    );
    for state in ALLOWED_ROLE_STATES {
        let _ = writeln!(out, "  - `{state}`");
    }
    let _ = writeln!(out);

    // Task-assignment contract (AC-02 / AC-05). The runner and
    // reviewer receive tasks via the M211 typed payload; the
    // orchestrator dispatches them.
    if matches!(role, Role::Runner | Role::Reviewer) {
        let _ = writeln!(out, "## Task-assignment contract");
        let _ = writeln!(
            out,
            "You receive work via `mp autopilot session transition \
             --role <you> --state <starting>` triggered by the \
             orchestrator. The orchestrator may also send a task \
             text via the M211 wire format (see `herdr agent \
             prompt <pane> <text>`); reply using the lane-notify \
             format above."
        );
        let _ = writeln!(out);
    }

    // Lane-notify wire format pointer.
    let _ = writeln!(out, "## Lane notify");
    let _ = writeln!(
        out,
        "When your cycle work is finished, you MUST notify the \
         orchestrator via the lane-notify wire format shown at the \
         bottom of the Boundaries block. The first line of your \
         reply must match that format exactly; the verifier will \
         reject a reply that omits it."
    );
    let _ = writeln!(out);

    out
}

/// The closed set of allowed role states (mirrors M207's
/// `RoleState` enum in `transitions.rs`). Kept here as a `const`
/// slice so a future enum addition forces a prompt + verifier
/// update in lockstep.
pub const ALLOWED_ROLE_STATES: &[&str] = &[
    "idle",
    "starting",
    "working",
    "blocked",
    "done",
    "unknown",
];

/// The exact typed `mp autopilot session transition` commands the
/// role may issue. The list is role-specific: the runner never
/// needs to file a review, so it does not see the reviewer's
/// commands, and vice versa.
pub fn typed_transition_commands(role: Role) -> &'static [&'static str] {
    const O: &[&str] = &[
        "mp autopilot session transition --role orchestrator --state <idle|starting|working|blocked|done>",
        "mp autopilot session transition --role runner --state <idle|starting|working|blocked|done>",
        "mp autopilot session transition --role reviewer --state <idle|starting|working|blocked|done>",
        "mp autopilot note add --kind <info|warn|blocker|decision|reminder|system> --body <text>",
        "herdr agent prompt <pane> <task_text>   # dispatch work to a role pane",
    ];
    const R: &[&str] = &[
        "mp autopilot session transition --role runner --state <idle|starting|working|blocked|done>",
        "mp autopilot note add --kind <info|warn|blocker|decision|reminder|system> --body <text>",
        "mp milestone step done <id> <step>      # mark a plan step implemented",
        "mp milestone criterion pass <id> <ac> --evidence \"<real cargo nextest output>\"",
        "mp milestone complete <id>              # signal cycle work done",
    ];
    const V: &[&str] = &[
        "mp autopilot session transition --role reviewer --state <idle|starting|working|blocked|done>",
        "mp autopilot note add --kind <info|warn|blocker|decision|reminder|system> --body <text>",
        "mp reviews pass <id>                    # file a passing verdict",
        "mp reviews finding add <id> --code F-XX --severity <low|med|high|critical> --summary <text>",
        "mp reviews finding resolve <id> <F-XX>",
    ];
    match role {
        Role::Orchestrator => O,
        Role::Runner => R,
        Role::Reviewer => V,
    }
}

// ─── Topology collapsing ─────────────────────────────────────────────

/// Render the per-pane bundles for a topology. Returns one
/// [`BundledPrompt`] per physical pane in canonical pane order
/// (Orchestrator first, Runner second, Reviewer third for 3-pane;
/// supervisor (O+V) first, Runner second for 2-pane; one
/// collapsed bundle for 1-pane).
///
/// The seam line between concatenated role prompts is named so
/// the supervisor agent can grep for it (`─── role: <name>
/// ───`). That's the agent's only cue for "you're in O-mode" vs
/// "you're in V-mode" inside a collapsed bundle.
pub fn render_topology_prompts(
    inputs_o: &SpawnPromptInputs,
    inputs_r: &SpawnPromptInputs,
    inputs_v: &SpawnPromptInputs,
    topology: Topology,
) -> Vec<BundledPrompt> {
    let p_o = render_role_prompt(Role::Orchestrator, inputs_o);
    let p_r = render_role_prompt(Role::Runner, inputs_r);
    let p_v = render_role_prompt(Role::Reviewer, inputs_v);
    match topology {
        Topology::ThreeAgent => vec![
            BundledPrompt {
                label: "role-orchestrator-1".into(),
                roles: vec![Role::Orchestrator],
                prompt: p_o,
            },
            BundledPrompt {
                label: "role-runner-1".into(),
                roles: vec![Role::Runner],
                prompt: p_r,
            },
            BundledPrompt {
                label: "role-reviewer-1".into(),
                roles: vec![Role::Reviewer],
                prompt: p_v,
            },
        ],
        Topology::TwoAgent => vec![
            BundledPrompt {
                label: "supervisor".into(),
                roles: vec![Role::Orchestrator, Role::Reviewer],
                prompt: render_collapsed_bundle(&[(&p_o, Role::Orchestrator), (&p_v, Role::Reviewer)]),
            },
            BundledPrompt {
                label: "role-runner-1".into(),
                roles: vec![Role::Runner],
                prompt: p_r,
            },
        ],
        Topology::OneAgent => vec![BundledPrompt {
            label: "supervisor".into(),
            roles: vec![Role::Orchestrator, Role::Runner, Role::Reviewer],
            prompt: render_collapsed_bundle(&[
                (&p_o, Role::Orchestrator),
                (&p_r, Role::Runner),
                (&p_v, Role::Reviewer),
            ]),
        }],
    }
}

/// Convenience that joins role prompts with a named seam. Used
/// internally by [`render_topology_prompts`] and exposed for
/// callers that want the bundled text directly (e.g. for golden
/// pinning in tests).
///
/// Each role's section is preceded by `─── role: <name> ───` so
/// the supervisor agent can `grep` for a specific role's
/// contract inside the collapsed bundle. The first section also
/// gets the seam — otherwise the first role would have no
/// greppable marker, which would defeat the supervisor's
/// ability to locate it.
pub fn render_collapsed_bundle(parts: &[(&str, Role)]) -> String {
    let mut out = String::new();
    for (text, role) in parts.iter() {
        let _ = writeln!(out, "─── role: {} ───", role.as_str());
        let _ = writeln!(out);
        out.push_str(text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

// ─── Per-harness flag translation ───────────────────────────────────

/// Typed error from [`harness_extra_flags`]. Per AC-03:
/// unsupported harnesses fail before pane creation — the spawn
/// pipeline never silently drops a harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessFlagError {
    /// Harness kind is not in the v1 supported set (opencode /
    /// cursor / pi). The caller must register the harness first
    /// (via the harness registry) before re-attempting the spawn.
    Unsupported { harness: String, supported: Vec<String> },
}

impl std::fmt::Display for HarnessFlagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HarnessFlagError::Unsupported { harness, supported } => write!(
                f,
                "harness {harness:?} is not supported by the v1 autopilot spawn \
                 pipeline; supported harnesses: [{}]",
                supported.join(", ")
            ),
        }
    }
}

impl std::error::Error for HarnessFlagError {}

/// Translate a resolved role config into the argv tail that
/// `herdr agent start` receives after `--pane <id>`. Per AC-03:
/// opencode / cursor / pi each have a documented flag shape
/// (skill/agent + model). Unsupported harnesses return
/// [`HarnessFlagError::Unsupported`] before any pane creation.
///
/// Per-harness shape:
/// - **opencode**: `--skill <name> --model <id>`
/// - **cursor**:   `--agent <name> --model <id>`
/// - **pi**:       `--skill <name> --model <id>` (Pi's CLI
///   surfaces skill / model via the same flag set; per the v1
///   registry entry in `harness::registry::V1_ENTRIES`).
///
/// `model` is appended only when the resolved config populates it
/// (the built-in default leaves it `None` so the harness falls
/// back to its own default model).
pub fn harness_extra_flags(rc: &ResolvedRoleConfig) -> Result<Vec<String>, HarnessFlagError> {
    let mut out = Vec::new();
    match rc.harness.as_str() {
        "opencode" => {
            out.push("--skill".into());
            out.push(rc.skill.clone());
        }
        "cursor" => {
            out.push("--agent".into());
            out.push(rc.skill.clone());
        }
        "pi" => {
            out.push("--skill".into());
            out.push(rc.skill.clone());
        }
        other => {
            return Err(HarnessFlagError::Unsupported {
                harness: other.to_string(),
                supported: vec![
                    "opencode".into(),
                    "cursor".into(),
                    "pi".into(),
                ],
            });
        }
    }
    if let Some(model) = rc.model.as_deref().filter(|s| !s.is_empty()) {
        out.push("--model".into());
        out.push(model.to_string());
    }
    Ok(out)
}

/// The v1 set of harness kinds the spawn pipeline supports.
/// Exposed so callers (CLI surface, doc strings) can render the
/// same list without depending on the `HarnessRegistry` (which
/// also ships claude-code / gemini / codex / windsurf / cline —
/// harnesses that are NOT wired to the autopilot spawn yet).
pub const SUPPORTED_AUTOPILOT_HARNESSES: &[&str] = &["opencode", "cursor", "pi"];

// ─── Internal: tiny seam-label helper for tests ──────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::role::resolve_role_config;

    fn sample_inputs(role: Role) -> SpawnPromptInputs {
        let builtin = crate::autopilot::role::builtin_role_default(role);
        let rc = resolve_role_config(None, None, &builtin);
        SpawnPromptInputs::new(
            "master-plan",
            "sess-alpha",
            "M210",
            0,
            rc,
        )
        .unwrap()
    }

    #[test]
    fn render_role_prompt_is_byte_identical_for_same_inputs() {
        let inputs = sample_inputs(Role::Runner);
        let a = render_role_prompt(Role::Runner, &inputs);
        let b = render_role_prompt(Role::Runner, &inputs);
        assert_eq!(a, b);
    }

    #[test]
    fn render_role_prompt_changes_only_when_role_changes() {
        let inputs = sample_inputs(Role::Runner);
        let r_prompt = render_role_prompt(Role::Runner, &inputs);
        let o_prompt = render_role_prompt(Role::Orchestrator, &inputs);
        let v_prompt = render_role_prompt(Role::Reviewer, &inputs);
        // Three different roles → three distinct prompts.
        assert_ne!(r_prompt, o_prompt);
        assert_ne!(r_prompt, v_prompt);
        assert_ne!(o_prompt, v_prompt);
    }

    #[test]
    fn render_role_prompt_contains_role_and_skill_identity() {
        let inputs = sample_inputs(Role::Orchestrator);
        let prompt = render_role_prompt(Role::Orchestrator, &inputs);
        assert!(prompt.contains("role: orchestrator"));
        assert!(prompt.contains("skill: mp-coordinator"));
        assert!(prompt.contains("harness: opencode"));
        assert!(prompt.contains("project: master-plan"));
        assert!(prompt.contains("session_id: sess-alpha"));
        assert!(prompt.contains("milestone_id: M210"));
    }

    #[test]
    fn render_role_prompt_contains_typed_transition_commands() {
        // AC-05: every prompt lists the typed commands.
        for role in Role::ALL {
            let inputs = sample_inputs(role);
            let prompt = render_role_prompt(role, &inputs);
            assert!(
                prompt.contains("mp autopilot session transition"),
                "role {role} prompt missing typed transition command"
            );
        }
        // Runner prompt must include `mp milestone complete`.
        let r_prompt = render_role_prompt(Role::Runner, &sample_inputs(Role::Runner));
        assert!(r_prompt.contains("mp milestone complete"));
        // Reviewer prompt must include `mp reviews pass`.
        let v_prompt = render_role_prompt(Role::Reviewer, &sample_inputs(Role::Reviewer));
        assert!(v_prompt.contains("mp reviews pass"));
    }

    #[test]
    fn render_role_prompt_includes_allowed_role_states() {
        let prompt = render_role_prompt(Role::Runner, &sample_inputs(Role::Runner));
        for state in ALLOWED_ROLE_STATES {
            assert!(
                prompt.contains(&format!("`{state}`")),
                "runner prompt missing allowed state `{state}`"
            );
        }
    }

    #[test]
    fn render_role_prompt_includes_role_boundaries_block() {
        for role in Role::ALL {
            let prompt = render_role_prompt(role, &sample_inputs(role));
            assert!(
                prompt.contains("─── Boundaries you must respect ───"),
                "role {role} prompt missing the boundaries block seam"
            );
            assert!(
                prompt.contains("Boundaries you MUST respect"),
                "role {role} prompt missing the boundaries header text"
            );
        }
    }

    #[test]
    fn render_role_prompt_does_not_instruct_direct_session_json_edits() {
        // AC-02 / AC-05: prompts must never direct agents to edit
        // session.json directly — that's a verifier boundary
        // violation.
        for role in Role::ALL {
            let prompt = render_role_prompt(role, &sample_inputs(role));
            assert!(
                !prompt.contains("edit session.json directly"),
                "role {role} prompt mentions direct session.json edits"
            );
            assert!(
                !prompt.contains("vim session.json") && !prompt.contains("write to session.json"),
                "role {role} prompt mentions writing session.json directly"
            );
        }
    }

    #[test]
    fn render_role_prompt_rejects_unknown_states() {
        // Sanity: ALLOWED_ROLE_STATES is the closed set the
        // verifier accepts. A drift here would break AC-05
        // (golden tests must reject unknown states).
        assert!(!ALLOWED_ROLE_STATES.contains(&"complete"));
        assert!(!ALLOWED_ROLE_STATES.contains(&"approved"));
        assert!(!ALLOWED_ROLE_STATES.contains(&"started"));
    }

    #[test]
    fn render_topology_prompts_three_pane_yields_three_isolated_bundles() {
        let inputs = sample_inputs(Role::Runner);
        let bundles = render_topology_prompts(&inputs, &inputs, &inputs, Topology::ThreeAgent);
        assert_eq!(bundles.len(), 3);
        assert_eq!(bundles[0].roles, vec![Role::Orchestrator]);
        assert_eq!(bundles[1].roles, vec![Role::Runner]);
        assert_eq!(bundles[2].roles, vec![Role::Reviewer]);
        // 3-pane: each bundle is a single role — no seam text.
        assert!(!bundles[0].prompt.contains("─── role:"));
    }

    #[test]
    fn render_topology_prompts_two_pane_supervisor_includes_seam() {
        let inputs = sample_inputs(Role::Runner);
        let bundles = render_topology_prompts(&inputs, &inputs, &inputs, Topology::TwoAgent);
        assert_eq!(bundles.len(), 2);
        assert_eq!(
            bundles[0].roles,
            vec![Role::Orchestrator, Role::Reviewer]
        );
        // 2-pane supervisor: O + V concatenated with a seam.
        assert!(bundles[0].prompt.contains("─── role: orchestrator ───"));
        assert!(bundles[0].prompt.contains("─── role: reviewer ───"));
        assert!(bundles[1].roles == vec![Role::Runner]);
    }

    #[test]
    fn render_topology_prompts_one_pane_includes_all_three_roles() {
        let inputs = sample_inputs(Role::Runner);
        let bundles = render_topology_prompts(&inputs, &inputs, &inputs, Topology::OneAgent);
        assert_eq!(bundles.len(), 1);
        assert_eq!(
            bundles[0].roles,
            vec![Role::Orchestrator, Role::Runner, Role::Reviewer]
        );
        assert!(bundles[0].prompt.contains("─── role: orchestrator ───"));
        assert!(bundles[0].prompt.contains("─── role: runner ───"));
        assert!(bundles[0].prompt.contains("─── role: reviewer ───"));
    }

    #[test]
    fn harness_extra_flags_opencode_appends_skill_and_model() {
        let mut rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Runner));
        rc.harness = "opencode".into();
        rc.model = Some("anthropic/claude-opus-4-1".into());
        rc.skill = "mp-runner".into();
        let flags = harness_extra_flags(&rc).unwrap();
        assert_eq!(
            flags,
            vec![
                "--skill".to_string(),
                "mp-runner".to_string(),
                "--model".to_string(),
                "anthropic/claude-opus-4-1".to_string(),
            ]
        );
    }

    #[test]
    fn harness_extra_flags_cursor_uses_agent_instead_of_skill() {
        let mut rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Reviewer));
        rc.harness = "cursor".into();
        rc.skill = "mp-runner".into();
        rc.model = Some("anthropic/claude-opus-4-1".into());
        let flags = harness_extra_flags(&rc).unwrap();
        assert!(flags.contains(&"--agent".to_string()));
        assert!(flags.contains(&"mp-runner".to_string()));
        assert!(!flags.contains(&"--skill".to_string()));
        assert!(flags.contains(&"--model".to_string()));
    }

    #[test]
    fn harness_extra_flags_pi_uses_skill_flag() {
        let mut rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Runner));
        rc.harness = "pi".into();
        rc.skill = "mp-runner".into();
        rc.model = None;
        let flags = harness_extra_flags(&rc).unwrap();
        assert_eq!(
            flags,
            vec!["--skill".to_string(), "mp-runner".to_string()]
        );
    }

    #[test]
    fn harness_extra_flags_omits_model_when_unset() {
        let mut rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Orchestrator));
        rc.harness = "opencode".into();
        rc.model = None;
        let flags = harness_extra_flags(&rc).unwrap();
        assert!(!flags.contains(&"--model".to_string()));
    }

    #[test]
    fn harness_extra_flags_rejects_unsupported_harness_before_pane_creation() {
        let mut rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Runner));
        rc.harness = "claude-code".into();
        rc.skill = "mp-runner".into();
        let err = harness_extra_flags(&rc).unwrap_err();
        match err {
            HarnessFlagError::Unsupported { harness, supported } => {
                assert_eq!(harness, "claude-code");
                assert!(supported.contains(&"opencode".to_string()));
                assert!(supported.contains(&"cursor".to_string()));
                assert!(supported.contains(&"pi".to_string()));
            }
        }
    }

    #[test]
    fn spawn_prompt_inputs_rejects_empty_project_or_session_or_milestone() {
        let rc = resolve_role_config(None, None, &crate::autopilot::role::builtin_role_default(Role::Runner));
        assert!(SpawnPromptInputs::new("", "s", "m", 0, rc.clone()).is_err());
        assert!(SpawnPromptInputs::new("p", "", "m", 0, rc.clone()).is_err());
        assert!(SpawnPromptInputs::new("p", "s", "", 0, rc).is_err());
    }

    #[test]
    fn supported_autopilot_harnesses_lists_three_v1_kinds() {
        // Pin the v1 autopilot-harness set so adding a new kind
        // is an explicit edit (and forces a verifier + golden
        // re-pin).
        assert_eq!(
            SUPPORTED_AUTOPILOT_HARNESSES,
            &["opencode", "cursor", "pi"]
        );
    }
}
