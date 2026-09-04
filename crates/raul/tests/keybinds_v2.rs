//! M139: v2 keybind tests — multi-key bindings, diagnostics, profile
//! export/import, and data-driven help display.
//!
//! Re-uses the v1 keybinds module surface; the v2 surface adds:
//!   * `BindingConfig::One | Many` untagged serde on the config value
//!   * `validated_keybinds()` returning diagnostics
//!   * `local_keybindings_profile_toml()` + `load_from_profile_toml()`
//!   * help display of multiple bindings for one action

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::keybinds::Keybinds;
use serde_json::json;

// ---------------------------------------------------------------------------
// M222 S7: migration precedence for `keybinds.toml`.
//
// The legacy `mp` project config surfaced the same set of
// overrides through the JSON `[keybinds]` section. M222
// introduces the user-level `~/.config/raul/keybinds.toml`.
// The transition contract:
//   * Hardcoded defaults < legacy `[keybinds]` JSON <
//     user-level `keybinds.toml`.
//   * `load_effective` returns the resolved `Keybinds`,
//     accumulating diagnostics, and a `hint_emitted` flag
//     the runner uses to surface the migration hint exactly
//     once when the legacy source is read.
// ---------------------------------------------------------------------------

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ---------------------------------------------------------------------------
// AC-01 — single action bound to multiple keys (untagged One|Many)
// ---------------------------------------------------------------------------

#[test]
fn one_form_still_works() {
    let cfg = json!({ "config": { "keybinds": { "quit": "x" } } });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert_eq!(kb.resolve(&ev(KeyCode::Char('x'))), Some(Action::Quit));
}

#[test]
fn many_form_binds_every_key() {
    let cfg = json!({
        "config": {
            "keybinds": {
                "quit": ["ctrl+w", "ctrl+shift+t"]
            }
        }
    });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    let w = ev_mods(KeyCode::Char('w'), KeyModifiers::CONTROL);
    let t = ev_mods(
        KeyCode::Char('t'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(kb.resolve(&w), Some(Action::Quit));
    assert_eq!(kb.resolve(&t), Some(Action::Quit));
}

#[test]
fn many_form_replaces_single() {
    // The v1 behavior (single key only) is replaced: a list of two overrides
    // the single default, so the old default `q` no longer resolves to Quit.
    let cfg = json!({
        "config": { "keybinds": { "quit": ["ctrl+w", "ctrl+shift+t"] } }
    });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), None);
}

fn ev_mods(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

#[test]
fn empty_list_disables_action() {
    let cfg = json!({ "config": { "keybinds": { "quit": [] } } });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), None);
}

// ---------------------------------------------------------------------------
// AC-02 — bad config does not crash; diagnostic + per-field fallback
// ---------------------------------------------------------------------------

#[test]
fn empty_string_emits_diagnostic_and_falls_back() {
    let cfg = json!({ "config": { "keybinds": { "quit": "" } } });
    let (diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert!(
        diags.iter().any(|d| d.field == "quit"),
        "empty string must emit a diagnostic; got: {diags:?}"
    );
    // Default binding still wins.
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn garbage_string_emits_diagnostic_and_falls_back() {
    let cfg = json!({ "config": { "keybinds": { "quit": "not-a-key" } } });
    let (diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert!(!diags.is_empty(), "garbage combo must emit a diagnostic");
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn unparseable_key_name_in_list_emits_diagnostic_and_falls_back() {
    let cfg = json!({
        "config": { "keybinds": { "quit": ["ctrl+w", "valid-looking but no", "ctrl+q"] } }
    });
    let (diags, kb) = Keybinds::validated_keybinds(&cfg);
    // The bad entry is reported; the whole field falls back to the default
    // (per-field fallback — a single bad combo invalidates the whole list
    // because the user almost certainly made a typo).
    assert!(
        diags.iter().any(|d| d.field == "quit"),
        "bad combo in list must surface a diagnostic; got: {diags:?}"
    );
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn wrong_type_emits_diagnostic_and_falls_back() {
    let cfg = json!({ "config": { "keybinds": { "quit": 42 } } });
    let (diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert!(!diags.is_empty());
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn one_bad_field_does_not_drop_others() {
    let cfg = json!({
        "config": { "keybinds": { "quit": "not-a-key", "up": "k" } }
    });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    // `up` override still applied; `quit` is at its default.
    assert_eq!(kb.resolve(&ev(KeyCode::Char('k'))), Some(Action::Up));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn conflicting_resolvable_bindings_emit_warning() {
    // Two resolvable actions on the same key. resolve() picks the
    // first-checked one (quit, per `resolve` field order); the warning
    // documents the shadowed later binding.
    let cfg = json!({
        "config": { "keybinds": { "quit": "ctrl+x", "escape": "ctrl+x" } }
    });
    let (diags, kb) = Keybinds::validated_keybinds(&cfg);
    assert!(
        diags.iter().any(|d| d.message.contains("also bound to")),
        "conflict must emit a diagnostic; got: {diags:?}"
    );
    // First match wins; the actual winner is documented, not guessed.
    assert_eq!(
        kb.resolve(&ev_mods(KeyCode::Char('x'), KeyModifiers::CONTROL)),
        Some(Action::Quit)
    );
}

// ---------------------------------------------------------------------------
// AC-03 — profile export round-trip
// ---------------------------------------------------------------------------

#[test]
fn profile_round_trips_special_char_keys() {
    // M139 code-review: the hand-written TOML writer/reader used to drop
    // bindings whose combo char is `#` (treated as a comment start) or
    // `"` (over-stripped by `trim_matches('"')`). Both are valid single-char
    // keys, so they must survive the round-trip.
    let kb = Keybinds {
        help: vec![(KeyCode::Char('#'), KeyModifiers::empty())],
        filter: vec![(KeyCode::Char('"'), KeyModifiers::empty())],
        // A multi-binding array containing a quoted-comma key (`,` spelled
        // as a raw char) must not be mis-split on the comma.
        quit: vec![
            (KeyCode::Char(','), KeyModifiers::empty()),
            (KeyCode::Char('x'), KeyModifiers::CONTROL),
        ],
        ..Default::default()
    };

    let profile = kb.local_keybindings_profile_toml();
    let (diags, reparsed) = Keybinds::load_from_profile_toml(&profile);
    assert!(
        diags.is_empty(),
        "special-char round-trip should not warn; got: {diags:?}\nprofile:\n{profile}"
    );
    assert_eq!(reparsed.help, kb.help, "`#` key dropped on round-trip");
    assert_eq!(reparsed.filter, kb.filter, "`\"` key dropped on round-trip");
    assert_eq!(
        reparsed.quit, kb.quit,
        "array with a `,`-key mis-split on round-trip"
    );
}

#[test]
fn profile_round_trips_user_displaced_defaults() {
    // Build a Keybinds that differs from defaults on a couple of fields.
    // `help` defaults to a single binding `?`, so `help = "?"` matches the
    // default and must NOT appear in the profile.
    let kb = Keybinds {
        quit: vec![
            (KeyCode::Char('w'), KeyModifiers::CONTROL),
            (
                KeyCode::Char('t'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
        ],
        help: vec![(KeyCode::Char('?'), KeyModifiers::empty())], // same as default
        ..Default::default()
    };

    let profile = kb.local_keybindings_profile_toml();
    // The default-equal `help` field must NOT appear in the profile.
    assert!(
        !profile.contains("help = "),
        "default-equal field leaked; got:\n{profile}"
    );

    // The displaced `quit` field must appear with the array form.
    assert!(
        profile.contains("quit = ["),
        "array form missing; got:\n{profile}"
    );

    // Round-trip: parse the profile back and assert the displaced field
    // survives; the equal field stays at the default.
    let (diags, reparsed) = Keybinds::load_from_profile_toml(&profile);
    assert!(
        diags.is_empty(),
        "round-trip should not error; got: {diags:?}"
    );
    assert_eq!(
        reparsed.quit, kb.quit,
        "displaced binding did not round-trip"
    );
    assert_eq!(
        reparsed.help,
        Keybinds::default().help,
        "default-equal field was not preserved as default"
    );
}

#[test]
fn profile_drops_default_disabled_bindings() {
    // A user who "disabled" a default by setting the field to an empty list
    // IS represented in the parsed config (parsed kb will have empty vec)
    // but the profile does NOT include it (we can't tell "user removed"
    // from "never set", so we don't try). Round-trip therefore turns the
    // disabled field back into the default — documented behavior.
    let kb = Keybinds {
        quit: vec![],
        ..Default::default()
    };
    let profile = kb.local_keybindings_profile_toml();
    assert!(
        !profile.contains("quit ="),
        "empty field should not appear in profile; got:\n{profile}"
    );
    let (_diags, reparsed) = Keybinds::load_from_profile_toml(&profile);
    assert_eq!(
        reparsed.quit,
        Keybinds::default().quit,
        "after round-trip, the empty field is restored to default"
    );
}

#[test]
fn profile_partial_displacement_round_trips() {
    // Only one of several fields changed.
    let cfg = json!({ "config": { "keybinds": { "up": "k" } } });
    let (_diags, kb) = Keybinds::validated_keybinds(&cfg);
    let profile = kb.local_keybindings_profile_toml();
    let (diags, reparsed) = Keybinds::load_from_profile_toml(&profile);
    assert!(
        diags.is_empty(),
        "round-trip should not error; got: {diags:?}"
    );
    assert_eq!(reparsed, kb);
}

// ---------------------------------------------------------------------------
// AC-04 — help display lists ALL bindings of a multi-binding action
// ---------------------------------------------------------------------------

#[test]
fn help_lists_all_bindings_for_multi_binding_action() {
    let kb = Keybinds {
        quit: vec![
            (KeyCode::Char('w'), KeyModifiers::CONTROL),
            (
                KeyCode::Char('t'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
        ],
        ..Default::default()
    };
    let entries = kb.help_entries();
    let quit = entries
        .iter()
        .find(|e| e.label == "Quit")
        .expect("Quit entry present");
    let display = quit.keys_display();
    assert!(
        display.contains("Ctrl+w"),
        "first binding missing: {display}"
    );
    assert!(
        display.contains("Ctrl+Shift+t"),
        "second binding missing: {display}"
    );
    // Bindings are joined in the order they were declared.
    let parts: Vec<&str> = display.split(',').map(str::trim).collect();
    assert_eq!(
        parts.len(),
        2,
        "expected exactly two bindings; got: {parts:?}"
    );
}

// ---------------------------------------------------------------------------
// M222 S7: layered loader. New tests below pin the precedence
// contract that user-level keybinds.toml > legacy mp-config
// JSON > hardcoded defaults. Reads from either source are
// always non-destructive; the legacy source emits one
// migration hint.
// ---------------------------------------------------------------------------

fn pair(c: KeyCode, m: KeyModifiers) -> (KeyCode, KeyModifiers) {
    (c, m)
}

/// AC-08 (precedence): user-level TOML wins over legacy JSON.
/// Without a user-level file, the legacy `[keybinds]` JSON
/// routes through to the effective map; with both sources
/// reading the same action, the TOML wins.
#[test]
fn load_effective_user_level_toml_overrides_legacy_json() {
    let legacy = json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x"
            }
        }
    });
    let toml = "[global]\nquit = \"ctrl+y\"\n";
    let (kb, diags, hint) = Keybinds::load_effective(Some(&legacy), Some(toml));
    assert!(diags.is_empty());
    assert!(hint, "legacy use must emit a migration hint");
    assert_eq!(
        kb.quit,
        vec![pair(KeyCode::Char('y'), KeyModifiers::CONTROL)],
        "user-level TOML must win over legacy JSON"
    );
}

/// AC-08 (no rewrite): reads from the legacy `[keybinds]`
/// section never update the source. We assert the source
/// payload is unchanged by passing an owned `Value` and
/// comparing it post-call.
#[test]
fn load_effective_does_not_mutate_legacy_json() {
    let legacy_json = json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x"
            },
            "ui": { "color": true }
        }
    });
    let before = legacy_json.clone();
    let _ = Keybinds::load_effective(Some(&legacy_json), None);
    assert_eq!(
        legacy_json, before,
        "load_effective must not mutate the legacy JSON payload"
    );
}

/// AC-08 (legacy wins over hardcoded defaults): with no
/// user-level TOML, the legacy JSON override for `quit`
/// reaches the effective map.
#[test]
fn load_effective_legacy_json_overrides_default() {
    let legacy = json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x"
            }
        }
    });
    let (kb, _, _) = Keybinds::load_effective(Some(&legacy), None);
    assert_eq!(
        kb.quit,
        vec![pair(KeyCode::Char('x'), KeyModifiers::CONTROL)],
        "legacy mp config must reach the effective map when no TOML is present"
    );
}

/// AC-08 (one migration hint): the layered loader returns a
/// `hint_emitted` flag the runner uses to push the migration
/// notice to stderr *once*. The flag MUST be `false` when no
/// legacy source is read and `true` only when the legacy
/// `[keybinds]` section was actually consulted.
#[test]
fn load_effective_hint_emitted_only_when_legacy_section_is_present() {
    // No legacy section: hint = false.
    let (_no_legacy, diags1, hint1) =
        Keybinds::load_effective(None, Some("[global]\nquit = \"x\"\n"));
    assert!(!hint1, "without legacy JSON the hint must NOT fire");
    assert!(diags1.is_empty(), "clean TOML must not warn");

    // Legacy section present but no user-level TOML: hint = true.
    let legacy = json!({ "config": { "keybinds": { "quit": "x" } } });
    let (legacy_only, _, hint2) = Keybinds::load_effective(Some(&legacy), None);
    assert!(
        hint2,
        "legacy source presence must fire the hint exactly once"
    );
    assert_eq!(
        legacy_only.quit,
        vec![pair(KeyCode::Char('x'), KeyModifiers::empty())]
    );

    // Both sources present: hint = true (legacy still consulted).
    let (both, _, hint3) =
        Keybinds::load_effective(Some(&legacy), Some("[global]\nquit = \"y\"\n"));
    assert!(
        hint3,
        "legacy source use must fire the hint exactly once per load"
    );
    assert_eq!(
        both.quit,
        vec![pair(KeyCode::Char('y'), KeyModifiers::empty())]
    );
}

/// AC-08 (per-lane precedence): the user-level TOML also wins
/// over the legacy JSON for per-lane actions. The autopilot
/// section is the only per-lane surface in v1.
#[test]
fn load_effective_user_toml_overrides_legacy_for_lane_autopilot() {
    let legacy = json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x"
            }
        }
    });
    let toml = "[autopilot]\nselect = \"f1\"\n";
    let (kb, _, hint) = Keybinds::load_effective(Some(&legacy), Some(toml));
    assert!(hint);
    assert_eq!(
        kb.quit,
        vec![pair(KeyCode::Char('x'), KeyModifiers::CONTROL)],
        "legacy global override still wins for fields not in TOML"
    );
    assert_eq!(
        kb.lane_autopilot.select,
        vec![pair(KeyCode::F(1), KeyModifiers::empty())],
        "TOML autopilot override applies to the per-lane field"
    );
}

/// AC-08 (no legacy source): both layers skip cleanly. No
/// hint, no diagnostics; defaults end-to-end.
#[test]
fn load_effective_with_no_legacy_and_no_toml_keeps_defaults() {
    let (kb, diags, hint) = Keybinds::load_effective(None, None);
    assert!(!hint);
    assert!(diags.is_empty());
    assert_eq!(kb, Keybinds::default());
}
