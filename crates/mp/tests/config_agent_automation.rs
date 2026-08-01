//! M147 AC-01 / AC-02: `[agent.automation]` section, defaults, and
//! enum validation for `branch_strategy` and `auto_remediate`.
//!
//! Round-trips `mp config set agent.automation.commit_after_execute` and
//! asserts that invalid enum values are rejected with a structured error
//! at both `config set` and `config validate` time.

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::fs;

fn config_path(env: &TestEnv) -> std::path::PathBuf {
    env.tmp.path().join("master-plan/config.json")
}

/// AC-01: `mp config set agent.automation.commit_after_execute true`
/// round-trips and the on-disk config carries the four fields with
/// the document defaults (false | false | current | none).
#[test]
fn config_set_round_trip_commit_after_execute() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.commit_after_execute",
        "true",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "good value must round-trip; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["key"], "agent.automation.commit_after_execute");
    assert_eq!(v["value"], "true");
    assert_eq!(
        v["config"]["agent"]["automation"]["commit_after_execute"],
        true
    );

    // Sanity: get echoes the stored value.
    let got = env.run_json(&[
        "config",
        "get",
        "agent.automation.commit_after_execute",
        "--format",
        "json",
    ]);
    assert_eq!(got["value"], true);
}

/// AC-01 (defaults): a brand-new project carries the four documented
/// defaults — `false | false | current | none` — for
/// `agent.automation.*`. The existing fixtures (`config.full.json`)
/// set these explicitly; this test guards against drift in the
/// template.
#[test]
fn config_show_surfaces_automation_defaults() {
    let env = TestEnv::new();
    let out = env.run(&["config", "show", "--format", "json"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let auto = &v["config"]["agent"]["automation"];
    assert_eq!(auto["commit_after_execute"], false);
    assert_eq!(auto["push_after_review"], false);
    assert_eq!(auto["branch_strategy"], "current");
    assert_eq!(auto["auto_remediate"], "none");
}

/// AC-01 (`get` returns effective value, not null): the accessor
/// surfaces defaults when no override is stored, so
/// `mp config get` matches what agents see at runtime. Bool fields
/// serialize as JSON booleans; enum fields as JSON strings.
#[test]
fn config_get_returns_effective_automation_defaults() {
    let env = TestEnv::new();
    for (key, expected_json) in [
        (
            "agent.automation.commit_after_execute",
            serde_json::json!(false),
        ),
        (
            "agent.automation.push_after_review",
            serde_json::json!(false),
        ),
        (
            "agent.automation.branch_strategy",
            serde_json::json!("current"),
        ),
        ("agent.automation.auto_remediate", serde_json::json!("none")),
    ] {
        let got = env.run_json(&["config", "get", key, "--format", "json"]);
        assert_eq!(
            got["value"], expected_json,
            "{key} must surface default {expected_json}; got {got:?}"
        );
    }
}

/// AC-02: invalid `branch_strategy` is rejected at apply time with a
/// structured error that names the valid values.
#[test]
fn config_set_rejects_invalid_branch_strategy() {
    let env = TestEnv::new();
    let before = fs::read(config_path(&env)).unwrap();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.branch_strategy",
        "foo",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "invalid branch_strategy must fail");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    let field_error = errors
        .iter()
        .find(|e| e["field"] == "agent.automation.branch_strategy")
        .unwrap_or_else(|| panic!("expected branch_strategy error; got {errors:?}"));
    let msg = field_error["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("per-milestone") && msg.contains("current") && msg.contains("none"),
        "branch_strategy error must name the valid set; got {msg:?}"
    );
    assert_eq!(fs::read(config_path(&env)).unwrap(), before);
}

/// AC-02: invalid `auto_remediate` is rejected at apply time with a
/// structured error that names the valid values.
#[test]
fn config_set_rejects_invalid_auto_remediate() {
    let env = TestEnv::new();
    let before = fs::read(config_path(&env)).unwrap();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.auto_remediate",
        "panic",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "invalid auto_remediate must fail");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    let field_error = errors
        .iter()
        .find(|e| e["field"] == "agent.automation.auto_remediate")
        .unwrap_or_else(|| panic!("expected auto_remediate error; got {errors:?}"));
    let msg = field_error["message"].as_str().unwrap_or("");
    for valid in ["none", "low", "medium", "high", "all"] {
        assert!(
            msg.contains(valid),
            "auto_remediate error must name {valid}; got {msg:?}"
        );
    }
    assert_eq!(fs::read(config_path(&env)).unwrap(), before);
}

/// AC-02: invalid values are also rejected by `mp config validate`
/// (hand-edit path), so a typo made by editing `config.toml` directly
/// does not silently propagate to `raul` or the agent.
#[test]
fn config_validate_rejects_invalid_automation_enum() {
    let env = TestEnv::new();
    let mut cfg: Value = serde_json::from_slice(&fs::read(config_path(&env)).unwrap()).unwrap();
    cfg["agent"]["automation"]["branch_strategy"] = Value::String("wrong".into());
    cfg["agent"]["automation"]["auto_remediate"] = Value::String("urgent".into());
    fs::write(
        config_path(&env),
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();

    let out = env.run(&["config", "validate", "--format", "json"]);
    assert!(!out.status.success(), "invalid enums must fail validate");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors
            .iter()
            .any(|e| e["field"] == "agent.automation.branch_strategy"),
        "expected branch_strategy error; got {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e["field"] == "agent.automation.auto_remediate"),
        "expected auto_remediate error; got {errors:?}"
    );
}

/// Defense in depth: `config set --dry-run` must surface the same
/// structured error as `config set`, and must NOT write the
/// candidate file when validation fails.
#[test]
fn config_set_dry_run_rejects_invalid_branch_strategy() {
    let env = TestEnv::new();
    let before = fs::read(config_path(&env)).unwrap();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.branch_strategy",
        "foo",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["ok"], false);
    assert!(v["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["field"] == "agent.automation.branch_strategy"));
    assert_eq!(fs::read(config_path(&env)).unwrap(), before);
}

/// All four valid `branch_strategy` values are accepted by `set` and
/// round-trip — guards against an enum value being silently dropped
/// during serialization.
#[test]
fn config_set_accepts_every_branch_strategy() {
    let env = TestEnv::new();
    for v in ["per-milestone", "current", "none"] {
        let out = env.run(&[
            "config",
            "set",
            "agent.automation.branch_strategy",
            v,
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "branch_strategy={v:?} must be accepted; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// All five valid `auto_remediate` values (none, low, medium, high,
/// all) are accepted by `set` and round-trip.
#[test]
fn config_set_accepts_every_auto_remediate() {
    let env = TestEnv::new();
    for v in ["none", "low", "medium", "high", "all"] {
        let out = env.run(&[
            "config",
            "set",
            "agent.automation.auto_remediate",
            v,
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "auto_remediate={v:?} must be accepted; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Bool fields accept the standard token set: `true|1|yes` → true;
/// `false|0|no` → false. Anything else → structured error.
#[test]
fn config_set_bool_automation_field_accepts_canonical_tokens() {
    let env = TestEnv::new();
    for (token, expected) in [
        ("true", true),
        ("1", true),
        ("yes", true),
        ("false", false),
        ("0", false),
        ("no", false),
    ] {
        let out = env.run(&[
            "config",
            "set",
            "agent.automation.push_after_review",
            token,
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "{token} must be accepted; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = env.run_json(&[
            "config",
            "get",
            "agent.automation.push_after_review",
            "--format",
            "json",
        ]);
        assert_eq!(
            got["value"].as_bool(),
            Some(expected),
            "{token} must round-trip to {expected}; got {got:?}"
        );
    }
}

/// `agent.automation.bogus` is not a recognized field — `set` and
/// `get` both reject it without writing or echoing `null`.
#[test]
fn config_set_rejects_unknown_automation_field() {
    let env = TestEnv::new();
    let before = fs::read(config_path(&env)).unwrap();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.bogus",
        "true",
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
            .any(|e| e["field"] == "agent.automation.bogus"),
        "expected agent.automation.bogus error; got {v:?}"
    );
    assert_eq!(fs::read(config_path(&env)).unwrap(), before);

    // `config get` on an unknown key exits non-zero; assert on the
    // raw Output rather than `run_json` (which assumes success).
    let get = env.run(&[
        "config",
        "get",
        "agent.automation.bogus",
        "--format",
        "json",
    ]);
    assert!(
        !get.status.success(),
        "get on unknown field must fail; stderr={}",
        String::from_utf8_lossy(&get.stderr)
    );
}
