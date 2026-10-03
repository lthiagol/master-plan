//! M200: `keybinds.refresh` / `keybinds.previous_lane` / `keybinds.focus_content`
//! — the three get-cases that pin the keybind deconflict on the
//! `mp config get` read path.
//!
//! AC-05 acceptance: on a fresh project, `config get` returns the
//! canonical default for unset actions. `focus_content` (now a TUI-internal
//! reserved action) errors out with unknown-key.

use crate::common::TestEnv;
use serde_json::{json, Value};

fn get(env: &TestEnv, key: &str) -> Value {
    env.run_json(&["config", "get", key, "--format", "json"])["value"].clone()
}

#[test]
fn config_keys_get_refresh_default() {
    let env = TestEnv::new();
    // M200: refresh default moves from `r` to `Ctrl-R` so it no longer
    // collides with `keybinds.resolve`. `mp config get keybinds.refresh`
    // on a fresh project returns the canonical default string.
    assert_eq!(get(&env, "keybinds.refresh"), json!("Ctrl-R"));
}

#[test]
fn config_keys_get_previous_lane_default() {
    let env = TestEnv::new();
    // M200: dropped the `h` alias (vim-style conflict with `hide_done`).
    // Only `Left` and `BackTab` remain.
    assert_eq!(get(&env, "keybinds.previous_lane"), json!("Left, BackTab"));
}

#[test]
fn config_keys_get_focus_content_errors_with_unknown_key() {
    let env = TestEnv::new();
    // M200: `focus_content` is no longer user-rebindable — the action is
    // a TUI-internal reserved action. `mp config get` must surface the
    // same rejection as any other unknown keybind action.
    let out = env.run(&[
        "config",
        "get",
        "keybinds.focus_content",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "config get keybinds.focus_content should error; stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn config_keys_set_focus_content_errors_with_deprecation_text() {
    let env = TestEnv::new();
    // M200: `mp config set keybinds.focus_content ...` is rejected (exit 1)
    // with deprecation text that names the field and points at CHANGELOG.md.
    // No milestone IDs in user-visible text. The deprecation message
    // lives in the structured JSON report (`errors[0].message`) per the
    // `emit_and_exit_on_fail` contract — not in stderr.
    let out = env.run(&[
        "config",
        "set",
        "keybinds.focus_content",
        "Enter",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "config set keybinds.focus_content should error"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("config set report is JSON");
    let errors = report["errors"]
        .as_array()
        .expect("errors is an array in the report");
    assert!(
        !errors.is_empty(),
        "report should carry at least one error; got: {report}"
    );
    let message = errors[0]["message"]
        .as_str()
        .expect("error message is a string");
    assert!(
        message.contains("focus_content"),
        "error message should name the deprecated field; got: {message}"
    );
    assert!(
        message.contains("CHANGELOG"),
        "error message should point at CHANGELOG.md; got: {message}"
    );
    assert!(
        !message.contains("M200") && !message.contains("M-200") && !message.contains("M 200"),
        "error message must not contain milestone IDs; got: {message}"
    );
}

#[test]
fn config_keys_validate_stale_focus_content_emits_deprecation_warning() {
    // M200: a stale `[keybinds] focus_content = ...` line in a project
    // config must surface as a non-blocking deprecation warning on
    // `mp config validate`. Exit code 0; the warning names the field
    // and points at CHANGELOG.md with no milestone IDs.
    let env = TestEnv::new();
    let candidate = env.tmp.path().join("stale-focus-content.json");
    let mut cfg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(env.tmp.path().join("master-plan/config.json")).unwrap(),
    )
    .unwrap();
    cfg["keybinds"]["focus_content"] = serde_json::json!("Enter");
    std::fs::write(&candidate, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run(&[
        "config",
        "validate",
        "--file",
        candidate.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "stale focus_content line should be a warning, not an error; \
         stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("validate report is JSON");
    let warnings = report["warnings"].as_array().expect("warnings is an array");
    assert!(
        warnings.iter().any(|w| {
            w["field"].as_str() == Some("keybinds.focus_content")
                && w["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("focus_content"))
        }),
        "expected a focus_content deprecation warning; got: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .all(|w| { !w["message"].as_str().unwrap_or("").contains("M200") }),
        "warnings must not contain milestone IDs; got: {warnings:?}"
    );
}

#[test]
fn config_keys_validate_clean_config_emits_no_focus_content_warning() {
    let env = TestEnv::new();
    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "clean config should validate clean; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("validate report is JSON");
    let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings
            .iter()
            .all(|w| { w["field"].as_str() != Some("keybinds.focus_content") }),
        "clean config should not emit focus_content warning; got: {warnings:?}"
    );
}

#[test]
fn config_keys_set_other_keybind_strips_stale_focus_content_line() {
    // M200: self-heal — when the user sets any other keybind, a stale
    // `keybinds.focus_content` line is silently dropped. Existing
    // overrides for the keybind the user just set remain intact.
    let env = TestEnv::new();

    // Seed a stale focus_content line by hand (the key was removed from
    // KEYBIND_ACTIONS, so `set` rejects it — we need to write the file
    // directly to model a project that pre-dates M200).
    let config_path = env.tmp.path().join("master-plan/config.json");
    let minimal = serde_json::json!({
        "ui": {},
        "workflow": {},
        "git": {},
        "next": {},
        "agent": {},
        "sort": {},
        "review": {},
        "keybinds": {"focus_content": "Enter"}
    });
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&minimal).unwrap(),
    )
    .expect("seed config");

    // Set an unrelated keybind — must succeed and drop the stale line.
    let out = env.run(&["config", "set", "keybinds.quit", "ctrl+c"]);
    assert!(
        out.status.success(),
        "config set keybinds.quit should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let report = env.run_json(&["config", "show", "--format", "json"]);
    let keybinds = &report["config"]["keybinds"];
    assert_eq!(
        keybinds.get("quit"),
        Some(&json!("ctrl+c")),
        "newly-set keybind should be present"
    );
    assert!(
        keybinds.get("focus_content").is_none(),
        "stale focus_content line should be self-healed away; got: {keybinds}"
    );
}

// ---------------------------------------------------------------------
// Autopilot UI preference keys (`ui.autopilot.*`)
// ---------------------------------------------------------------------
//
// These three keys back the Autopilot tab's layout preferences: the
// left/right split width, the active sidebar tab, and whether the
// sidebar is visible at all. The contract is "additive + validated":
// unset means "use the documented default" and nothing is written to
// disk until the operator (or raul, on a drag) sets a value; a bad
// value is a clean `config set` error rather than a read-time panic.

/// `config set` on a `ui.autopilot.*` key must exit 0 and the value
/// must round-trip through `config get`.
fn set_ok(env: &TestEnv, key: &str, value: &str) {
    let out = env.run(&["config", "set", key, value, "--format", "json"]);
    assert!(
        out.status.success(),
        "config set {key} {value} should succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// `config set` on a `ui.autopilot.*` key must exit non-zero and leave
/// the stored value untouched.
fn set_rejected(env: &TestEnv, key: &str, value: &str) {
    let out = env.run(&["config", "set", key, value, "--format", "json"]);
    assert!(
        !out.status.success(),
        "config set {key} {value} should be rejected"
    );
    let report: Value = serde_json::from_slice(&out.stdout).expect("config set report is JSON");
    let errors = report["errors"].as_array().expect("errors is an array");
    assert!(
        !errors.is_empty(),
        "rejected set should carry an error; got: {report}"
    );
}

#[test]
fn ui_autopilot_defaults_on_fresh_project() {
    let env = TestEnv::new();
    // A project that has never opened the Autopilot tab reads the
    // documented defaults: 40/60 split, Progress tab, sidebar shown.
    assert_eq!(get(&env, "ui.autopilot.split_pct"), json!(40));
    assert_eq!(get(&env, "ui.autopilot.sidebar_tab"), json!("progress"));
    assert_eq!(get(&env, "ui.autopilot.sidebar_visible"), json!(true));
}

#[test]
fn ui_autopilot_keys_are_absent_from_disk_until_set() {
    let env = TestEnv::new();
    // Additive contract: reading a default must not materialize the
    // `ui.autopilot` object in config.json.
    for key in [
        "ui.autopilot.split_pct",
        "ui.autopilot.sidebar_tab",
        "ui.autopilot.sidebar_visible",
    ] {
        let _ = get(&env, key);
    }
    let raw = std::fs::read_to_string(env.tmp.path().join("master-plan/config.json"))
        .expect("read config.json");
    let cfg: Value = serde_json::from_str(&raw).expect("config.json is JSON");
    assert!(
        cfg["ui"].get("autopilot").is_none(),
        "reading defaults must not write ui.autopilot; got: {}",
        cfg["ui"]
    );
}

#[test]
fn ui_autopilot_split_pct_round_trips() {
    let env = TestEnv::new();
    // Both clamp bounds are legal values, not just the midpoint.
    for pct in ["25", "55", "75"] {
        set_ok(&env, "ui.autopilot.split_pct", pct);
        assert_eq!(
            get(&env, "ui.autopilot.split_pct"),
            json!(pct.parse::<i64>().unwrap()),
            "split_pct {pct} should round-trip"
        );
    }
}

#[test]
fn ui_autopilot_split_pct_rejects_out_of_range() {
    let env = TestEnv::new();
    set_ok(&env, "ui.autopilot.split_pct", "40");
    // `-10` is deliberately absent: clap rejects a leading-dash token as
    // an unknown flag before the value ever reaches the validator, so it
    // exercises the arg parser rather than the range check.
    for bad in ["0", "24", "76", "100", "abc", "40.5"] {
        set_rejected(&env, "ui.autopilot.split_pct", bad);
    }
    // A rejected set must not corrupt the stored value.
    assert_eq!(get(&env, "ui.autopilot.split_pct"), json!(40));
}

#[test]
fn ui_autopilot_sidebar_tab_round_trips() {
    let env = TestEnv::new();
    for tab in ["progress", "activity", "state"] {
        set_ok(&env, "ui.autopilot.sidebar_tab", tab);
        assert_eq!(get(&env, "ui.autopilot.sidebar_tab"), json!(tab));
    }
}

#[test]
fn ui_autopilot_sidebar_tab_rejects_unknown_enum() {
    let env = TestEnv::new();
    set_ok(&env, "ui.autopilot.sidebar_tab", "activity");
    for bad in ["Progress", "logs", "", "state "] {
        set_rejected(&env, "ui.autopilot.sidebar_tab", bad);
    }
    assert_eq!(get(&env, "ui.autopilot.sidebar_tab"), json!("activity"));
}

#[test]
fn ui_autopilot_sidebar_visible_round_trips() {
    let env = TestEnv::new();
    for value in ["false", "true", "0", "1"] {
        set_ok(&env, "ui.autopilot.sidebar_visible", value);
    }
    assert_eq!(get(&env, "ui.autopilot.sidebar_visible"), json!(true));
    set_rejected(&env, "ui.autopilot.sidebar_visible", "maybe");
}

#[test]
fn ui_autopilot_keys_persist_to_config_json() {
    let env = TestEnv::new();
    // raul writes preferences by shelling out to `mp config set`; the
    // values must land in config.json under `ui.autopilot` so the next
    // raul launch reads them back.
    set_ok(&env, "ui.autopilot.split_pct", "65");
    set_ok(&env, "ui.autopilot.sidebar_tab", "state");
    set_ok(&env, "ui.autopilot.sidebar_visible", "false");
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert_eq!(report["config"]["ui"]["autopilot"]["split_pct"], json!(65));
    assert_eq!(
        report["config"]["ui"]["autopilot"]["sidebar_tab"],
        json!("state")
    );
    assert_eq!(
        report["config"]["ui"]["autopilot"]["sidebar_visible"],
        json!(false)
    );
}

#[test]
fn ui_autopilot_keys_appear_in_config_schema() {
    // The docs schema is what `mp config list` / the settings editor
    // read; a key that validates but is undocumented is invisible to
    // users.
    let env = TestEnv::new();
    let report = env.run_json(&["config", "schema", "--format", "json"]);
    let keys: Vec<&str> = report["keys"]
        .as_array()
        .expect("keys is an array")
        .iter()
        .filter_map(|k| k["key"].as_str())
        .collect();
    for expected in [
        "ui.autopilot.split_pct",
        "ui.autopilot.sidebar_tab",
        "ui.autopilot.sidebar_visible",
    ] {
        assert!(
            keys.contains(&expected),
            "{expected} should be listed in the config schema; got: {keys:?}"
        );
    }
}
