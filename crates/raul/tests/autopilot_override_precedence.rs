//! M215 AC-03: override precedence (global > drive > default).
//!
//! On Start, the override panel writes its values into session.json's
//! `config_overrides` + `roles.<role>` blocks. The next session
//! without overrides falls back to the project defaults. This test
//! pins the precedence at the typed payload level — the production
//! code path that wires the panel into session.json consumes
//! `OverridePanel::to_session_overrides()` directly, so the
//! precedence contract is enforced there.
//!
//! Precedence:
//! 1. Drive overrides (panel payload) — win when set
//! 2. Project defaults (`mp config.json`) — used when a drive
//!    field is the inherit sentinel (empty / None)
//! 3. Hard-coded defaults (3-agent topology, 2s refresh) — used
//!    when neither is set
//!
//! The test exercises each branch explicitly so a future change
//! to `to_session_overrides()` cannot silently flip the precedence
//! rules.

#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;

use raul::tui::autopilot::{
    OverridePanel, RoleOverride, SessionOverridesPayload, DEFAULT_REFRESH_SECS, DEFAULT_TOPOLOGY,
};

/// AC-03: the precedence baseline. With every role envelope empty,
/// the persisted payload has empty `roles` — the runtime reads
/// `mp config.json` for every role, falling through to the
/// hard-coded defaults if the config is silent. The
/// `config_overrides` block still carries the panel's topology /
/// refresh so the override panel always has a typed presence on
/// the session.
#[test]
fn empty_panel_payload_uses_project_defaults_for_every_role() {
    let panel = OverridePanel::default();
    let payload = panel.to_session_overrides();
    assert!(
        payload.roles.is_empty(),
        "empty panel must not bloat session.json with empty role envelopes; \
         the runtime reads project defaults instead"
    );
    assert_eq!(payload.config_overrides.topology, DEFAULT_TOPOLOGY);
    assert_eq!(
        payload.config_overrides.poll_interval_ms,
        Some(DEFAULT_REFRESH_SECS * 1000)
    );
}

/// AC-03: drive overrides win over project defaults when set. A
/// panel that sets `runner.harness = "opencode"` produces a
/// payload where `roles.runner.harness = "opencode"` — even if
/// the project's `autopilot.roles.runner.harness` in config.json
/// is `cursor`, the session honors the drive value.
#[test]
fn drive_overrides_win_over_project_defaults() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        RoleOverride {
            model: Some("anthropic/claude-opus-4-1".to_string()),
            harness: Some("opencode".to_string()),
            ..RoleOverride::empty()
        },
    );
    let payload = panel.to_session_overrides();

    assert!(
        payload.roles.contains_key("runner"),
        "runner must appear in the persisted roles when overridden"
    );
    let runner = &payload.roles["runner"];
    assert_eq!(runner["harness"], "opencode");
    assert_eq!(runner["model"], "anthropic/claude-opus-4-1");

    // Orchestrator + reviewer stay absent — they inherit from the
    // project default (or the hard-coded fallback). The runtime
    // resolution reads `mp config.json` for them.
    assert!(!payload.roles.contains_key("orchestrator"));
    assert!(!payload.roles.contains_key("reviewer"));
}

/// AC-03: a drive role with a *partial* override (only model set)
/// persists only the set fields. Empty harness / skill / extras
/// drop out so the runtime falls back to the project default for
/// those fields. The model's drive value wins.
#[test]
fn partial_drive_override_persists_only_set_fields() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        RoleOverride {
            model: Some("anthropic/claude-opus-4-1".to_string()),
            // harness / skill / extras all stay None / empty.
            ..RoleOverride::empty()
        },
    );
    let payload = panel.to_session_overrides();
    let runner = &payload.roles["runner"];

    // Model wins — drive value.
    assert_eq!(runner["model"], "anthropic/claude-opus-4-1");
    // Harness / skill / extras drop out — runtime falls back to
    // project default.
    assert!(runner.get("harness").is_none());
    assert!(runner.get("skill").is_none());
    assert!(runner.get("extras").is_none());
}

/// AC-03: empty-string fields are treated as the inherit sentinel,
/// identical to `None`. The validator passes; the serializer drops
/// the key from the persisted payload so the runtime reads the
/// project default. A user who types-and-clears a field gets the
/// same outcome as a user who never touched the field.
#[test]
fn empty_string_is_inherit_sentinel_not_persisted_empty_string() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        RoleOverride {
            model: Some(String::new()),
            harness: Some(String::new()),
            skill: Some(String::new()),
            extras: Some(String::new()),
            // ..RoleOverride::empty() — fully explicit, fully empty.
        },
    );
    assert!(
        panel.validate().is_ok(),
        "empty-string fields must pass validation — they are the inherit sentinel"
    );
    let payload = panel.to_session_overrides();
    // The role envelope is fully empty (every field dropped) so
    // the role drops out of `roles` too.
    assert!(
        !payload.roles.contains_key("runner"),
        "fully-empty role envelopes must drop out of session.json"
    );
}

/// AC-03: when the drive sets a custom topology, the payload's
/// `config_overrides.topology` reflects it. The runtime resolves
/// the topology from `config_overrides` first, falling back to
/// `autopilot.topology` in `mp config.json` if the session is
/// silent. A drive topology wins over a project topology.
#[test]
fn drive_topology_wins_over_project_default() {
    let mut panel = OverridePanel::default();
    panel.topology = "two-agent".to_string();
    let payload = panel.to_session_overrides();
    assert_eq!(payload.config_overrides.topology, "two-agent");
}

/// AC-03: the poll_interval_ms is derived from refresh_secs * 1000.
/// When the drive shortens the refresh, the session honors it for
/// its lifetime; a second session without overrides reverts to
/// the project default (or the 2s hard-coded fallback). The
/// payload's `poll_interval_ms` is the single source of truth
/// for the session's polling cadence.
#[test]
fn drive_refresh_wins_over_project_default() {
    let mut panel = OverridePanel::default();
    panel.refresh_secs = 5;
    let payload = panel.to_session_overrides();
    assert_eq!(payload.config_overrides.poll_interval_ms, Some(5000));

    // Boundary value — 1 second is the smallest valid cadence.
    panel.refresh_secs = 1;
    let payload = panel.to_session_overrides();
    assert_eq!(payload.config_overrides.poll_interval_ms, Some(1000));
}

/// AC-03: the precedence chain is global > drive > default. In
/// the panel's perspective:
/// - "global" = session.json `config_overrides` (always set, even
///   to defaults).
/// - "drive" = per-role `roles.<role>` blocks (only set when the
///   panel has explicit values).
/// - "default" = `mp config.json` `autopilot.roles.<role>.*` plus
///   the hard-coded fallback.
///
/// A round-trip through the typed payload keeps both layers
/// visible — the panel's `to_session_overrides()` builds both
/// blocks so the runtime can resolve at lookup time.
#[test]
fn precedence_chain_keeps_both_layers_visible_in_the_payload() {
    let mut panel = OverridePanel::default();
    panel.topology = "one-agent".to_string();
    panel.refresh_secs = 4;
    panel.roles.insert(
        "runner".to_string(),
        RoleOverride {
            model: Some("anthropic/claude-opus-4-1".to_string()),
            harness: Some("opencode".to_string()),
            ..RoleOverride::empty()
        },
    );

    let payload = panel.to_session_overrides();
    // Global layer (always populated).
    assert_eq!(payload.config_overrides.topology, "one-agent");
    assert_eq!(payload.config_overrides.poll_interval_ms, Some(4000));
    // Drive layer (only the runner is set; orchestrator/reviewer
    // inherit from project defaults).
    assert!(payload.roles.contains_key("runner"));
    assert!(!payload.roles.contains_key("orchestrator"));
    assert!(!payload.roles.contains_key("reviewer"));

    // Round-trip through serde to confirm the shape survives
    // JSON serialization (the production write goes through serde
    // before it lands on disk).
    let v = serde_json::to_value(&payload).unwrap();
    let back: SessionOverridesPayload = serde_json::from_value(v).unwrap();
    assert_eq!(back, payload);
}

/// AC-03: a payload with explicit topology + overrides on every
/// role carries every role into the persisted payload. The
/// runtime resolves each role's fields individually — drive wins,
/// default otherwise.
#[test]
fn every_role_with_overrides_persists_independently() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "orchestrator".to_string(),
        RoleOverride {
            harness: Some("cursor".to_string()),
            ..RoleOverride::empty()
        },
    );
    panel.roles.insert(
        "runner".to_string(),
        RoleOverride {
            harness: Some("opencode".to_string()),
            ..RoleOverride::empty()
        },
    );
    panel.roles.insert(
        "reviewer".to_string(),
        RoleOverride {
            harness: Some("pi".to_string()),
            ..RoleOverride::empty()
        },
    );
    let payload = panel.to_session_overrides();

    // All three roles present; each carries its drive harness.
    assert_eq!(
        payload.roles.keys().cloned().collect::<Vec<_>>(),
        vec![
            "orchestrator".to_string(),
            "reviewer".to_string(),
            "runner".to_string(),
        ],
        "BTreeMap iteration is alphabetical (orchestrator, reviewer, runner)"
    );
    assert_eq!(payload.roles["orchestrator"]["harness"], "cursor");
    assert_eq!(payload.roles["runner"]["harness"], "opencode");
    assert_eq!(payload.roles["reviewer"]["harness"], "pi");
}

/// AC-03: the precedence chain is consumable in a stable,
/// canonical form. The payload's `config_overrides` is always
/// populated, even when the panel is "empty". The runtime uses
/// the presence of the block to decide "this session was started
/// with explicit overrides" — an empty `config_overrides` would
/// be a schema violation (the field is required).
#[test]
fn config_overrides_block_is_always_populated_even_for_empty_panels() {
    let payload = SessionOverridesPayload::empty();
    let v = serde_json::to_value(&payload).unwrap();
    // The block is always present, even when empty.
    assert!(v.get("config_overrides").is_some());
    // Topology defaults to the canonical 3-agent value.
    assert_eq!(v["config_overrides"]["topology"], "three-agent");
    // poll_interval_ms defaults to 2s.
    assert_eq!(v["config_overrides"]["poll_interval_ms"], 2000);
    // `roles` is present (empty object for the empty case).
    assert!(v.get("roles").is_some());
    assert_eq!(v["roles"].as_object().unwrap().len(), 0);
}

/// AC-03: the precedence is enforceable from the panel alone — a
/// caller that constructs an `OverridePanel` and converts it to
/// `SessionOverridesPayload` never has to reach into the `mp`
/// autopilot types. The session.json write path is a thin
/// serde round-trip.
#[test]
fn panel_to_session_payload_is_a_pure_typed_round_trip() {
    let mut panel = OverridePanel::default();
    panel.topology = "two-agent".to_string();
    panel.refresh_secs = 7;
    let mut roles = BTreeMap::new();
    roles.insert(
        "reviewer".to_string(),
        RoleOverride {
            skill: Some("mp-coordinator".to_string()),
            ..RoleOverride::empty()
        },
    );
    panel.roles = roles;

    let payload = panel.to_session_overrides();
    let json = serde_json::to_string(&payload).unwrap();
    let back: SessionOverridesPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, payload);

    // The serialized payload's `config_overrides` carries the
    // panel-level topology + refresh, and the `roles` block
    // carries the drive override.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["config_overrides"]["topology"], "two-agent");
    assert_eq!(v["config_overrides"]["poll_interval_ms"], 7000);
    assert_eq!(v["roles"]["reviewer"]["skill"], "mp-coordinator");
}
