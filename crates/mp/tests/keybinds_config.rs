//! M138 S4 / AC-03: the `[keybinds]` section round-trips through
//! `mp config set` / `get` / `show`.
//!
//! mp owns the config schema; raul reads it via `mp config show`. This test
//! pins that a keybind override is stored, reported, and validated (unknown
//! action names error out) without touching the rest of the config.

mod common;

use crate::common::TestEnv;
use serde_json::{json, Value};

fn get(env: &TestEnv, key: &str) -> Value {
    env.run_json(&["config", "get", key, "--format", "json"])["value"].clone()
}

#[test]
fn keybind_defaults_to_canonical_form_when_unset() {
    // M200: `mp config get` now surfaces the canonical default from
    // `KEYBIND_DEFAULTS` when the user has not customized the action,
    // matching what raul applies at runtime. Pre-M200 this returned
    // null, which conflated "unset" with "I don't know what raul uses".
    let env = TestEnv::new();
    assert_eq!(get(&env, "keybinds.quit"), json!("q, Q"));
    assert_eq!(get(&env, "keybinds.up"), json!("Up, k"));
    assert_eq!(get(&env, "keybinds.refresh"), json!("Ctrl-R"));
}

#[test]
fn keybind_set_get_roundtrips() {
    let env = TestEnv::new();
    env.run(&["config", "set", "keybinds.quit", "ctrl+c"]);
    assert_eq!(get(&env, "keybinds.quit"), json!("ctrl+c"));

    env.run(&["config", "set", "keybinds.up", "k"]);
    assert_eq!(get(&env, "keybinds.up"), json!("k"));
}

#[test]
fn keybind_shows_in_config_show() {
    let env = TestEnv::new();
    env.run(&["config", "set", "keybinds.quit", "ctrl+c"]);
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert_eq!(report["config"]["keybinds"]["quit"], json!("ctrl+c"));
}

#[test]
fn empty_value_clears_the_override() {
    let env = TestEnv::new();
    env.run(&["config", "set", "keybinds.quit", "ctrl+c"]);
    assert_eq!(get(&env, "keybinds.quit"), json!("ctrl+c"));
    env.run(&["config", "set", "keybinds.quit", ""]);
    // After clearing the override, `config get` falls back to the
    // canonical default (M200) instead of null.
    assert_eq!(get(&env, "keybinds.quit"), json!("q, Q"));
}

#[test]
fn unknown_keybind_action_errors() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "keybinds.nonexistent",
        "x",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "setting an unknown keybind action should error"
    );

    let out = env.run(&["config", "get", "keybinds.nonexistent", "--format", "json"]);
    assert!(
        !out.status.success(),
        "getting an unknown keybind action should error"
    );
}

#[test]
fn keybind_set_does_not_disturb_other_config() {
    let env = TestEnv::new();
    env.run(&["config", "set", "ui.theme", "dracula"]);
    env.run(&["config", "set", "keybinds.quit", "ctrl+c"]);
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert_eq!(report["config"]["ui"]["theme"], json!("dracula"));
    assert_eq!(report["config"]["keybinds"]["quit"], json!("ctrl+c"));
}
