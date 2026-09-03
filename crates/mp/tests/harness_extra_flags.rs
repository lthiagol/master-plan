//! M210 / AC-03: `harness_extra_flags(RoleConfig)` returns the
//! documented argv tail per harness (opencode / cursor / pi),
//! including the exact skill/agent and model translation.
//! Unsupported harnesses fail before any pane creation (typed
//! `HarnessFlagError::Unsupported`).
//!
//! Per-harness shape pinned by golden fixtures:
//! - opencode: `--skill <name> [--model <id>]`
//! - cursor:   `--agent <name> [--model <id>]`
//! - pi:       `--skill <name> [--model <id>]`

use mp::autopilot::prompts::spawn::{
    harness_extra_flags, HarnessFlagError, RoleReexport as Role, SUPPORTED_AUTOPILOT_HARNESSES,
};
use mp::autopilot::role::{resolve_role_config, ResolvedRoleConfig};

fn rc(role: Role) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    resolve_role_config(None, None, &builtin)
}

#[test]
fn golden_opencode_appends_skill_and_model() {
    let mut r = rc(Role::Runner);
    r.harness = "opencode".into();
    r.skill = "mp-runner".into();
    r.model = Some("anthropic/claude-opus-4-1".into());
    let flags = harness_extra_flags(&r).unwrap();
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
fn golden_cursor_uses_agent_instead_of_skill() {
    let mut r = rc(Role::Reviewer);
    r.harness = "cursor".into();
    r.skill = "mp-runner".into();
    r.model = Some("anthropic/claude-opus-4-1".into());
    let flags = harness_extra_flags(&r).unwrap();
    // Cursor uses --agent, not --skill.
    assert!(flags.contains(&"--agent".to_string()));
    assert!(flags.contains(&"mp-runner".to_string()));
    assert!(!flags.contains(&"--skill".to_string()));
    // Model is appended after.
    assert!(flags.contains(&"--model".to_string()));
    assert!(flags.contains(&"anthropic/claude-opus-4-1".to_string()));
    // Wire shape: --agent <name> --model <id>
    assert_eq!(
        flags,
        vec![
            "--agent".to_string(),
            "mp-runner".to_string(),
            "--model".to_string(),
            "anthropic/claude-opus-4-1".to_string(),
        ]
    );
}

#[test]
fn golden_pi_uses_skill_flag() {
    let mut r = rc(Role::Runner);
    r.harness = "pi".into();
    r.skill = "mp-runner".into();
    r.model = None;
    let flags = harness_extra_flags(&r).unwrap();
    // Pi v1 surfaces skill via the same --skill flag as opencode
    // (per harness/registry V1_ENTRIES model_flag pattern).
    assert_eq!(
        flags,
        vec!["--skill".to_string(), "mp-runner".to_string()]
    );
}

#[test]
fn pi_with_model_includes_model_after_skill() {
    let mut r = rc(Role::Runner);
    r.harness = "pi".into();
    r.skill = "mp-runner".into();
    r.model = Some("anthropic/claude-sonnet-4-5".into());
    let flags = harness_extra_flags(&r).unwrap();
    assert_eq!(
        flags,
        vec![
            "--skill".to_string(),
            "mp-runner".to_string(),
            "--model".to_string(),
            "anthropic/claude-sonnet-4-5".to_string(),
        ]
    );
}

#[test]
fn model_flag_omitted_when_unset() {
    let mut r = rc(Role::Orchestrator);
    r.harness = "opencode".into();
    r.model = None;
    let flags = harness_extra_flags(&r).unwrap();
    assert!(!flags.contains(&"--model".to_string()));
}

#[test]
fn unsupported_harness_rejected_before_pane_creation() {
    // Per AC-03: an unsupported harness must surface a typed
    // error so the spawn pipeline can reject before any pane
    // is created.
    for harness in [
        "claude-code",
        "gemini",
        "codex",
        "windsurf",
        "cline",
        "unknown-harness",
    ] {
        let mut r = rc(Role::Runner);
        r.harness = harness.into();
        let err = harness_extra_flags(&r).unwrap_err();
        match err {
            HarnessFlagError::Unsupported {
                harness: got,
                supported,
            } => {
                assert_eq!(got, harness);
                assert!(supported.contains(&"opencode".to_string()));
                assert!(supported.contains(&"cursor".to_string()));
                assert!(supported.contains(&"pi".to_string()));
            }
        }
    }
}

#[test]
fn supported_harness_list_pinned_at_three_v1_kinds() {
    // Per AC-03: the v1 autopilot spawn set is the closed
    // opencode / cursor / pi triple. Adding a new kind is an
    // explicit edit (and forces a verifier + golden re-pin).
    assert_eq!(SUPPORTED_AUTOPILOT_HARNESSES.len(), 3);
    assert!(SUPPORTED_AUTOPILOT_HARNESSES.contains(&"opencode"));
    assert!(SUPPORTED_AUTOPILOT_HARNESSES.contains(&"cursor"));
    assert!(SUPPORTED_AUTOPILOT_HARNESSES.contains(&"pi"));
}

#[test]
fn harness_flag_error_display_mentions_supported_list() {
    let mut r = rc(Role::Runner);
    r.harness = "claude-code".into();
    let err = harness_extra_flags(&r).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("claude-code"));
    assert!(msg.contains("opencode"));
    assert!(msg.contains("cursor"));
    assert!(msg.contains("pi"));
}
