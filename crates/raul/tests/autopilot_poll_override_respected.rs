//! M217 / AC-04 — `refresh_secs` override precedence.
//!
//! The resolution chain, highest priority first:
//!
//! 1. `session.json` → `config_overrides.refresh_secs` (what D2's
//!    override panel writes)
//! 2. `config.json` → `autopilot.refresh_secs`
//! 3. the built-in 2s default
//!
//! An explicit user override always beats the project default, and
//! the project default always beats the built-in. The chain never
//! falls back *up*: a present-but-unusable value at a higher link
//! is normalized (clamped) at that link rather than silently
//! deferring to the next one, so the operator's intent is never
//! quietly discarded.
//!
//! The resolved link is reported alongside the number
//! ([`RefreshSource`]) — the footer renders it, and these tests
//! assert on it, so "2s" from an override and "2s" from the
//! default are distinguishable.

use raul::tui::poll::{
    resolve_refresh_policy, resolve_refresh_secs, AutopilotPoller, PollDecision, RefreshSource,
    DEFAULT_REFRESH_SECS, MAX_REFRESH_SECS, MIN_REFRESH_SECS,
};
use serde_json::{json, Value};

fn session_override(secs: Value) -> Value {
    json!({"session": {"id": "alpha", "config_overrides": {"refresh_secs": secs}}})
}

fn project_config(secs: Value) -> Value {
    json!({"autopilot": {"refresh_secs": secs}})
}

// ─── the three links ────────────────────────────────────────────

#[test]
fn m217_ac04_builtin_default_applies_when_nothing_is_configured() {
    let p = resolve_refresh_policy(&Value::Null, &Value::Null);
    assert_eq!(p.secs, DEFAULT_REFRESH_SECS);
    assert_eq!(p.secs, 2, "the polish default is 2s");
    assert_eq!(p.source, RefreshSource::BuiltinDefault);
}

#[test]
fn m217_ac04_empty_objects_are_the_same_as_absent() {
    let p = resolve_refresh_policy(&json!({"session": {}}), &json!({"autopilot": {}}));
    assert_eq!(p.secs, DEFAULT_REFRESH_SECS);
    assert_eq!(p.source, RefreshSource::BuiltinDefault);
}

#[test]
fn m217_ac04_project_config_beats_the_builtin_default() {
    let p = resolve_refresh_policy(&Value::Null, &project_config(json!(4)));
    assert_eq!(p.secs, 4);
    assert_eq!(p.source, RefreshSource::ProjectConfig);
}

#[test]
fn m217_ac04_session_override_beats_the_project_config() {
    let p = resolve_refresh_policy(&session_override(json!(9)), &project_config(json!(4)));
    assert_eq!(
        p.secs, 9,
        "the per-drive override the operator typed must win"
    );
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

#[test]
fn m217_ac04_session_override_beats_the_builtin_default_too() {
    let p = resolve_refresh_policy(&session_override(json!(7)), &Value::Null);
    assert_eq!(p.secs, 7);
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

#[test]
fn m217_ac04_precedence_table_covers_every_present_absent_combination() {
    // Every (session present?, config present?) combination, with
    // the expected winner spelled out — the table *is* the
    // precedence contract.
    let cases: [(Option<u64>, Option<u64>, u64, RefreshSource); 4] = [
        (None, None, 2, RefreshSource::BuiltinDefault),
        (None, Some(4), 4, RefreshSource::ProjectConfig),
        (Some(9), None, 9, RefreshSource::SessionOverride),
        (Some(9), Some(4), 9, RefreshSource::SessionOverride),
    ];
    for (session, config, expected_secs, expected_source) in cases {
        let s = session
            .map(|v| session_override(json!(v)))
            .unwrap_or(Value::Null);
        let c = config
            .map(|v| project_config(json!(v)))
            .unwrap_or(Value::Null);
        let p = resolve_refresh_policy(&s, &c);
        assert_eq!(
            (p.secs, p.source),
            (expected_secs, expected_source),
            "session={session:?} config={config:?}"
        );
    }
}

// ─── payload-shape tolerance ────────────────────────────────────

#[test]
fn m217_ac04_override_is_read_from_an_unwrapped_session_envelope() {
    // `mp autopilot session show` may emit the session fields at
    // the payload root rather than under a `session` key.
    let unwrapped = json!({"id": "alpha", "config_overrides": {"refresh_secs": 6}});
    let p = resolve_refresh_policy(&unwrapped, &Value::Null);
    assert_eq!(p.secs, 6);
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

#[test]
fn m217_ac04_numeric_strings_are_accepted() {
    // The override panel collects free text; a value that has not
    // been normalized to a number yet must still be honoured
    // rather than silently falling through to the default.
    let p = resolve_refresh_policy(&session_override(json!("5")), &project_config(json!(3)));
    assert_eq!(p.secs, 5);
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

#[test]
fn m217_ac04_unparseable_override_falls_through_to_the_next_link() {
    // A value that carries no cadence at all (not a number, not a
    // numeric string) is not an override — fall through rather
    // than invent a number.
    for garbage in [json!("soon"), json!(true), json!(null), json!({"a": 1})] {
        let p = resolve_refresh_policy(
            &session_override(garbage.clone()),
            &project_config(json!(4)),
        );
        assert_eq!(p.secs, 4, "garbage override {garbage:?}");
        assert_eq!(p.source, RefreshSource::ProjectConfig);
    }
}

// ─── clamping ───────────────────────────────────────────────────

#[test]
fn m217_ac04_zero_is_clamped_to_the_minimum_not_treated_as_absent() {
    // `refresh_secs: 0` would turn the idle hook into a busy loop.
    // It is still an *override* (the operator asked for "as fast
    // as possible"), so it clamps at its own link rather than
    // deferring to the project config.
    let p = resolve_refresh_policy(&session_override(json!(0)), &project_config(json!(4)));
    assert_eq!(p.secs, MIN_REFRESH_SECS);
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

#[test]
fn m217_ac04_absurdly_large_values_are_clamped_to_the_maximum() {
    let p = resolve_refresh_policy(&session_override(json!(86_400)), &Value::Null);
    assert_eq!(p.secs, MAX_REFRESH_SECS);
    assert_eq!(p.source, RefreshSource::SessionOverride);
}

// ─── the poller honours the resolved policy ─────────────────────

#[test]
fn m217_ac04_poller_adopts_the_resolved_cadence() {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    assert_eq!(p.refresh_secs(), 2);
    p.apply_policy_from_payloads(&session_override(json!(9)), &project_config(json!(4)));
    assert_eq!(p.refresh_secs(), 9);
    assert_eq!(p.interval_ms(), 9_000);

    p.begin(0);
    p.finish(0);
    assert_eq!(
        p.begin(8_999),
        PollDecision::NotDue,
        "the 9s override must actually gate the schedule, not just the label"
    );
    assert_eq!(p.begin(9_000), PollDecision::Fire);
}

#[test]
fn m217_ac04_adopting_a_policy_does_not_force_an_extra_request() {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    p.begin(0);
    p.finish(0);
    // A session override arriving mid-drive takes effect on the
    // next due check; it must not reset the timer (which would
    // let a payload change trigger an unscheduled request).
    p.apply_policy_from_payloads(&session_override(json!(9)), &Value::Null);
    assert_eq!(p.begin(10), PollDecision::NotDue);
    assert_eq!(p.fired_count(), 1);
}

#[test]
fn m217_ac04_footer_names_the_winning_link() {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    assert!(
        p.footer_label().contains("default"),
        "got {:?}",
        p.footer_label()
    );
    p.apply_policy_from_payloads(&Value::Null, &project_config(json!(4)));
    assert!(p.footer_label().contains("4s"));
    assert!(p.footer_label().contains("config"));
    p.apply_policy_from_payloads(&session_override(json!(9)), &project_config(json!(4)));
    assert!(p.footer_label().contains("9s"));
    assert!(
        p.footer_label().contains("session"),
        "the operator must be able to see *why* the cadence is what it is; got {:?}",
        p.footer_label()
    );
}

#[test]
fn m217_ac04_convenience_wrapper_agrees_with_the_full_resolver() {
    let s = session_override(json!(9));
    let c = project_config(json!(4));
    assert_eq!(
        resolve_refresh_secs(&s, &c),
        resolve_refresh_policy(&s, &c).secs
    );
    assert_eq!(resolve_refresh_secs(&Value::Null, &Value::Null), 2);
}
