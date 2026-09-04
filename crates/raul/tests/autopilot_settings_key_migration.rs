//! M214 / M229 AC-02: settings key migration + M229 cleanup.
//!
//! `ui.show_autopilot_tab` is the canonical key for the Autopilot
//! lane visibility toggle. M229 removed the legacy
//! `ui.show_watch_tab` back-compat shim from
//! `UiConfig::from_config_payload`: configs carrying only the
//! legacy key now see the default `false` after upgrade. Operators
//! who kept the tab visible must opt in explicitly via the new key.

use raul::config::UiConfig;
use serde_json::json;

/// M229: with only `ui.show_watch_tab = true` in the config payload,
/// the legacy key is no longer honored. `show_autopilot_tab` reads
/// as `false` (the default). The read also does NOT promote the
/// legacy value to the new key — that responsibility belongs to
/// the on-disk upgrade path.
#[test]
fn legacy_key_alone_no_longer_drives_show_autopilot_tab() {
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
        !cfg.show_autopilot_tab,
        "M229 removed the legacy show_watch_tab back-compat shim; the legacy key must NOT drive show_autopilot_tab"
    );
}

/// M214: with only `ui.show_autopilot_tab = true`, the new key
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

/// M229: when both keys are present, the new key wins. The legacy
/// key has no back-compat meaning; the new key is authoritative.
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
        "new key must win over legacy when both are present"
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

/// App default matches parsed default.
#[test]
fn app_default_matches_parsed_default() {
    use raul::tui::app::App;
    let app = App::new();
    assert!(
        !app.show_autopilot_tab,
        "App::show_autopilot_tab must default to false in App::new()"
    );
}

/// Settings lane does not advertise the legacy key.
#[test]
fn settings_lane_exposes_only_new_key() {
    use raul::tui::modes::settings::SETTINGS_KEYS;
    let keys: Vec<&str> = SETTINGS_KEYS.iter().map(|(_, k)| *k).collect();
    assert!(
        keys.contains(&"ui.show_autopilot_tab"),
        "Settings lane must expose ui.show_autopilot_tab; got {keys:?}"
    );
    assert!(
        !keys.contains(&"ui.show_watch_tab"),
        "Settings lane must NOT expose ui.show_watch_tab — the legacy key was removed; got {keys:?}"
    );
}
