//! M210 / AC-05: each role prompt contains valid typed
//! session-transition commands and allowed states from M207;
//! golden tests reject unknown states, direct session-file
//! edits, and missing actor / session identity.
//!
//! This test file cross-checks three surfaces:
//!  1. The role prompt's typed-transition commands map onto
//!     the `transitions::RoleState` enum (no drift between
//!     prompt and verifier state machine).
//!  2. The prompt's "Allowed role states" list matches the
//!     closed `RoleState` enum; unknown states are absent.
//!  3. The prompt never instructs direct session.json edits
//!     (verifier boundary violation).
//!  4. The session transition command requires an actor and
//!     session identity — the prompt names the lane-notify
//!     format that carries both.

use mp::autopilot::prompts::spawn::{
    render_role_prompt, RoleReexport as Role, SpawnPromptInputs, ALLOWED_ROLE_STATES,
};
use mp::autopilot::role::{resolve_role_config, ResolvedRoleConfig};
use mp::autopilot::spawn::{is_allowed_transition, ALLOWED_STATE_TRANSITIONS};
use mp::autopilot::transitions::{is_valid as m207_is_valid, RoleState};

fn rc(role: Role) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    resolve_role_config(None, None, &builtin)
}

fn inputs(role: Role) -> SpawnPromptInputs {
    SpawnPromptInputs::new("master-plan", "sess-alpha", "M210", 0, rc(role)).unwrap()
}

#[test]
fn allowed_role_states_mirror_role_state_enum() {
    // The prompt's allowed-states list must match the M207
    // enum exactly. A future enum addition forces both surfaces
    // to update in lockstep.
    let states: Vec<String> = [
        RoleState::Idle,
        RoleState::Starting,
        RoleState::Working,
        RoleState::Blocked,
        RoleState::Done,
        RoleState::Unknown,
    ]
    .iter()
    .map(|s| s.as_str().to_string())
    .collect();
    let allowed: Vec<String> = ALLOWED_ROLE_STATES.iter().map(|s| s.to_string()).collect();
    assert_eq!(states, allowed);
}

#[test]
fn allowed_state_transitions_match_m207_state_machine() {
    // AC-05 invariant: every transition the prompt mentions
    // must pass M207's state machine check. A drift here means
    // the prompt mentions a transition the verifier rejects.
    for (from, to) in ALLOWED_STATE_TRANSITIONS {
        let from_s: RoleState = from.parse().expect("known state");
        let to_s: RoleState = to.parse().expect("known state");
        assert!(
            m207_is_valid(from_s, to_s),
            "transition {from} -> {to} is in ALLOWED_STATE_TRANSITIONS but rejected by M207"
        );
    }
}

#[test]
fn is_allowed_transition_matches_const_table() {
    for (from, to) in ALLOWED_STATE_TRANSITIONS {
        assert!(is_allowed_transition(from, to));
    }
    // And a known-invalid transition is rejected.
    assert!(!is_allowed_transition("idle", "done"));
    assert!(!is_allowed_transition("done", "starting"));
}

#[test]
fn every_role_prompt_lists_allowed_states_in_backticks() {
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        for state in ALLOWED_ROLE_STATES {
            assert!(
                prompt.contains(&format!("`{state}`")),
                "role {role} prompt missing allowed state `{state}`"
            );
        }
    }
}

#[test]
fn every_role_prompt_rejects_unknown_states() {
    // If any of these appear in a prompt, that's drift — the
    // M207 enum doesn't define them and the verifier would
    // reject the role's transition attempt.
    let unknown = [
        "complete", "approved", "started", "queued", "failed", "skipped",
    ];
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        for state in unknown {
            assert!(
                !prompt.contains(&format!("`{state}`")),
                "role {role} prompt mentions unknown state `{state}`"
            );
        }
    }
}

#[test]
fn prompts_instruct_typed_transition_commands_only() {
    // The transition command line in each prompt must use the
    // typed mp autopilot session transition form. No raw
    // session.json mutations, no sed/awk to session.json.
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        assert!(
            prompt.contains("mp autopilot session transition --role"),
            "role {role} prompt missing typed session transition form"
        );
        // Anti-patterns the prompt must NOT contain.
        let lc = prompt.to_ascii_lowercase();
        assert!(
            !lc.contains("echo '{"),
            "role {role} prompt mentions raw echo writes"
        );
        assert!(
            !lc.contains("jq '"),
            "role {role} prompt mentions raw jq edits"
        );
        assert!(
            !lc.contains("sed -i '"),
            "role {role} prompt mentions raw sed edits"
        );
    }
}

#[test]
fn lane_notify_format_carries_actor_and_session_identity() {
    // The lane-notify wire format is what the verifier reads
    // back from the role pane's reply. It must carry:
    // - session id
    // - milestone id
    // - cycle
    // - role
    // - next decision
    for role in Role::ALL {
        let prompt = render_role_prompt(role, &inputs(role));
        // The wire-format string lives inside the Boundaries
        // block; check that every required field is named.
        assert!(
            prompt.contains("session="),
            "role {role} lane-notify missing session="
        );
        assert!(
            prompt.contains("milestone="),
            "role {role} lane-notify missing milestone="
        );
        assert!(
            prompt.contains("cycle="),
            "role {role} lane-notify missing cycle="
        );
        assert!(
            prompt.contains(&format!("role={}", role.as_str())),
            "role {role} lane-notify missing role={}",
            role.as_str()
        );
        assert!(
            prompt.contains("next="),
            "role {role} lane-notify missing next="
        );
    }
}

#[test]
fn role_specific_commands_are_role_scoped_not_horizontal() {
    // Each role's command list must NOT include another
    // role's commands as commands — the verifier cross-checks
    // that a runner pane never claims reviews, and vice versa.
    // Boundary-reminder mentions ("you do NOT call ...") are
    // fine and expected.
    let runner = render_role_prompt(Role::Runner, &inputs(Role::Runner));
    let reviewer = render_role_prompt(Role::Reviewer, &inputs(Role::Reviewer));
    let orch = render_role_prompt(Role::Orchestrator, &inputs(Role::Orchestrator));

    // Runner prompt: command form `mp reviews pass <id>` is
    // forbidden. The boundary-reminder text "you do NOT call
    // `mp reviews pass`" must still be present.
    assert!(
        !runner.contains("mp reviews pass <id>"),
        "runner prompt must not include `mp reviews pass <id>` as a command form"
    );
    assert!(
        runner.contains("do NOT call `mp reviews pass`")
            || runner.contains("NEVER claim `mp reviews pass`"),
        "runner prompt must mention `mp reviews pass` as a boundary reminder"
    );

    // Reviewer prompt: command form `mp milestone complete
    // <id>` is forbidden. The boundary-reminder text must
    // still be present.
    assert!(
        !reviewer.contains("mp milestone complete <id>"),
        "reviewer prompt must not include `mp milestone complete <id>` as a command form"
    );
    assert!(
        reviewer.contains("NEVER call `mp milestone complete`"),
        "reviewer prompt must mention `mp milestone complete` as a boundary reminder"
    );

    // Orchestrator prompt: command form `mp milestone step
    // done <id>` is forbidden (orchestrator dispatches via
    // herdr, not via milestone step).
    assert!(
        !orch.contains("mp milestone step done <id>"),
        "orchestrator prompt must not include `mp milestone step done <id>` as a command form"
    );
}
