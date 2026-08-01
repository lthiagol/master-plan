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
