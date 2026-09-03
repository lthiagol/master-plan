//! M214 AC-02: settings key migration.
//!
//! `ui.show_autopilot_tab` controls Autopilot lane visibility. If
//! it is absent and `ui.show_watch_tab` exists, reads honor the
//! legacy value without rewriting. The next explicit Settings save
//! persists the new key. When both keys exist, the new key wins.

use raul::config::UiConfig;
use serde_json::json;

/// AC-02: with only `ui.show_watch_tab = true` in the config payload,
/// `UiConfig::show_autopilot_tab` reads as `true`. The read does
/// NOT rewrite the legacy key — `show_autopilot_tab` stays absent
/// in the input payload after the parse.
#[test]
fn legacy_key_only_is_honored_without_rewrite() {
    let payload = json!({
        "config": {
            "ui": {
                "color": true,
                "hide_done": false,
                "show_watch_tab": true,
            }
        }
    });
    let cfg = UiConfig::from_config_payload(&payload);
    assert!(
        cfg.show_autopilot_tab,
        "legacy `ui.show_watch_tab = true` must surface as `show_autopilot_tab = true`"
    );
    // The original payload is preserved verbatim — the read does
    // not rewrite the legacy key. The next explicit Settings save
    // writes the new key on top.
    assert_eq!(
        payload["config"]["ui"].get("show_autopilot_tab"),
        None,
        "from_config_payload must not mutate the input payload"
    );
    assert_eq!(
        payload["config"]["ui"]["show_watch_tab"],
        json!(true),
        "legacy key must remain in the input payload — the read shim never deletes it"
    );
}

/// AC-02: with only `ui.show_autopilot_tab = true`, the new key
/// drives `show_autopilot_tab` directly. The legacy key is not
/// consulted.
#[test]
fn new_key_only_is_used_directly() {
    let payload = json!({
        "config": {
            "ui": {
                "show_autopilot_tab": true,
            }
        }
    });
    let cfg = UiConfig::from_config_payload(&payload);
    assert!(cfg.show_autopilot_tab);
}

/// AC-02: when both keys are present, the new key wins so a
/// user-driven save overrides any stale legacy value.
#[test]
fn new_key_wins_over_legacy_when_both_present() {
    let payload = json!({
        "config": {
            "ui": {
                "show_autopilot_tab": false,
                "show_watch_tab": true,
            }
        }
    });
    let cfg = UiConfig::from_config_payload(&payload);
    assert!(
        !cfg.show_autopilot_tab,
        "new key must win over legacy when both are present (user-driven save overrides stale legacy value)"
    );
}

/// AC-02: with neither key present, the default `false` is honored.
/// A missing config never accidentally re-enables the tab.
#[test]
fn absent_keys_default_to_false() {
    let payload = json!({
        "config": {
            "ui": {
                "color": true,
            }
        }
    });
    let cfg = UiConfig::from_config_payload(&payload);
    assert!(
        !cfg.show_autopilot_tab,
        "default `false` must hold when neither key is present"
    );
}

/// AC-02: the App field mirrors the parsed value. `App::show_autopilot_tab`
/// defaults to `false` in `App::new` — same as the parsed default — so a
/// fresh app behaves identically to an app loaded with no key.
#[test]
fn app_default_matches_parsed_default() {
    use raul::tui::app::App;
    let app = App::new();
    assert!(
        !app.show_autopilot_tab,
        "App::show_autopilot_tab must default to false in App::new()"
    );
}

/// AC-02: when the operator toggles `ui.show_autopilot_tab = true`
/// via the raul Settings lane and presses `s` to save, the staged
/// edit writes through `mp config set ui.show_autopilot_tab true`
/// (the new key — the legacy key is not consulted on the write
/// path). Pin this with a dry-run: the staged key string is exactly
/// `ui.show_autopilot_tab` regardless of which value is currently on
/// disk.
#[test]
fn settings_lane_saves_under_new_key_only() {
    use raul::tui::modes::settings::SETTINGS_KEYS;
    // The Settings lane's flat key list must contain `ui.show_autopilot_tab`
    // and must NOT contain `ui.show_watch_tab` — saving the new key writes
    // to the new key only, never the legacy key.
    let keys: Vec<&str> = SETTINGS_KEYS.iter().map(|(_, k)| *k).collect();
    assert!(
        keys.contains(&"ui.show_autopilot_tab"),
        "Settings lane must expose ui.show_autopilot_tab; got {keys:?}"
    );
    assert!(
        !keys.contains(&"ui.show_watch_tab"),
        "Settings lane must NOT expose ui.show_watch_tab — the legacy key is read-only; got {keys:?}"
    );
}
