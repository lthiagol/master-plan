use crate::common::TestEnv;
use serde_json::{json, Value};

fn get(env: &TestEnv, key: &str) -> Value {
    env.run_json(&["config", "get", key, "--format", "json"])["value"].clone()
}

#[test]
fn ui_defaults_when_unset() {
    let env = TestEnv::new();
    assert_eq!(get(&env, "ui.color"), Value::Bool(true));
    assert_eq!(get(&env, "ui.icons"), json!("unicode"));
    assert_eq!(get(&env, "ui.theme"), json!("mocha"));
    assert_eq!(get(&env, "ui.hide_done"), Value::Bool(false));
    // M198 WP1: the Watch tab is hidden by default. The
    // operator opts in via `mp config set ui.show_autopilot_tab
    // true`. The doctor + raul surfaces read this flag to
    // filter the Watch lane out of the tab bar.
    assert_eq!(get(&env, "ui.show_autopilot_tab"), Value::Bool(false));
}

#[test]
fn ui_set_get_roundtrips() {
    let env = TestEnv::new();
    env.run(&["config", "set", "ui.color", "false"]);
    assert_eq!(get(&env, "ui.color"), Value::Bool(false));

    env.run(&["config", "set", "ui.icons", "ascii"]);
    assert_eq!(get(&env, "ui.icons"), json!("ascii"));

    env.run(&["config", "set", "ui.theme", "dracula"]);
    assert_eq!(get(&env, "ui.theme"), json!("dracula"));

    env.run(&["config", "set", "ui.hide_done", "true"]);
    assert_eq!(get(&env, "ui.hide_done"), Value::Bool(true));

    // M198: same round-trip contract as `ui.hide_done`.
    env.run(&["config", "set", "ui.show_autopilot_tab", "true"]);
    assert_eq!(get(&env, "ui.show_autopilot_tab"), Value::Bool(true));
    env.run(&["config", "set", "ui.show_autopilot_tab", "false"]);
    assert_eq!(get(&env, "ui.show_autopilot_tab"), Value::Bool(false));
}

#[test]
fn ui_theme_accepts_every_advertised_palette() {
    let env = TestEnv::new();
    // Every name in the single source of truth must round-trip through
    // `mp config set` — the Settings picker writes exactly these. A
    // name that validates in the schema but is rejected here would
    // make the picker's save path fail at the last step.
    for theme in mp_model::UI_THEMES {
        env.run(&["config", "set", "ui.theme", theme]);
        assert_eq!(
            get(&env, "ui.theme"),
            json!(theme),
            "ui.theme = {theme} must round-trip"
        );
    }
    // The default is one of the advertised values.
    assert!(mp_model::UI_THEMES.contains(&"mocha"));
}

#[test]
fn ui_theme_rejects_unknown_value() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "ui.theme", "moxha", "--format", "json"]);
    assert!(
        !out.status.success(),
        "ui.theme must reject a name outside UI_THEMES"
    );
}

#[test]
fn ui_icons_rejects_invalid_value() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "ui.icons", "emoji", "--format", "json"]);
    assert!(
        !out.status.success(),
        "ui.icons should reject values outside none|ascii|unicode"
    );
}

#[test]
fn unknown_ui_key_errors() {
    let env = TestEnv::new();
    let out = env.run(&["config", "get", "ui.nonexistent", "--format", "json"]);
    assert!(!out.status.success(), "unknown config key should error");
}

#[test]
fn show_includes_ui_after_set() {
    let env = TestEnv::new();
    env.run(&["config", "set", "ui.theme", "mocha"]);
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert_eq!(report["config"]["ui"]["theme"], json!("mocha"));
}
