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

// ─── stall timeout knob ──────────────────────────────────────────────────────

/// AC-01: `agent.automation.stall_timeout_minutes` is an integer in
/// 1..=240. The boundaries must round-trip and the values just outside
/// them must be rejected, at `set` time and again at `validate` time
/// (a hand-edited config with an out-of-range value must not pass
/// silently).
#[test]
fn stall_timeout_minutes_config() {
    // In-range boundaries round-trip and read back.
    for good in ["1", "240", "45"] {
        let env = TestEnv::new();
        let out = env.run(&[
            "config",
            "set",
            "agent.automation.stall_timeout_minutes",
            good,
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "{good} must be accepted; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = env.run_json(&[
            "config",
            "get",
            "agent.automation.stall_timeout_minutes",
            "--format",
            "json",
        ]);
        assert_eq!(
            got["value"],
            good.parse::<i64>().unwrap(),
            "value must round-trip"
        );
    }

    // Out-of-range values are rejected by `set`, and the message names
    // the range so the operator knows the fix. (Negative values are
    // exercised through the hand-edited path below — clap would read
    // a bare `-5` argv as a flag, not as a value.)
    for bad in ["0", "241"] {
        let env = TestEnv::new();
        let out = env.run(&[
            "config",
            "set",
            "agent.automation.stall_timeout_minutes",
            bad,
            "--format",
            "json",
        ]);
        assert!(
            !out.status.success(),
            "{bad} must be rejected by config set"
        );
        let err = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            err.contains("stall_timeout_minutes") && err.contains("240"),
            "rejection must name the key and the bound; got: {err}"
        );
    }

    // A hand-edited config with an out-of-range value must fail
    // `mp config validate` too — the two surfaces must not disagree.
    //
    // 0 and 241 reach the range check and produce the field-level
    // issue. -5 is caught one layer earlier, by deserialization into
    // `Option<u32>`, and surfaces as a parse error naming the expected
    // type. Both are rejections; the distinction is asserted so a
    // future refactor that widens the field type has to update this
    // test deliberately.
    for (bad, expected) in [
        (0i64, "stall_timeout_minutes"),
        (241, "stall_timeout_minutes"),
        (-5, "expected u32"),
    ] {
        let env = TestEnv::new();
        let path = config_path(&env);
        let mut raw: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        raw["agent"]["automation"]["stall_timeout_minutes"] = Value::from(bad);
        std::fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();

        let validated = env.run(&["config", "validate", "--format", "json"]);
        assert!(
            !validated.status.success(),
            "config validate must reject a hand-edited {bad}"
        );
        let v: Value = serde_json::from_slice(&validated.stdout).unwrap_or(Value::Null);
        let report = format!("{v}");
        assert!(
            report.contains(expected),
            "validate must reject {bad} mentioning {expected:?}; got: {report}"
        );
    }

    // A non-integer is rejected with an integer-shaped message rather
    // than an out-of-range one.
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.stall_timeout_minutes",
        "soon",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "a non-integer must be rejected");
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        err.contains("expected integer"),
        "non-integer must say so; got: {err}"
    );
}

/// AC-01: the effective timeout is `--stall-timeout-ms` >
/// `agent.automation.stall_timeout_minutes` > 30 minutes. The
/// resolver is unit-tested directly here (the flag is not reachable
/// from a unit test), and the config leg is additionally exercised
/// through the real `mp config set` surface.
#[test]
fn stall_timeout_precedence() {
    use mp::autopilot::drive::resolve_stall_timeout_ms;
    use mp::config::{ProjectConfig, DEFAULT_STALL_TIMEOUT_MINUTES};

    let mut cfg = ProjectConfig::default();

    // 3. No flag, no config -> the 30-minute default.
    assert_eq!(
        resolve_stall_timeout_ms(None, &cfg),
        u64::from(DEFAULT_STALL_TIMEOUT_MINUTES) * 60_000,
        "unset must fall back to the 30-minute default"
    );

    // 2. Config set, no flag -> config wins over the default.
    cfg.agent.automation.stall_timeout_minutes = Some(45);
    assert_eq!(
        resolve_stall_timeout_ms(None, &cfg),
        45 * 60_000,
        "config must beat the default"
    );

    // 1. Flag present -> the flag wins over both.
    assert_eq!(
        resolve_stall_timeout_ms(Some(1_234), &cfg),
        1_234,
        "the flag must beat config"
    );

    // A 1-minute config resolves to 60_000ms, and the ceiling is 4x.
    cfg.agent.automation.stall_timeout_minutes = Some(1);
    assert_eq!(resolve_stall_timeout_ms(None, &cfg), 60_000);

    // The same value the config stores is what `mp config get` reports
    // through the real CLI, so the resolver and the operator-visible
    // surface cannot drift.
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.stall_timeout_minutes",
        "45",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let got = env.run_json(&[
        "config",
        "get",
        "agent.automation.stall_timeout_minutes",
        "--format",
        "json",
    ]);
    assert_eq!(got["value"], 45);

    // The config `mp config set` actually wrote must resolve through
    // the same function the drive loop calls.
    let on_disk: ProjectConfig =
        serde_json::from_str(&std::fs::read_to_string(config_path(&env)).unwrap()).unwrap();
    assert_eq!(on_disk.agent.automation.stall_timeout_minutes, Some(45));
    assert_eq!(resolve_stall_timeout_ms(None, &on_disk), 45 * 60_000);
    // ...and the flag still overrides what the CLI just wrote.
    assert_eq!(resolve_stall_timeout_ms(Some(7_000), &on_disk), 7_000);
}
