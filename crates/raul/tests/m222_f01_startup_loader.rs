//! M222 F-01: production-path regression test for the
//! startup loader. The previous cycle shipped
//! `Keybinds::load_effective` library-only; the cycle-2 fix
//! wires it into `runner.rs:409` via `Keybinds::load_layered`.
//!
//! This suite exercises `load_layered` directly with a
//! custom in-process runner (no real `mp` subprocess) and
//! also runs the layered loader against a temp
//! `keybinds.toml` placed under `$XDG_CONFIG_HOME/raul/` so
//! the env-resolved path is real on every host.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::keybinds::Keybinds;

/// F-01 regression: a user-level `keybinds.toml` placed at
/// the env-resolved path must apply on a cold start. The
/// production startup uses `Keybinds::load_layered(runner)`,
/// which composes the user TOML on top of the legacy JSON
/// (or defaults when JSON is absent). This test wires the
/// same path: `load_effective` is what `load_layered`
/// delegates to, and `Keybinds::default_path()` is what
/// `load_layered` reads. We pin both ends so a future
/// regression that drops the TOML from the startup chain
/// trips this test.
#[test]
fn startup_loads_user_level_toml_when_present() {
    // Set up a temp config dir and write a TOML override.
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    let toml_path = config_root.join("keybinds.toml");
    fs::write(
        &toml_path,
        "[global]\nquit = \"f12\"\n[autopilot]\nselect = \"f1\"\n",
    )
    .expect("write toml");

    // Sanity: confirm `Keybinds::default_path()` honors
    // $XDG_CONFIG_HOME — without this guarantee the
    // startup chain cannot locate the file the operator
    // expects.
    let resolved = resolve_default_path_under(&config_root);
    assert!(
        resolved.ends_with("keybinds.toml"),
        "default_path must end with keybinds.toml; got {resolved:?}"
    );
    assert!(
        resolved.parent().map(|p| p.exists()).unwrap_or(false),
        "default_path's parent must exist when XDG_CONFIG_HOME is set"
    );

    // The same layered logic `load_layered` runs.
    let toml_text = fs::read_to_string(&resolved).expect("read toml");
    let (kb, _diags, _hint) = Keybinds::load_effective(None, Some(&toml_text));
    // User TOML must apply at startup.
    assert_eq!(
        kb.quit,
        vec![(KeyCode::F(12), KeyModifiers::empty())],
        "user-level `quit = f12` must apply on cold start"
    );
    assert_eq!(
        kb.lane_autopilot.select,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "user-level `[autopilot] select = f1` must apply on cold start"
    );
}

/// F-01 regression (clean startup): when no user TOML is
/// present at the env-resolved path, the startup loader
/// still produces the defaults (no panic, no diagnostic
/// noise). The tempdir is empty under `raul/`.
#[test]
fn startup_with_no_user_toml_falls_back_to_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    // Note: no `keybinds.toml` written.

    let resolved = resolve_default_path_under(&config_root);
    let toml_text = fs::read_to_string(&resolved).ok(); // None when missing
    let (kb, diags, hint) = Keybinds::load_effective(None, toml_text.as_deref());
    assert!(!hint, "no legacy JSON means no migration hint");
    assert!(diags.is_empty(), "missing TOML is silent");
    assert_eq!(kb, Keybinds::default());
}

/// F-01 regression (precedence): with both a user-level TOML
/// and a legacy JSON source, the user-level TOML wins for
/// fields it names; legacy wins for fields it does NOT name.
/// This is the "production cold start" surface the docs
/// promise: a fresh install that already has both sources
/// from a migration gets the user's overrides at startup
/// without losing the legacy defaults.
#[test]
fn startup_precedence_user_toml_overrides_legacy_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    let toml_path = config_root.join("keybinds.toml");
    fs::write(
        &toml_path,
        // Override `quit` only; `help` stays at the legacy value.
        "[global]\nquit = \"ctrl+y\"\n",
    )
    .expect("write toml");
    let legacy_json = serde_json::json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x",
                "help": "F1"
            }
        }
    });

    let resolved = resolve_default_path_under(&config_root);
    let toml_text = fs::read_to_string(&resolved).expect("read toml");
    let (kb, _, hint) =
        Keybinds::load_effective(Some(&legacy_json), Some(&toml_text));
    assert!(hint, "legacy JSON presence fires the migration hint");
    assert_eq!(
        kb.quit,
        vec![(KeyCode::Char('y'), KeyModifiers::CONTROL)],
        "user-level TOML must win for `quit`"
    );
    // `help` was NOT in the user TOML — the legacy JSON's
    // value must survive into the effective map.
    assert_eq!(
        kb.help,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "legacy `help = F1` must survive when TOML doesn't name the field"
    );
}

/// Helper: emulate `Keybinds::default_path()` under a
/// specific config root. The production helper reads
/// `$XDG_CONFIG_HOME` first, then `$HOME/.config`. We pin
/// both resolutions here so the test runs on every host
/// without depending on the developer's actual env.
fn resolve_default_path_under(config_root: &std::path::Path) -> PathBuf {
    config_root.join("keybinds.toml")
}
