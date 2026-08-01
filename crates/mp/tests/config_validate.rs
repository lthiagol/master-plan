//! M156 AC-01: `mp config validate` emits structured JSON
//! `{ ok, errors[{field,message}], warnings[{field,message}] }`.

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;

#[test]
fn validate_good_current_config_ok() {
    let env = TestEnv::new();
    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "good config should exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], true);
    assert_eq!(v["errors"].as_array().unwrap().len(), 0);
    assert!(v["warnings"].is_array());
}

#[test]
fn validate_bad_icons_reports_field_error() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    cfg["ui"]["icons"] = Value::String("emoji".into());
    fs::write(&config_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(!out.status.success(), "bad ui.icons should fail closed");
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| {
            e["field"] == "ui.icons" && e["message"].as_str().unwrap_or("").contains("none")
        }),
        "expected ui.icons field error; got {errors:?}"
    );
}

#[test]
fn validate_file_flag_candidate_good() {
    let env = TestEnv::new();
    let candidate = env.tmp.path().join("candidate-good.json");
    fs::copy(env.tmp.path().join("master-plan/config.json"), &candidate).unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        candidate.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["errors"].as_array().unwrap().len(), 0);
}

#[test]
fn validate_file_flag_candidate_bad_does_not_write() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    let before = fs::read_to_string(&config_path).unwrap();

    let candidate = env.tmp.path().join("candidate-bad.json");
    let mut cfg: Value = serde_json::from_str(&before).unwrap();
    cfg["agent"]["runner"]["harness"] = Value::String("tmux".into());
    fs::write(&candidate, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        candidate.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| { e["field"] == "agent.runner.harness" }));

    let after = fs::read_to_string(&config_path).unwrap();
    assert_eq!(before, after, "validate must never write config");
}

#[test]
fn validate_unparseable_file_ok_false() {
    let env = TestEnv::new();
    let candidate = env.tmp.path().join("broken.json");
    fs::write(&candidate, "{ not json").unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        candidate.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(!v["errors"].as_array().unwrap().is_empty());
}

/// Coverage gap (M156 review): validate --file with an unknown
/// keybind action must surface a structured error against the bad field.
#[test]
fn validate_file_with_unknown_keybind_reports_field_error() {
    let env = TestEnv::new();
    let candidate = env.tmp.path().join("bad-keybind.json");
    let mut cfg: Value = serde_json::from_str(
        &fs::read_to_string(env.tmp.path().join("master-plan/config.json")).unwrap(),
    )
    .unwrap();
    cfg["keybinds"]["unknown_action"] = Value::String("x".into());
    fs::write(&candidate, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        candidate.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("keybinds.unknown_action")),
        "expected keybinds.unknown_action error; got {:?}",
        v["errors"]
    );
}

/// Coverage gap (M156 review): validate --file on a directory must
/// surface a structured `file` error rather than crashing.
#[test]
fn validate_file_on_directory_returns_field_error() {
    let env = TestEnv::new();
    let dir_path = env.tmp.path().join("is-a-dir");
    fs::create_dir(&dir_path).unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        dir_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("file")),
        "expected field=file error; got {:?}",
        v["errors"]
    );
}

/// Coverage gap (M156 review): validate (no --file) on a corrupt
/// current config must surface a structured `config` error rather
/// than silently falling back to defaults.
#[test]
fn validate_current_corrupt_config_returns_field_error() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    fs::write(&config_path, "{ not json").unwrap();

    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("config")),
        "expected field=config error; got {:?}",
        v["errors"]
    );
}

/// Coverage gap (M156 review): warnings field surfaces a structured
/// warning for empty workflow.profile (semantic gate now populates it).
#[test]
fn validate_empty_profile_emits_warning() {
    let env = TestEnv::new();
    let config_path = env.tmp.path().join("master-plan/config.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    cfg["workflow"]["profile"] = Value::String(String::new());
    fs::write(&config_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "empty profile is a warning, not an error"
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(
        v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["field"].as_str() == Some("workflow.profile")),
        "expected workflow.profile warning; got {:?}",
        v["warnings"]
    );
}
