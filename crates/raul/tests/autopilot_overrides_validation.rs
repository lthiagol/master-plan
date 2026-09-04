//! M215 AC-02: Autopilot override panel validation.
//!
//! Override panel captures topology, refresh interval, and per-role
//! model/harness/skill/extras. Empty model/skill values mean
//! inherit; unknown harness, invalid topology, malformed extras,
//! and non-positive refresh values are rejected before persistence.

#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;

use raul::tui::autopilot::{
    validate_extras_json, OverridePanel, SessionOverridesPayload, ALLOWED_HARNESSES,
    ALLOWED_TOPOLOGIES, DEFAULT_REFRESH_SECS, DEFAULT_TOPOLOGY,
};

/// AC-02: the allow-list constants agree with the rest of the
/// autopilot stack. `ALLOWED_TOPOLOGIES` matches
/// `mp::autopilot::role::Topology` (kebab-case slugs) and
/// `ALLOWED_HARNESSES` matches
/// `mp::autopilot::prompts::spawn::SUPPORTED_AUTOPILOT_HARNESSES`.
/// The override panel must not silently disagree — a typo here
/// would let the panel accept a value the verifier rejects.
#[test]
fn allow_lists_agree_with_mp_autopilot_constants() {
    assert!(ALLOWED_TOPOLOGIES.contains(&"one-agent"));
    assert!(ALLOWED_TOPOLOGIES.contains(&"two-agent"));
    assert!(ALLOWED_TOPOLOGIES.contains(&"three-agent"));
    assert_eq!(ALLOWED_TOPOLOGIES.len(), 3);

    assert!(ALLOWED_HARNESSES.contains(&"opencode"));
    assert!(ALLOWED_HARNESSES.contains(&"cursor"));
    assert!(ALLOWED_HARNESSES.contains(&"pi"));
    assert_eq!(ALLOWED_HARNESSES.len(), 3);
}

/// AC-02: the default panel uses the canonical 3-agent topology and
/// the 2-second refresh interval. The defaults drive the picker
/// preview so the user sees the same cadence as the legacy Watch
/// surface — no surprise on first launch.
#[test]
fn default_panel_uses_three_agent_and_two_second_refresh() {
    let panel = OverridePanel::default();
    assert_eq!(panel.topology, DEFAULT_TOPOLOGY);
    assert_eq!(panel.topology, "three-agent");
    assert_eq!(panel.refresh_secs, DEFAULT_REFRESH_SECS);
    assert_eq!(panel.refresh_secs, 2);
    assert!(panel.roles.is_empty());
}

/// AC-02: `validate_extras_json` accepts empty strings (the
/// inherit sentinel) and well-formed JSON objects; rejects
/// malformed JSON and non-object payloads. The override panel
/// surfaces inline errors on bad extras input, so the validator
/// must distinguish "valid inherit" from "invalid input" cleanly.
#[test]
fn validate_extras_json_accepts_empty_and_objects_rejects_others() {
    // Empty = inherit — no validation needed.
    assert!(validate_extras_json("").is_ok());

    // Well-formed JSON object — passes.
    assert!(validate_extras_json(r#"{"k":1}"#).is_ok());
    assert!(validate_extras_json("{}").is_ok());

    // Malformed JSON — rejected.
    let err = validate_extras_json("{ not json").unwrap_err();
    assert!(
        err.to_lowercase().contains("json"),
        "error must explain the JSON parse failure: {err}"
    );

    // JSON but not an object — rejected (extras must be a map of
    // free-form key/value pairs).
    let err = validate_extras_json(r#""a plain string""#).unwrap_err();
    assert!(
        err.contains("object"),
        "error must explain that extras must be a JSON object: {err}"
    );

    let err = validate_extras_json("42").unwrap_err();
    assert!(err.contains("object"), "got {err}");

    let err = validate_extras_json("[1, 2, 3]").unwrap_err();
    assert!(err.contains("object"), "got {err}");
}

/// AC-02: the panel rejects an unknown topology with a
/// `UnknownTopology` error naming the bad value. The error
/// message includes the allow-list so the user can fix the typo
/// without consulting the docs.
#[test]
fn override_panel_rejects_unknown_topology() {
    let mut panel = OverridePanel::default();
    panel.topology = "four-agent".to_string();
    let err = panel.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("four-agent"), "msg: {msg}");
    assert!(msg.contains("three-agent"), "msg: {msg}");
}

/// AC-02: the panel rejects an unknown harness on any role with an
/// `UnknownHarness` error that names both the role and the bad
/// value. An empty string is the inherit sentinel — it passes
/// validation even though "empty" is not in the harness allow-list.
#[test]
fn override_panel_rejects_unknown_harness() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        raul::tui::autopilot::RoleOverride {
            harness: Some("claude-code".to_string()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    let err = panel.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("runner"), "msg: {msg}");
    assert!(msg.contains("claude-code"), "msg: {msg}");

    // Empty harness is the inherit sentinel — it must pass.
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        raul::tui::autopilot::RoleOverride {
            harness: Some(String::new()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    assert!(panel.validate().is_ok());
}

/// AC-02: the panel rejects malformed extras (any non-JSON
/// payload) with a `MalformedExtras` error that names the role
/// and surfaces the parse failure. The role key matters because
/// the renderer needs to know which row to flag.
#[test]
fn override_panel_rejects_malformed_extras_with_role_context() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "orchestrator".to_string(),
        raul::tui::autopilot::RoleOverride {
            extras: Some("{ not json".to_string()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    let err = panel.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("orchestrator"), "msg: {msg}");
    assert!(
        msg.to_lowercase().contains("json"),
        "msg must mention JSON parse failure: {msg}"
    );

    // Empty extras is the inherit sentinel — it passes.
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "orchestrator".to_string(),
        raul::tui::autopilot::RoleOverride {
            extras: Some(String::new()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    assert!(panel.validate().is_ok());
}

/// AC-02: the panel rejects a refresh_secs of 0 (any non-positive
/// value, but the picker never lets the user type a negative
/// number — the test pins the only reachable case).
#[test]
fn override_panel_rejects_non_positive_refresh() {
    let mut panel = OverridePanel::default();
    panel.refresh_secs = 0;
    let err = panel.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("refresh") || msg.contains("> 0"), "msg: {msg}");

    // Positive refresh_secs always passes — including the boundary
    // value of 1 (the smallest valid cadence).
    panel.refresh_secs = 1;
    assert!(panel.validate().is_ok());
}

/// AC-02: empty role envelopes drop out of the persisted payload
/// so an "I accept the defaults" panel produces a minimal session
/// payload. The non-empty roles keep every field they set.
#[test]
fn to_session_overrides_drops_empty_roles() {
    let mut panel = OverridePanel::default();
    // Only the runner gets an override; orchestrator/reviewer
    // stay inherited and must not bloat the session payload.
    panel.roles.insert(
        "runner".to_string(),
        raul::tui::autopilot::RoleOverride {
            model: Some("anthropic/claude-opus-4-1".to_string()),
            harness: Some("opencode".to_string()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    let payload = panel.to_session_overrides();

    assert_eq!(payload.config_overrides.topology, "three-agent");
    assert_eq!(payload.config_overrides.poll_interval_ms, Some(2000));

    // Only `runner` survives — empty role envelopes drop out.
    let mut expected_keys = std::collections::BTreeSet::new();
    expected_keys.insert("runner".to_string());
    let actual_keys: std::collections::BTreeSet<_> = payload.roles.keys().cloned().collect();
    assert_eq!(actual_keys, expected_keys);

    let runner = &payload.roles["runner"];
    assert_eq!(runner["model"], "anthropic/claude-opus-4-1");
    assert_eq!(runner["harness"], "opencode");
    // No skill/extras set — the keys drop out entirely.
    assert!(runner.get("skill").is_none());
    assert!(runner.get("extras").is_none());
}

/// AC-02: extras round-trip through the persisted payload as a
/// JSON object — the string the user types is parsed back into a
/// `Value` so `session.json` carries structured data the verifier
/// can read. The validator's gate (above) ensures the string is a
/// well-formed JSON object before the round-trip runs.
#[test]
fn override_panel_extras_round_trip_through_session_payload() {
    let mut panel = OverridePanel::default();
    panel.roles.insert(
        "runner".to_string(),
        raul::tui::autopilot::RoleOverride {
            extras: Some(r#"{"max_retries":3,"label":"r-1"}"#.to_string()),
            ..raul::tui::autopilot::RoleOverride::empty()
        },
    );
    let payload = panel.to_session_overrides();
    let extras = &payload.roles["runner"]["extras"];
    assert_eq!(extras["max_retries"], 3);
    assert_eq!(extras["label"], "r-1");
}

/// AC-02: the persisted payload's `config_overrides.poll_interval_ms`
/// is derived from the panel's `refresh_secs * 1000`. The session
/// contract stores milliseconds; the panel stores seconds (a
/// friendlier unit for the human-facing form).
#[test]
fn poll_interval_ms_is_refresh_secs_times_thousand() {
    let mut panel = OverridePanel::default();
    panel.refresh_secs = 5;
    let payload = panel.to_session_overrides();
    assert_eq!(payload.config_overrides.poll_interval_ms, Some(5000));
}

/// AC-02: `SessionOverridesPayload::empty()` is the
/// "no-overrides" baseline the precedence tests build on top of
/// (AC-03). Asserting its shape here pins the contract: empty
/// roles + canonical config overrides.
#[test]
fn empty_session_payload_baseline() {
    let payload = SessionOverridesPayload::empty();
    assert_eq!(payload.config_overrides.topology, DEFAULT_TOPOLOGY);
    assert_eq!(
        payload.config_overrides.poll_interval_ms,
        Some(DEFAULT_REFRESH_SECS * 1000)
    );
    assert!(payload.roles.is_empty());
}

/// AC-02: `BTreeMap` ordering on `OverridePanel::roles` is stable
/// so the persisted payload iterates in role-alphabetical order
/// (orchestrator / reviewer / runner). The verifier reads the
/// `roles` field by role key, but the iteration order matters for
/// deterministic diffs and tests.
#[test]
fn roles_iterate_in_alphabetical_order() {
    let mut panel = OverridePanel::default();
    // Insert in non-alphabetical order.
    for role in ["runner", "orchestrator", "reviewer"] {
        panel.roles.insert(
            role.to_string(),
            raul::tui::autopilot::RoleOverride {
                model: Some(format!("model-for-{role}")),
                ..raul::tui::autopilot::RoleOverride::empty()
            },
        );
    }
    let keys: Vec<&String> = panel.roles.keys().collect();
    assert_eq!(
        keys,
        vec![
            &"orchestrator".to_string(),
            &"reviewer".to_string(),
            &"runner".to_string(),
        ]
    );

    // The persisted payload's roles also iterate alphabetically.
    let payload = panel.to_session_overrides();
    let persisted_keys: Vec<&String> = payload.roles.keys().collect();
    assert_eq!(
        persisted_keys,
        vec![
            &"orchestrator".to_string(),
            &"reviewer".to_string(),
            &"runner".to_string(),
        ]
    );
}

/// AC-02: the panel keeps every well-known harness in the
/// allow-list so the panel never silently refuses a value the
/// `mp` autopilot stack supports. This is a defensive pin — a
/// future change to the allow-list that drops a working harness
/// fails here.
#[test]
fn every_well_known_harness_is_in_the_allow_list() {
    for h in ["opencode", "cursor", "pi"] {
        let mut panel = OverridePanel::default();
        panel.roles.insert(
            "runner".to_string(),
            raul::tui::autopilot::RoleOverride {
                harness: Some(h.to_string()),
                ..raul::tui::autopilot::RoleOverride::empty()
            },
        );
        assert!(panel.validate().is_ok(), "{h} must be in the allow-list");
    }
}

/// AC-02: every well-known topology is in the allow-list —
/// defensive pin against future regressions. The panel is the
/// human-facing form; an unreachable topology is a UX bug.
#[test]
fn every_well_known_topology_is_in_the_allow_list() {
    for t in ["one-agent", "two-agent", "three-agent"] {
        let mut panel = OverridePanel::default();
        panel.topology = t.to_string();
        assert!(panel.validate().is_ok(), "{t} must be in the allow-list");
    }
}

/// AC-02: the `BTreeMap` import is reachable so consumers that
/// want to construct the panel by hand (e.g., a future CLI mode)
/// can do so without reaching into the override module's private
/// API. This pins the type's reachability from the integration
/// test binary.
#[test]
fn btreemap_is_a_reusable_type_for_constructing_the_panel() {
    let mut roles: BTreeMap<String, raul::tui::autopilot::RoleOverride> = BTreeMap::new();
    roles.insert(
        "orchestrator".to_string(),
        raul::tui::autopilot::RoleOverride::empty(),
    );
    let panel = OverridePanel {
        topology: DEFAULT_TOPOLOGY.to_string(),
        roles,
        refresh_secs: DEFAULT_REFRESH_SECS,
    };
    assert!(panel.validate().is_ok());
}
