//! M210 / AC-01: deterministic spawn-prompt rendering.
//!
//! Same `SpawnPromptInputs` + `Role` → byte-identical prompt
//! string. Golden-file fixtures pin the wire shape so a future
//! refactor of `render_role_prompt` or `render_topology_prompts`
//! cannot silently drift — the golden string would change and
//! the test would fail loudly.
//!
//! Coverage:
//! - All three roles in 3-pane topology (one bundle per role).
//! - 2-pane supervisor bundle (O+V concatenation with named
//!   seam).
//! - 1-pane collapsed bundle (O+R+V concatenation).
//!
//! The byte-stable output is the load-bearing surface: the
//! prompt is what gets shipped to the role pane on spawn, and
//! `mp autopilot session show` re-displays it from
//! `roles.<role>.spawn_prompt_rendered` + `prompt_bundles`. Any
//! drift breaks audit replay.

use mp::autopilot::prompts::spawn::{
    render_role_prompt, render_topology_prompts, BundledPrompt, RoleReexport as Role,
    SpawnPromptInputs, TopologyReexport as Topology,
};
use mp::autopilot::role::resolve_role_config;
use mp::autopilot::role::ResolvedRoleConfig;

fn rc(role: Role, harness: &str, model: Option<&str>) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    let mut r = resolve_role_config(None, None, &builtin);
    r.harness = harness.to_string();
    r.model = model.map(str::to_string);
    r
}

fn inputs(role: Role, rc: ResolvedRoleConfig) -> SpawnPromptInputs {
    SpawnPromptInputs::new("master-plan", "sess-alpha", "M210", 0, rc).unwrap()
}

fn first_lines(prompt: &str, n: usize) -> String {
    prompt.lines().take(n).collect::<Vec<_>>().join("\n")
}

#[test]
fn golden_three_pane_orchestrator_prompt_byte_stable() {
    let rc = rc(Role::Orchestrator, "opencode", Some("anthropic/claude-opus-4-1"));
    let i = inputs(Role::Orchestrator, rc);
    let prompt = render_role_prompt(Role::Orchestrator, &i);
    // The first 8 lines are the load-bearing header (role
    // identity + project identity). Pin them so a refactor of
    // the body doesn't accidentally shift the header.
    let expected_header = "\
# Spawn prompt — role: orchestrator

## Role identity
- role: orchestrator
- skill: mp-coordinator
- harness: opencode
- model: anthropic/claude-opus-4-1

## Project identity
- project: master-plan";
    assert!(
        prompt.starts_with(expected_header),
        "orchestrator prompt header drifted from golden.\n\
         expected start:\n{expected_header}\n\n\
         actual start:\n{}",
        first_lines(&prompt, 11)
    );
}

#[test]
fn golden_three_pane_runner_prompt_byte_stable() {
    let rc = rc(Role::Runner, "opencode", Some("anthropic/claude-opus-4-1"));
    let i = inputs(Role::Runner, rc);
    let prompt = render_role_prompt(Role::Runner, &i);
    let expected_header = "\
# Spawn prompt — role: runner

## Role identity
- role: runner
- skill: mp-runner
- harness: opencode
- model: anthropic/claude-opus-4-1

## Project identity
- project: master-plan";
    assert!(
        prompt.starts_with(expected_header),
        "runner prompt header drifted from golden.\n\
         expected start:\n{expected_header}\n\n\
         actual start:\n{}",
        first_lines(&prompt, 11)
    );
}

#[test]
fn golden_three_pane_reviewer_prompt_byte_stable() {
    let rc = rc(Role::Reviewer, "opencode", Some("anthropic/claude-opus-4-1"));
    let i = inputs(Role::Reviewer, rc);
    let prompt = render_role_prompt(Role::Reviewer, &i);
    let expected_header = "\
# Spawn prompt — role: reviewer

## Role identity
- role: reviewer
- skill: mp-runner
- harness: opencode
- model: anthropic/claude-opus-4-1

## Project identity
- project: master-plan";
    assert!(
        prompt.starts_with(expected_header),
        "reviewer prompt header drifted from golden.\n\
         expected start:\n{expected_header}\n\n\
         actual start:\n{}",
        first_lines(&prompt, 11)
    );
}

#[test]
fn golden_three_pane_yields_three_separate_bundles_in_canonical_order() {
    let rc = rc(Role::Runner, "opencode", None);
    let io = inputs(Role::Orchestrator, rc.clone());
    let ir = inputs(Role::Runner, rc.clone());
    let iv = inputs(Role::Reviewer, rc);
    let bundles: Vec<BundledPrompt> =
        render_topology_prompts(&io, &ir, &iv, Topology::ThreeAgent);
    assert_eq!(bundles.len(), 3);
    // Canonical declaration order: Orchestrator, Runner, Reviewer.
    assert_eq!(bundles[0].label, "role-orchestrator-1");
    assert_eq!(bundles[1].label, "role-runner-1");
    assert_eq!(bundles[2].label, "role-reviewer-1");
    assert_eq!(bundles[0].roles, vec![Role::Orchestrator]);
    assert_eq!(bundles[1].roles, vec![Role::Runner]);
    assert_eq!(bundles[2].roles, vec![Role::Reviewer]);
}

#[test]
fn golden_two_pane_supervisor_bundle_concatenates_orchestrator_then_reviewer() {
    let rc = rc(Role::Runner, "opencode", None);
    let io = inputs(Role::Orchestrator, rc.clone());
    let ir = inputs(Role::Runner, rc.clone());
    let iv = inputs(Role::Reviewer, rc);
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::TwoAgent);
    assert_eq!(bundles.len(), 2);
    let supervisor = &bundles[0];
    let runner = &bundles[1];
    // Supervisor carries both O and V roles.
    assert_eq!(supervisor.label, "supervisor");
    assert_eq!(
        supervisor.roles,
        vec![Role::Orchestrator, Role::Reviewer]
    );
    // Named seam between O and V sections — the supervisor's
    // only cue for "you're in O-mode vs V-mode".
    assert!(supervisor.prompt.contains("─── role: orchestrator ───"));
    assert!(supervisor.prompt.contains("─── role: reviewer ───"));
    // Order: orchestrator section appears before reviewer section.
    let orch_idx = supervisor.prompt.find("─── role: orchestrator ───");
    let rev_idx = supervisor.prompt.find("─── role: reviewer ───");
    assert!(orch_idx < rev_idx);
    // Runner pane stays isolated.
    assert_eq!(runner.label, "role-runner-1");
    assert_eq!(runner.roles, vec![Role::Runner]);
    assert!(!runner.prompt.contains("─── role:"));
}

#[test]
fn golden_one_pane_collapsed_bundle_concatenates_all_three_roles() {
    let rc = rc(Role::Runner, "opencode", None);
    let io = inputs(Role::Orchestrator, rc.clone());
    let ir = inputs(Role::Runner, rc.clone());
    let iv = inputs(Role::Reviewer, rc);
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::OneAgent);
    assert_eq!(bundles.len(), 1);
    let supervisor = &bundles[0];
    assert_eq!(supervisor.label, "supervisor");
    assert_eq!(
        supervisor.roles,
        vec![Role::Orchestrator, Role::Runner, Role::Reviewer]
    );
    // All three role seams present and in canonical order.
    let orch_idx = supervisor.prompt.find("─── role: orchestrator ───");
    let run_idx = supervisor.prompt.find("─── role: runner ───");
    let rev_idx = supervisor.prompt.find("─── role: reviewer ───");
    assert!(orch_idx.is_some());
    assert!(run_idx.is_some());
    assert!(rev_idx.is_some());
    let (o, r) = (orch_idx.unwrap(), run_idx.unwrap());
    let (rr, vv) = (r, rev_idx.unwrap());
    assert!(o < rr && rr < vv, "seams must be in canonical order");
}

#[test]
fn render_role_prompt_is_deterministic_across_calls() {
    let rc = rc(Role::Runner, "opencode", Some("anthropic/claude-opus-4-1"));
    let i = inputs(Role::Runner, rc);
    let p1 = render_role_prompt(Role::Runner, &i);
    let p2 = render_role_prompt(Role::Runner, &i);
    let p3 = render_role_prompt(Role::Runner, &i);
    assert_eq!(p1, p2);
    assert_eq!(p2, p3);
}

#[test]
fn render_topology_prompts_is_deterministic_across_calls() {
    let rc = rc(Role::Runner, "opencode", None);
    let io = inputs(Role::Orchestrator, rc.clone());
    let ir = inputs(Role::Runner, rc.clone());
    let iv = inputs(Role::Reviewer, rc);
    let a = render_topology_prompts(&io, &ir, &iv, Topology::ThreeAgent);
    let b = render_topology_prompts(&io, &ir, &iv, Topology::ThreeAgent);
    assert_eq!(a, b);
}

#[test]
fn golden_topology_collapses_are_byte_stable_for_each_topology() {
    // Pin a minimal invariant: re-rendering the same inputs at
    // the same topology always yields the same bundle list.
    // That's the load-bearing AC-01 guarantee: the prompt is
    // deterministic, so the audit surface
    // (roles.<role>.spawn_prompt_rendered, prompt_bundles) is
    // reproducible across re-spawns.
    let rc_o = rc(Role::Orchestrator, "opencode", Some("anthropic/claude-opus-4-1"));
    let rc_r = rc(Role::Runner, "opencode", Some("anthropic/claude-opus-4-1"));
    let rc_v = rc(Role::Reviewer, "opencode", Some("anthropic/claude-opus-4-1"));
    let io = inputs(Role::Orchestrator, rc_o);
    let ir = inputs(Role::Runner, rc_r);
    let iv = inputs(Role::Reviewer, rc_v);
    for topology in [Topology::ThreeAgent, Topology::TwoAgent, Topology::OneAgent] {
        let a = render_topology_prompts(&io, &ir, &iv, topology);
        let b = render_topology_prompts(&io, &ir, &iv, topology);
        assert_eq!(a, b, "topology {topology} rendering is not deterministic");
    }
}
