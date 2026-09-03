//! M209 / AC-03: `mp autopilot config get/set autopilot.<key>` is
//! the canonical read/write surface for the autopilot section.
//! Equivalent to `mp config get/set autopilot.<key>` — both surfaces
//! must round-trip and validate against the same rules.
//!
//! Pins:
//! - `mp autopilot config get autopilot.topology` returns the
//!   current value (or default).
//! - `mp autopilot config set autopilot.topology three-agent` writes
//!   and reads back.
//! - Unknown topology / unknown role / unknown role-field surface as
//!   structured validation errors.
//! - `extras` is a JSON-object typed shape (no string writeable).
//! - Per-role writes land under `autopilot.roles.<role>.<field>`.

mod common;

use crate::common::TestEnv;
use serde_json::{json, Value};

fn get(env: &TestEnv, key: &str) -> Value {
    env.run_json(&["autopilot", "config", "get", key, "--format", "json"])["value"].clone()
}

fn set(env: &TestEnv, key: &str, value: &str) {
    let out = env.run(&["autopilot", "config", "set", key, value]);
    assert!(
        out.status.success(),
        "set {key}={value} failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn topology_default_is_three_agent() {
    // Initial config carries no `autopilot.topology` -> the getter
    // surfaces the documented default.
    let env = TestEnv::new();
    assert_eq!(get(&env, "autopilot.topology"), json!("three-agent"));
}

#[test]
fn topology_set_accepts_documented_values() {
    for value in ["one-agent", "two-agent", "three-agent"] {
        let env = TestEnv::new();
        set(&env, "autopilot.topology", value);
        assert_eq!(
            get(&env, "autopilot.topology"),
            json!(value),
            "value={value}"
        );
    }
}

#[test]
fn topology_set_rejects_unknown_value() {
    let env = TestEnv::new();
    let out = env.run(&[
        "autopilot",
        "config",
        "set",
        "autopilot.topology",
        "four-agent",
    ]);
    assert!(
        !out.status.success(),
        "four-agent must be rejected; got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("autopilot.topology") && combined.contains("one-agent"),
        "error must name the key + a valid value: {combined}"
    );
}

#[test]
fn role_default_is_unset_for_fresh_project() {
    // `autopilot.roles.<role>.<field>` defaults to `null` (not set)
    // for every role + field combination.
    let env = TestEnv::new();
    for role in ["orchestrator", "runner", "reviewer"] {
        for field in ["model", "harness", "skill"] {
            let key = format!("autopilot.roles.{role}.{field}");
            assert_eq!(get(&env, &key), Value::Null, "{key}");
        }
    }
}

#[test]
fn role_field_roundtrips_for_every_role() {
    // The AC pins `mp autopilot config get autopilot.roles.runner.harness`.
    // Cover all three roles + all three scalar fields so a missing
    // get/set path fails loudly here.
    let env = TestEnv::new();
    for role in ["orchestrator", "runner", "reviewer"] {
        for field in ["model", "harness", "skill"] {
            let key = format!("autopilot.roles.{role}.{field}");
            set(&env, &key, "anthropic/claude-opus-4-1");
            assert_eq!(
                get(&env, &key),
                json!("anthropic/claude-opus-4-1"),
                "role={role} field={field}"
            );
        }
    }
}

#[test]
fn role_set_rejects_unknown_role() {
    let env = TestEnv::new();
    let out = env.run(&[
        "autopilot",
        "config",
        "set",
        "autopilot.roles.planner.harness",
        "opencode",
    ]);
    assert!(
        !out.status.success(),
        "unknown role must be rejected; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("planner"),
        "error must echo the bad role: {combined}"
    );
}

#[test]
fn role_set_rejects_unknown_field() {
    let env = TestEnv::new();
    let out = env.run(&[
        "autopilot",
        "config",
        "set",
        "autopilot.roles.runner.unknown_field",
        "x",
    ]);
    assert!(
        !out.status.success(),
        "unknown role field must be rejected; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn refresh_secs_roundtrips() {
    let env = TestEnv::new();
    set(&env, "autopilot.refresh_secs", "30");
    assert_eq!(get(&env, "autopilot.refresh_secs"), json!(30));
}

#[test]
fn refresh_secs_rejects_negative_value() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "config", "set", "autopilot.refresh_secs", "-1"]);
    assert!(
        !out.status.success(),
        "negative refresh_secs must be rejected"
    );
}

#[test]
fn mp_config_get_set_with_autopilot_prefix_also_round_trips() {
    // The `mp autopilot config` surface is a shortcut for the
    // umbrella `mp config get/set autopilot.<key>`. Both must
    // agree on the read/write semantics.
    let env = TestEnv::new();
    set(&env, "autopilot.topology", "two-agent");

    let via_mp_config = env.run_json(&["config", "get", "autopilot.topology", "--format", "json"]);
    assert_eq!(via_mp_config["value"], json!("two-agent"));

    let out = env.run(&["config", "set", "autopilot.topology", "one-agent"]);
    assert!(out.status.success());
    assert_eq!(get(&env, "autopilot.topology"), json!("one-agent"));
}

#[test]
fn autopilot_config_get_accepts_unprefixed_key() {
    // The user can type either `autopilot.topology` or `topology` —
    // the dedicated surface normalizes so they don't have to type
    // the prefix twice.
    let env = TestEnv::new();
    let out = env.run_json(&["autopilot", "config", "get", "topology", "--format", "json"]);
    assert_eq!(out["key"], json!("autopilot.topology"));
    assert_eq!(out["value"], json!("three-agent"));
}

#[test]
fn autopilot_config_set_with_unprefixed_key_writes_through() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "config", "set", "topology", "two-agent"]);
    assert!(out.status.success());
    assert_eq!(get(&env, "autopilot.topology"), json!("two-agent"));
}

#[test]
fn autopilot_config_set_unknown_topology_key_returns_structured_error() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "config", "set", "autopilot.bogus", "x"]);
    assert!(
        !out.status.success(),
        "unknown autopilot key must be rejected"
    );
}
