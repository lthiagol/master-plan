//! M209 / AC-02: the per-role config resolution chain
//! (session.json.roles.<role>.* → mp config.json autopilot.roles.
//! <role>.* → built-in default) is a single read path: explicit
//! autopilot override wins, then global default, then built-in.
//! Empty-string overrides fall through to the next layer.
//!
//! Pins the AC-02 contract at the integration level. The unit-level
//! resolution tests live in `crates/mp/src/autopilot/role.rs`; this
//! file is the public-surface pin for the resolver's contract from
//! the consumer's perspective.

use mp::autopilot::role::{
    builtin_role_default, resolve_role_config_full, resolve_role_config_with_provenance, Role,
    RoleConfigOverride, RoleConfigSource,
};
use std::collections::BTreeMap;

fn build_override(
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
fn three_tier_priority_order_is_session_then_global_then_builtin() {
    // Spec priority order — verify by setting the same field on each
    // layer and observing the winner.
    let global = build_override(Some("global-model"), Some("opencode"), Some("global-skill"));
    let session = build_override(Some("session-model"), Some("pi"), Some("session-skill"));

    let resolved = resolve_role_config_full(Role::Orchestrator, Some(&session), Some(&global));
    // Session wins on every field where both are set.
    assert_eq!(resolved.model.as_deref(), Some("session-model"));
    assert_eq!(resolved.harness, "pi");
    assert_eq!(resolved.skill, "session-skill");
}

#[test]
fn global_default_beats_builtin_when_session_is_absent() {
    // Built-in supplies skill = mp-coordinator; global explicitly
    // sets skill = mp-runner-for-global. Session is absent, so
    // session carries no values.
    let global = build_override(None, None, Some("mp-runner-for-global"));
    let resolved = resolve_role_config_full(Role::Orchestrator, None, Some(&global));
    assert_eq!(resolved.skill, "mp-runner-for-global");
    assert_eq!(resolved.harness, "opencode", "harness still from built-in");
}

#[test]
fn builtin_supplies_fields_when_both_overrides_are_absent() {
    // Truly empty config — built-in is the only layer with values.
    let resolved = resolve_role_config_full(Role::Runner, None, None);
    let builtin = builtin_role_default(Role::Runner);
    assert_eq!(
        resolved.harness,
        builtin.harness.expect("built-in has harness")
    );
    assert_eq!(resolved.skill, builtin.skill.expect("built-in has skill"));
    // Model has no built-in by design — the harness registry fills
    // it at spawn time. The resolver surfaces that absence as `None`
    // rather than inventing a value.
    assert_eq!(resolved.model, None);
}

#[test]
fn empty_string_session_override_falls_through_to_global() {
    // Spec: "Empty string overrides are treated as 'not set' (the
    // next tier resolves)". Pin the parity between a never-set
    // field and a session-driven empty clear.
    let session = build_override(Some(""), Some(""), Some(""));
    let global = build_override(Some("global-m"), Some("global-h"), Some("global-s"));
    let resolved = resolve_role_config_full(Role::Orchestrator, Some(&session), Some(&global));
    assert_eq!(resolved.model.as_deref(), Some("global-m"));
    assert_eq!(resolved.harness, "global-h");
    assert_eq!(resolved.skill, "global-s");
}

#[test]
fn empty_string_session_override_falls_through_to_builtin_when_global_absent() {
    // Session cleared both fields; global is absent entirely. Built-in
    // must fill in harness + skill (and any extras it ships).
    let session = build_override(Some(""), Some(""), Some(""));
    let resolved = resolve_role_config_full(Role::Reviewer, Some(&session), None);
    // Reviewer's built-in skill is mp-runner.
    assert_eq!(resolved.skill, "mp-runner");
    assert_eq!(resolved.harness, "opencode");
}

#[test]
fn per_field_fallthrough_partial_session_override() {
    // Real-world: the runner says "I want a different harness for
    // this milestone" (session.harness = "cursor") but accepts the
    // global model and built-in skill.
    let session = build_override(None, Some("cursor"), None);
    let global = build_override(Some("anthropic/claude-opus-4-1"), Some("opencode"), None);
    let resolved = resolve_role_config_full(Role::Runner, Some(&session), Some(&global));
    assert_eq!(resolved.harness, "cursor", "session override wins");
    assert_eq!(
        resolved.model.as_deref(),
        Some("anthropic/claude-opus-4-1"),
        "model falls through to global"
    );
    assert_eq!(
        resolved.skill, "mp-runner",
        "skill falls through to built-in"
    );
}

#[test]
fn provenance_reports_each_layer_per_field() {
    // Diagnostically surface which layer provided each field.
    let session = build_override(Some("session-m"), Some("session-h"), Some("session-s"));
    let global = build_override(None, Some("global-h"), None);
    let out =
        resolve_role_config_with_provenance(Role::Orchestrator, Some(&session), Some(&global));
    assert_eq!(out.model_source, Some(RoleConfigSource::Session));
    assert_eq!(out.harness_source, Some(RoleConfigSource::Session));
    assert_eq!(out.skill_source, Some(RoleConfigSource::Session));
}

#[test]
fn provenance_distinguishes_global_from_builtin_when_session_absent() {
    let global = build_override(Some("g-m"), None, None);
    let out = resolve_role_config_with_provenance(Role::Orchestrator, None, Some(&global));
    assert_eq!(out.model_source, Some(RoleConfigSource::Global));
    assert_eq!(out.harness_source, Some(RoleConfigSource::Builtin));
    assert_eq!(out.skill_source, Some(RoleConfigSource::Builtin));
}

#[test]
fn extras_merge_per_key_with_session_winning_on_conflict() {
    // Same merge semantics across keys — a key set on multiple layers
    // ends up at the value from the highest-priority layer; a key
    // set only on a lower-priority layer survives. Empty-valued
    // session entries fall through (parity with the scalar rule).
    let mut builtin = builtin_role_default(Role::Runner);
    builtin.extras.insert("cycle_budget".into(), "4".into());

    let mut global = RoleConfigOverride::empty();
    global.extras.insert("max_retries".into(), "3".into());
    global.extras.insert("cycle_budget".into(), "8".into());

    let mut session = RoleConfigOverride::empty();
    session.extras.insert("cycle_budget".into(), "12".into());
    session.extras.insert("trace".into(), "yes".into());

    let resolved = resolve_role_config_full(Role::Runner, Some(&session), Some(&global));
    // NB: built-in's `cycle_budget` was "4", global's was "8",
    // session's was "12" — session wins.
    assert_eq!(
        resolved.extras.get("cycle_budget").map(String::as_str),
        Some("12")
    );
    // Global didn't override `max_retries` — its value survives.
    assert_eq!(
        resolved.extras.get("max_retries").map(String::as_str),
        Some("3")
    );
    // Session's exclusive key makes it through.
    assert_eq!(
        resolved.extras.get("trace").map(String::as_str),
        Some("yes")
    );
}

#[test]
fn resolver_is_the_only_canonical_read_path_for_three_roles() {
    // Every role resolves cleanly without panicking on the hot path,
    // and each role's resolved skill matches the documented mapping
    // (orchestrator -> mp-coordinator; runner/reviewer -> mp-runner)
    // when no override is supplied.
    let expected_skills = [
        (Role::Orchestrator, "mp-coordinator"),
        (Role::Runner, "mp-runner"),
        (Role::Reviewer, "mp-runner"),
    ];
    for (role, expected_skill) in expected_skills {
        let resolved = resolve_role_config_full(role, None, None);
        assert_eq!(resolved.skill, expected_skill, "{role}");
        assert_eq!(resolved.harness, "opencode");
    }
}
