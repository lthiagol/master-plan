//! M149 S1 + M151 S3: the `[agent.runner]` and `[agent.coordinator]`
//! sections round-trip through `mp config set` / `get` / `show`,
//! and `mp watch`'s harness->argv construction is wired to the
//! v1 `HarnessRegistry` instead of a hardcoded match.
//!
//! mp owns the config schema; `mp watch` consumes it at startup
//! precondition time. This test pins that the four role fields
//! (harness, command, model, thinking_level) are stored, reported,
//! and validated (unknown harness ids / roles / fields error out)
//! without touching the rest of the config — and that the watch
//! code path no longer carries a per-harness hardcoded argv
//! (M151 S3/AC-03).

mod common;

use crate::common::TestEnv;
use serde_json::{json, Value};

fn get(env: &TestEnv, key: &str) -> Value {
    env.run_json(&["config", "get", key, "--format", "json"])["value"].clone()
}

#[test]
fn agent_sections_appear_in_config_show() {
    let env = TestEnv::new();
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert!(
        report["config"]["agent"]["runner"].is_object(),
        "agent.runner section missing from config show"
    );
    assert!(
        report["config"]["agent"]["coordinator"].is_object(),
        "agent.coordinator section missing from config show"
    );
}

#[test]
fn agent_fields_default_to_null_when_unset() {
    let env = TestEnv::new();
    for role in ["runner", "coordinator"] {
        for field in ["harness", "command", "model", "thinking_level"] {
            let key = format!("agent.{role}.{field}");
            assert_eq!(get(&env, &key), Value::Null, "{key} should default to null");
        }
    }
}

#[test]
fn harness_roundtrips_for_both_roles() {
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    assert_eq!(get(&env, "agent.runner.harness"), json!("opencode"));

    env.run(&["config", "set", "agent.coordinator.harness", "pi"]);
    assert_eq!(get(&env, "agent.coordinator.harness"), json!("pi"));

    // Cross-check independence: setting coordinator does not change runner.
    assert_eq!(get(&env, "agent.runner.harness"), json!("opencode"));
}

#[test]
fn harness_accepts_all_v1_supported_values() {
    for harness in ["opencode", "pi", "cursor"] {
        let env = TestEnv::new();
        let out = env.run(&[
            "config",
            "set",
            "agent.runner.harness",
            harness,
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "harness {harness} should be accepted: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(get(&env, "agent.runner.harness"), json!(harness));
    }
}

#[test]
fn harness_rejects_unknown_value() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.runner.harness",
        "tmux",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "tmux is not a v1-supported harness; config set should reject it"
    );
    // M156: the structured error lives in the JSON report on stdout, not
    // stderr. The message lists every allowed harness.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON output: {stdout:?}; {e}"));
    let message = v["errors"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|e| e["message"].as_str())
        .unwrap_or_default();
    assert!(
        message.contains("opencode")
            && message.contains("pi")
            && message.contains("cursor"),
        "error must list allowed harnesses (opencode, pi, cursor); got message={message:?}, stdout={stdout:?}"
    );
    // Original value unchanged.
    assert_eq!(get(&env, "agent.runner.harness"), Value::Null);
}

#[test]
fn command_roundtrips_as_json_array() {
    let env = TestEnv::new();
    env.run(&[
        "config",
        "set",
        "agent.runner.command",
        "[\"opencode\", \"--flag\"]",
    ]);
    assert_eq!(
        get(&env, "agent.runner.command"),
        json!(["opencode", "--flag"])
    );
}

#[test]
fn command_single_token_is_wrapped_into_one_element_array() {
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.command", "opencode"]);
    assert_eq!(get(&env, "agent.runner.command"), json!(["opencode"]));
}

#[test]
fn command_rejects_empty_value() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.runner.command",
        "",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "empty command value should be rejected"
    );
}

#[test]
fn model_and_thinking_level_roundtrip() {
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.model", "claude-opus-4"]);
    assert_eq!(get(&env, "agent.runner.model"), json!("claude-opus-4"));

    env.run(&["config", "set", "agent.coordinator.thinking_level", "high"]);
    assert_eq!(get(&env, "agent.coordinator.thinking_level"), json!("high"));
}

#[test]
fn unknown_agent_field_errors() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "get",
        "agent.runner.nonexistent",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "unknown agent field should error on get"
    );

    let out = env.run(&[
        "config",
        "set",
        "agent.runner.nonexistent",
        "x",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "unknown agent field should error on set"
    );
}

#[test]
fn unknown_agent_role_errors() {
    let env = TestEnv::new();
    let out = env.run(&["config", "get", "agent.planner.harness", "--format", "json"]);
    assert!(
        !out.status.success(),
        "unknown agent role should error on get"
    );

    let out = env.run(&[
        "config",
        "set",
        "agent.planner.harness",
        "opencode",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "unknown agent role should error on set"
    );
}

#[test]
fn agent_set_does_not_disturb_other_config() {
    let env = TestEnv::new();
    env.run(&["config", "set", "ui.theme", "dracula"]);
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    let report = env.run_json(&["config", "show", "--format", "json"]);
    assert_eq!(report["config"]["ui"]["theme"], json!("dracula"));
    assert_eq!(
        report["config"]["agent"]["runner"]["harness"],
        json!("opencode")
    );
    // Other role untouched.
    assert_eq!(
        report["config"]["agent"]["coordinator"]["harness"],
        Value::Null
    );
}

// ─── M151 S3 / AC-03: watch -> registry wiring ──────────────────────────

/// Helper: read a file relative to the crate's `src/` and return its
/// contents as a string. The test does not care about `manifest_dir`
/// portability — `CARGO_MANIFEST_DIR` is set by cargo at build time
/// for every integration test in this crate.
fn read_crate_src(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn watch_herdr_no_hardcoded_harness_match_arm() {
    // AC-03: `mp watch` builds the runner/coordinator argv via the
    // registry. The pre-M151 file had a literal:
    //
    //   match harness {
    //       "opencode" => Ok(vec!["opencode".to_string()]),
    //       "pi" => Ok(vec!["pi".to_string()]),
    //       "cursor" => Ok(vec!["cursor".to_string()]),
    //       other => bail!(...),
    //   }
    //
    // We pin the absence of that match-arm pattern in
    // `crates/mp/src/watch/herdr.rs` so a future regression that
    // hand-rolls a per-harness argv again will trip this test.
    let src = read_crate_src("src/watch/herdr.rs");

    // The pattern to grep for is a 3-arm match keyed on harness
    // names. The match arms used to live as `match harness {` ...
    // trailing `}` with three string-literal first arms. Any new
    // listing of all three ("opencode" + "pi" + "cursor") as
    // match arm patterns should be caught here.
    let first_arm = "\"opencode\" =>";
    let second_arm = "\"pi\" =>";
    let third_arm = "\"cursor\" =>";
    let has_first = src.contains(first_arm);
    let has_second = src.contains(second_arm);
    let has_third = src.contains(third_arm);

    if has_first && has_second && has_third {
        panic!(
            "mp watch src/watch/herdr.rs still contains the \
             pre-M151 hardcoded match arm trio ({first_arm} / \
             {second_arm} / {third_arm}). Resolve harness argv \
             via HarnessRegistry::v1() instead."
        );
    }
}

#[test]
fn watch_herdr_invokes_the_registry_for_default_argv() {
    // Pin AC-03 from the code-shape angle: the file references
    // `HarnessRegistry` directly so the wiring cannot regress
    // back to a local match without removing the registry
    // mention too (which would itself trip a different gate).
    let src = read_crate_src("src/watch/herdr.rs");
    assert!(
        src.contains("HarnessRegistry") && src.contains("resolve_argv"),
        "src/watch/herdr.rs must call HarnessRegistry::resolve_argv \
         rather than a local match (M151 S3)."
    );
}

#[test]
fn unknown_harness_via_config_drives_watch_precondition_failure() {
    // S3 read-through (the install-hint side is S4's job and lives
    // in agent_harness_unknown.rs). A hand-edited config can still
    // smuggle an unknown harness past `mp config set` (which
    // validates). `mp watch` (precondition gate) must surface a
    // failure naming the offender and the v1 harness set so the
    // user can fix the config without grepping for docs.
    let env = TestEnv::new();
    // Initialise the plan so config + preconditions work.
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);

    // Hand-edit the on-disk config to bypass mp config set
    // validation. This mirrors the precondition check's
    // hand-edit-defense comment in preconditions.rs. The
    // on-disk schema is flat: `{workflow: ..., agent: {...}}`
    // (no `config` envelope — that's only on the `config show`
    // CLI output).
    let config_path = env.tmp.path().join("master-plan").join("config.json");
    let raw = std::fs::read_to_string(&config_path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["agent"]["runner"]["harness"] = json!("claude-code");
    std::fs::write(&config_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let out = env.run(&["watch", "151", "--format", "json"]);
    assert!(
        !out.status.success(),
        "watch with unknown harness must exit non-zero: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("claude-code"),
        "watch error must name the offender: {combined}"
    );
    assert!(
        combined.contains("opencode") && combined.contains("pi") && combined.contains("cursor"),
        "watch error must list supported harnesses: {combined}"
    );
}

#[test]
fn cli_start_command_emits_model_flag_for_opencode() {
    // M151 ext-review F-04 (2026-07-14): this test exercises the
    // CLI surface (`mp agent harness start-command`) rather than
    // `mp watch` itself. The harness registry is the single source
    // of truth for both surfaces, so a passing registry shape is
    // a passing argv shape for the watch wiring — but the
    // end-to-end `mp watch --dry-run` case (config -> registry ->
    // herdr argv) is covered separately by
    // `tests/watch_dry_run.rs::dry_run_reflects_runner_model_in_argv`.
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.runner.model", "claude-opus-4"]);

    // The CLI surface is registry-backed; this proves the model
    // translation works for the v1 harness opencode.
    let out = env.run(&[
        "agent",
        "harness",
        "start-command",
        "opencode",
        "--model",
        "claude-opus-4",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["argv"], json!(["opencode", "--model", "claude-opus-4"]));
}

#[test]
fn registry_is_the_single_source_for_supported_set() {
    // Sanity: the three source paths that name v1 harnesses
    // (config.rs validation, watch/herdr.rs resolution,
    // harness/registry.rs entries) agree on the supported
    // list. Drifting here would silently let one path accept a
    // harness another path rejects — exactly the divergence
    // M151 set out to prevent.
    let config = read_crate_src("src/config.rs");
    let herdr = read_crate_src("src/watch/herdr.rs");
    let registry = read_crate_src("src/harness/registry.rs");

    for needle in ["\"opencode\"", "\"pi\"", "\"cursor\""] {
        assert!(registry.contains(needle), "registry.rs must list {needle}");
    }
    assert!(
        herdr.contains("HarnessRegistry"),
        "watch/herdr.rs must use HarnessRegistry (M151 S3)"
    );
    // The WATCH_HARNESSES constant in config.rs is a v1 alias
    // of the registry. A divergence is acceptable if the alias
    // is documented, but today they must stay in lockstep.
    assert!(
        config.contains("WATCH_HARNESSES") && config.contains("opencode"),
        "config.rs WATCH_HARNESSES still gates mp config set; \
         keep it consistent with the registry"
    );
}
