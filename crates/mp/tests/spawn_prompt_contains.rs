//! M210 / AC-02: every rendered prompt contains the required
//! identity + boundary + contract blocks.
//!
//! Per the spec, each prompt must carry:
//! - project / session_id / milestone_id identity
//! - role + skill identity
//! - explicit `Boundaries you must respect` block for that role
//! - the M211 task-assignment contract (runner / reviewer)
//! - the exact `mp autopilot session transition` commands the
//!   role must use
//! - the lane-notify wire format pointer
//!
//! And the prompts must NEVER direct agents to edit session.json
//! directly (verifier boundary violation).
//!
//! This file pins all of those requirements so a future
//! renderer refactor cannot silently drop a block or insert a
//! dangerous direct-edit instruction.

use mp::autopilot::prompts::spawn::{
    render_role_prompt, render_topology_prompts, RoleReexport as Role,
    SpawnPromptInputs, TopologyReexport as Topology,
};
use mp::autopilot::role::{resolve_role_config, ResolvedRoleConfig};

fn rc(role: Role) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    resolve_role_config(None, None, &builtin)
}

fn inputs(role: Role) -> SpawnPromptInputs {
    SpawnPromptInputs::new(
        "master-plan",
        "sess-alpha",
        "M210",
        0,
        rc(role),
    )
    .unwrap()
}

#[test]
fn every_role_prompt_includes_project_session_milestone_identity() {
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        assert!(
            prompt.contains("project: master-plan"),
            "role {role} prompt missing project identity"
        );
        assert!(
            prompt.contains("session_id: sess-alpha"),
            "role {role} prompt missing session_id"
        );
        assert!(
            prompt.contains("milestone_id: M210"),
            "role {role} prompt missing milestone_id"
        );
        assert!(
            prompt.contains("queue_position:"),
            "role {role} prompt missing queue_position"
        );
    }
}

#[test]
fn every_role_prompt_includes_role_and_skill_identity() {
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        assert!(
            prompt.contains(&format!("role: {}", role.as_str())),
            "role {role} prompt missing role label"
        );
        let expected_skill = match role {
            Role::Orchestrator => "mp-coordinator",
            _ => "mp-runner",
        };
        assert!(
            prompt.contains(&format!("skill: {expected_skill}")),
            "role {role} prompt missing skill {expected_skill}"
        );
    }
}

#[test]
fn every_role_prompt_includes_explicit_boundaries_block() {
    // Per DD-01 the boundaries text lives in code so drift is
    // detectable at commit time. Each role's block is distinct.
    let orch = render_role_prompt(Role::Orchestrator, &inputs(Role::Orchestrator));
    let runner = render_role_prompt(Role::Runner, &inputs(Role::Runner));
    let reviewer = render_role_prompt(Role::Reviewer, &inputs(Role::Reviewer));

    assert!(orch.contains("Boundaries you MUST respect"));
    assert!(orch.contains("You are the ORCHESTRATOR"));
    assert!(orch.contains("you NEVER claim or run review"));
    assert!(orch.contains("call `mp reviews pass`"));
    assert!(orch.contains("edit any file under `master-plan/`"));

    assert!(runner.contains("Boundaries you MUST respect"));
    assert!(runner.contains("You are the RUNNER"));
    assert!(runner.contains("Per-AC evidence MUST be a real"));
    assert!(runner.contains("mp milestone complete <id>"));

    assert!(reviewer.contains("Boundaries you MUST respect"));
    assert!(reviewer.contains("You are the REVIEWER"));
    assert!(reviewer.contains("`mp reviews pass`"));
    assert!(reviewer.contains("NEVER implement steps"));
}

#[test]
fn runner_and_reviewer_prompts_include_task_assignment_contract() {
    // The orchestrator dispatches work via M211 typed payloads.
    // The runner and reviewer prompts must reference the
    // typed-receive contract so they don't try to invent their
    // own.
    let runner = render_role_prompt(Role::Runner, &inputs(Role::Runner));
    assert!(runner.contains("Task-assignment contract"));
    assert!(runner.contains("session transition --role runner"));

    let reviewer = render_role_prompt(Role::Reviewer, &inputs(Role::Reviewer));
    assert!(reviewer.contains("Task-assignment contract"));
    assert!(reviewer.contains("session transition --role reviewer"));
}

#[test]
fn orchestrator_prompt_includes_dispatch_command() {
    let orch = render_role_prompt(Role::Orchestrator, &inputs(Role::Orchestrator));
    assert!(orch.contains("Task-assignment contract").not());
    // The orchestrator dispatches via herdr agent prompt (M211).
    assert!(orch.contains("dispatch work to a role pane"));
}

#[test]
fn every_role_prompt_lists_typed_session_transition_commands() {
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        assert!(
            prompt.contains("Typed session-transition surface"),
            "role {role} prompt missing the typed-transition section"
        );
        assert!(
            prompt.contains("mp autopilot session transition"),
            "role {role} prompt missing the typed session transition command"
        );
        // Per-role commands.
        match role {
            Role::Orchestrator => {
                assert!(prompt.contains("--role orchestrator"));
                assert!(prompt.contains("--role runner"));
                assert!(prompt.contains("--role reviewer"));
                assert!(prompt.contains("herdr agent prompt"));
            }
            Role::Runner => {
                assert!(prompt.contains("--role runner"));
                assert!(prompt.contains("mp milestone step done"));
                assert!(prompt.contains("mp milestone criterion pass"));
                assert!(prompt.contains("mp milestone complete"));
            }
            Role::Reviewer => {
                assert!(prompt.contains("--role reviewer"));
                assert!(prompt.contains("mp reviews pass"));
                assert!(prompt.contains("mp reviews finding add"));
                assert!(prompt.contains("mp reviews finding resolve"));
            }
        }
    }
}

#[test]
fn every_role_prompt_includes_lane_notify_pointer() {
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        assert!(
            prompt.contains("Lane notify"),
            "role {role} prompt missing lane-notify section"
        );
        assert!(
            prompt.contains("lane-notify wire format"),
            "role {role} prompt missing wire-format pointer"
        );
    }
}

#[test]
fn prompts_never_direct_agents_to_edit_session_json_directly() {
    // AC-02 + AC-05: the verifier rejects direct session.json
    // edits. The prompt must not instruct them.
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        let lc = prompt.to_ascii_lowercase();
        assert!(
            !lc.contains("edit session.json"),
            "role {role} prompt mentions direct session.json edits"
        );
        assert!(
            !lc.contains("write to session.json"),
            "role {role} prompt mentions writing session.json"
        );
        assert!(
            !lc.contains("vim session.json"),
            "role {role} prompt mentions vim-ing session.json"
        );
        assert!(
            !lc.contains("modify session.json directly"),
            "role {role} prompt mentions modifying session.json directly"
        );
    }
}

#[test]
fn topology_collapsed_bundle_carries_all_role_boundaries() {
    // For 1-pane and 2-pane supervisor modes, the bundled
    // prompt must still contain every role's Boundaries block —
    // the supervisor agent inherits every role's contract.
    let io = inputs(Role::Orchestrator);
    let ir = inputs(Role::Runner);
    let iv = inputs(Role::Reviewer);
    for topology in [Topology::TwoAgent, Topology::OneAgent] {
        let bundles = render_topology_prompts(&io, &ir, &iv, topology);
        let supervisor_prompt = &bundles[0].prompt;
        assert!(
            supervisor_prompt.contains("You are the ORCHESTRATOR"),
            "{topology} supervisor missing orchestrator boundaries"
        );
        assert!(
            supervisor_prompt.contains("You are the REVIEWER"),
            "{topology} supervisor missing reviewer boundaries"
        );
        if topology == Topology::OneAgent {
            assert!(
                supervisor_prompt.contains("You are the RUNNER"),
                "1-pane supervisor missing runner boundaries"
            );
        }
    }
}

// Tiny helper trait so we can write `x.not()`.
trait Not {
    fn not(&self) -> bool;
}
impl Not for bool {
    fn not(&self) -> bool {
        !*self
    }
}
