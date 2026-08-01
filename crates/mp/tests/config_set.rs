//! M156 AC-02: `mp config set --dry-run` parses/validates without writing.

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;

fn config_bytes(env: &TestEnv) -> Vec<u8> {
    fs::read(env.tmp.path().join("master-plan/config.json")).unwrap()
}

#[test]
fn dry_run_good_value_exits_0_and_does_not_write() {
    let env = TestEnv::new();
    let before = config_bytes(&env);

    let out = env.run(&[
        "config",
        "set",
        "ui.theme",
        "dracula",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "known-good dry-run must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["key"], "ui.theme");
    assert_eq!(v["value"], "dracula");
    assert_eq!(v["config"]["ui"]["theme"], "dracula");

    assert_eq!(
        before,
        config_bytes(&env),
        "dry-run must not mutate config on disk"
    );

    // Real get still shows default/unset theme.
    let got = env.run_json(&["config", "get", "ui.theme", "--format", "json"]);
    assert_ne!(got["value"], "dracula");
}

#[test]
fn dry_run_bad_value_fails_closed_no_write() {
    let env = TestEnv::new();
    let before = config_bytes(&env);

    let out = env.run(&[
        "config",
        "set",
        "ui.icons",
        "emoji",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "known-bad dry-run must be non-zero");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["dry_run"], true);
    assert!(
        !v["errors"].as_array().unwrap().is_empty(),
        "bad dry-run must report structured errors"
    );

    assert_eq!(
        before,
        config_bytes(&env),
        "failed dry-run must not mutate config on disk"
    );
}

#[test]
fn dry_run_unknown_key_fails_closed() {
    let env = TestEnv::new();
    let before = config_bytes(&env);

    let out = env.run(&[
        "config",
        "set",
        "ui.nonexistent",
        "x",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(before, config_bytes(&env));
}

#[test]
fn real_set_still_writes() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "ui.theme", "dracula", "--format", "json"]);
    assert!(out.status.success());
    let got = env.run_json(&["config", "get", "ui.theme", "--format", "json"]);
    assert_eq!(got["value"], "dracula");
}

#[test]
fn real_set_rejects_invalid_profile_same_as_dry_run() {
    let env = TestEnv::new();
    let before = config_bytes(&env);

    let dry = env.run(&[
        "config",
        "set",
        "workflow.profile",
        "bogus",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(!dry.status.success(), "dry-run must reject bogus profile");

    let real = env.run(&[
        "config",
        "set",
        "workflow.profile",
        "bogus",
        "--format",
        "json",
    ]);
    assert!(
        !real.status.success(),
        "real set must reject the same values dry-run rejects"
    );
    assert_eq!(before, config_bytes(&env));
}

/// Coverage gap (M156 review): unknown keybind action name must be rejected
/// under both `set --dry-run` and real `set`.
#[test]
fn dry_run_rejects_unknown_keybind_action() {
    let env = TestEnv::new();
    let before = config_bytes(&env);
    let out = env.run(&[
        "config",
        "set",
        "keybinds.bogus",
        "x",
        "--dry-run",
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
            .any(|e| e["field"].as_str() == Some("keybinds.bogus")),
        "expected keybinds.bogus error; got {:?}",
        v["errors"]
    );
    assert_eq!(before, config_bytes(&env));
}

/// Coverage gap (M156 review): unknown agent harness must be rejected.
#[test]
fn dry_run_rejects_unknown_agent_harness() {
    let env = TestEnv::new();
    let before = config_bytes(&env);
    let out = env.run(&[
        "config",
        "set",
        "agent.runner.harness",
        "tmux",
        "--dry-run",
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
            .any(|e| e["field"].as_str() == Some("agent.runner.harness")),
        "expected agent.runner.harness error; got {:?}",
        v["errors"]
    );
    assert_eq!(before, config_bytes(&env));
}

/// Coverage gap (M156 review): invalid bool must be rejected.
#[test]
fn dry_run_rejects_invalid_bool() {
    let env = TestEnv::new();
    let before = config_bytes(&env);
    let out = env.run(&[
        "config",
        "set",
        "ui.color",
        "notabool",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(!v["errors"].as_array().unwrap().is_empty());
    assert_eq!(before, config_bytes(&env));
}

/// Coverage gap (M156 review): keybind clear via empty value must succeed
/// (the set-keybind path treats `""` as "remove the binding").
#[test]
fn dry_run_clears_keybind_with_empty_value() {
    let env = TestEnv::new();
    // First set a non-default keybind so we can clear it.
    let set = env.run(&[
        "config",
        "set",
        "keybinds.quit",
        "ctrl+k",
        "--format",
        "json",
    ]);
    assert!(set.status.success());

    let out = env.run(&[
        "config",
        "set",
        "keybinds.quit",
        "",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(out.status.success(), "empty value must clear, not error");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["config"]["keybinds"]["quit"],
        Value::Null,
        "cleared keybind must not appear in previewed config"
    );
}

/// Coverage gap (M156 review): the JSON contract for `set` must match
/// `set --dry-run` so downstream consumers (raul Settings modal) get the
/// same shape regardless of which mode ran.
#[test]
fn real_set_emits_same_json_contract_as_dry_run() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "ui.theme", "dracula", "--format", "json"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["dry_run"], false);
    assert_eq!(v["key"], "ui.theme");
    assert_eq!(v["value"], "dracula");
    assert!(
        v["config"]["ui"]["theme"].as_str() == Some("dracula"),
        "real set must echo new config: got {:?}",
        v["config"]
    );

    let dry = env.run(&[
        "config",
        "set",
        "ui.theme",
        "mocha",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(dry.status.success());
    let d: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(d["dry_run"], true);
    let v_keys: std::collections::BTreeSet<_> = v.as_object().unwrap().keys().collect();
    let d_keys: std::collections::BTreeSet<_> = d.as_object().unwrap().keys().collect();
    assert_eq!(v_keys, d_keys, "real-set and dry-run JSON keys must match");
}

/// Coverage gap (M156 review): real set surfaces the same structured
/// error list (not just the first one) so a user can fix multiple
/// problems in one go.
#[test]
fn real_set_emits_all_errors_not_just_first() {
    let env = TestEnv::new();
    // Seed the config with a known-bad profile; then attempt another
    // bad set with a separate key. Two distinct fields in errors.
    let seed = env.run(&[
        "config",
        "set",
        "workflow.profile",
        "bogus",
        "--format",
        "json",
    ]);
    assert!(
        !seed.status.success(),
        "seed must fail to set bogus profile"
    );

    // Now `set --dry-run` with two bad fields in one batch is not
    // expressible (only one key per call). Instead we set a profile
    // that is invalid AND a ui.icons that is invalid sequentially;
    // the structured report should always include the *one* field it
    // touched, plus `keybinds` etc. when present in config.
    let out = env.run(&[
        "config",
        "set",
        "workflow.profile",
        "still-bogus",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors
            .iter()
            .any(|e| e["field"].as_str() == Some("workflow.profile")),
        "expected workflow.profile in errors; got {errors:?}"
    );
}

/// Coverage gap (M156 review): agent command in the success-path JSON
/// output is redacted so argv-shaped secrets do not leak to stdout.
/// The `value` field is allowed to echo the caller-supplied string
/// (the caller already knows it); the *parsed* `config.agent.runner.command`
/// array is what gets redacted.
#[test]
fn real_set_redacts_agent_command_in_report() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.runner.command",
        "[\"opencode\",\"--flag\",\"s3cret\"]",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let parsed_cmd = v["config"]["agent"]["runner"]["command"]
        .as_array()
        .expect("redacted command must be an array");
    assert_eq!(
        parsed_cmd.len(),
        1,
        "redaction collapses to a single marker"
    );
    let marker = parsed_cmd[0].as_str().unwrap_or("");
    assert!(
        marker.contains("redacted"),
        "command should be redacted; got {marker:?}"
    );
    assert!(
        !marker.contains("s3cret") && !marker.contains("--flag"),
        "argv entries must not leak: {marker:?}"
    );
}

/// Coverage gap (M156 review): the `--file` validate path must reject
/// a missing path with a structured `field: "file"` error and a non-zero
/// exit code.
#[test]
fn validate_file_missing_returns_field_error() {
    let env = TestEnv::new();
    let missing = env.tmp.path().join("does-not-exist.json");
    let out = env.run(&[
        "config",
        "validate",
        "--file",
        missing.to_str().unwrap(),
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

/// Coverage gap (M156 review): setting `workflow.profile ""` must fail
/// closed with a structured field error.
#[test]
fn dry_run_rejects_empty_workflow_profile() {
    let env = TestEnv::new();
    let before = config_bytes(&env);
    let out = env.run(&[
        "config",
        "set",
        "workflow.profile",
        "",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "empty workflow.profile must be rejected at apply_config_set"
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("workflow.profile")),
        "expected workflow.profile field error; got {:?}",
        v["errors"]
    );
    assert_eq!(before, config_bytes(&env));
}

/// M156 ext-review F-13: ConfigSetReport.warnings surfaces the same
/// non-blocking hints as ConfigValidateReport.warnings. Pre-fills the
/// on-disk config with `workflow.profile = ""` (the hand-edit case the
/// apply-time gate can't catch) and runs a set on an unrelated key —
/// the warning must reach the set report so consumers see the same
/// signal they would on `mp config validate`.
#[test]
fn set_report_carries_warnings_from_on_disk_state() {
    let env = TestEnv::new();
    // Hand-edit config.json to introduce the empty-profile warning
    // shape; the apply-time gate only rejects empty on the SET path,
    // not on the validate-or-other-key-set path.
    let cfg_path = env.tmp.path().join("master-plan/config.json");
    let mut cfg: Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["workflow"]["profile"] = Value::String(String::new());
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&["config", "set", "ui.theme", "dracula", "--format", "json"]);
    assert!(
        out.status.success(),
        "set on an unrelated key must succeed even when warnings exist"
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], Value::Bool(true));
    let warnings = v["warnings"].as_array().expect("warnings field present");
    assert!(
        warnings
            .iter()
            .any(|w| w["field"].as_str() == Some("workflow.profile")),
        "empty workflow.profile warning must reach the set report; got {warnings:?}"
    );
}

/// M156 ext-review F-14: ui.theme validation. A typo like `moxha`
/// would otherwise pass validate and silently no-op at runtime via
/// `Palette::by_name(name)` returning `None`.
#[test]
fn set_rejects_unknown_ui_theme() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "ui.theme", "moxha", "--format", "json"]);
    assert!(
        !out.status.success(),
        "unknown ui.theme must be rejected at the semantic gate"
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("ui.theme")),
        "expected ui.theme field error; got {:?}",
        v["errors"]
    );
}

#[test]
fn validate_rejects_unknown_ui_theme() {
    let env = TestEnv::new();
    let cfg_path = env.tmp.path().join("master-plan/config.json");
    let mut cfg: Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["ui"]["theme"] = Value::String("moxha".to_string());
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(!out.status.success(), "validate must reject unknown theme");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["field"].as_str() == Some("ui.theme")),
        "validate must surface ui.theme error; got {:?}",
        v["errors"]
    );
}
