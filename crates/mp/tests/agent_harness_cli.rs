//! M151 S2 / AC-02 — `mp agent harness list` and
//! `mp agent harness start-command <name>` CLI integration tests.
//!
//! Both subcommands emit JSON by default; the shape underpins
//! humans (`raul`-style readers) and agents (JSON parsers).
//! `start-command` errors out non-zero on unknown harnesses and
//! pins the install-hint message so the registry surface and the
//! `mp watch` precondition gate cannot diverge.

mod common;

use common::TestEnv;
use serde_json::{json, Value};

fn run_harness_list(env: &TestEnv) -> Value {
    let out = env.run(&["agent", "harness", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "harness list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("harness list: parse JSON")
}

#[test]
fn harness_list_emits_three_v1_entries() {
    let env = TestEnv::new();
    let report = run_harness_list(&env);
    let harnesses = report["harnesses"]
        .as_array()
        .expect("harnesses must be an array");
    assert_eq!(
        harnesses.len(),
        3,
        "v1 registry has exactly 3 entries (opencode/pi/cursor)"
    );

    let ids: Vec<&str> = harnesses
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["opencode", "pi", "cursor"]);
}

#[test]
fn harness_list_entries_carry_command_and_flag_metadata() {
    let env = TestEnv::new();
    let report = run_harness_list(&env);
    let harnesses = report["harnesses"].as_array().unwrap();

    // Spot-check the structural fields. The exact display names are
    // not part of the contract — only that some human-readable
    // string is present, and that model_flag/thinking_flag exist
    // (so consumers can branch on support).
    for entry in harnesses {
        assert!(entry["id"].is_string(), "{entry:?}");
        assert!(entry["display_name"].is_string(), "{entry:?}");
        assert!(entry["command"].is_string(), "{entry:?}");
        assert!(
            entry["model_flag"].is_null() || entry["model_flag"].is_string(),
            "model_flag must be a string or null: {entry:?}"
        );
        assert!(
            entry["thinking_flag"].is_null() || entry["thinking_flag"].is_string(),
            "thinking_flag must be a string or null: {entry:?}"
        );
    }

    let cursor = harnesses
        .iter()
        .find(|h| h["id"] == "cursor")
        .expect("cursor in registry");
    assert_eq!(cursor["model_flag"], json!("--model"));
    assert_eq!(cursor["thinking_flag"], json!("--thinking"));
}

#[test]
fn harness_list_works_in_a_fresh_init() {
    // Init the env first so MP_HOME/etc. are consistent; the list
    // command should work the same on day-zero checkouts (no
    // project bootstrap required).
    let env = TestEnv::new();
    let report = run_harness_list(&env);
    assert_eq!(report["harnesses"].as_array().unwrap().len(), 3);
}

#[test]
fn start_command_opencode_prints_base_argv() {
    let env = TestEnv::new();
    let out = env.run(&[
        "agent",
        "harness",
        "start-command",
        "opencode",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "start-command opencode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], "opencode");
    assert_eq!(v["argv"], json!(["opencode"]));
}

#[test]
fn start_command_appends_model_flag_when_overridden() {
    let env = TestEnv::new();
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
    assert!(
        out.status.success(),
        "start-command --model opencode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["argv"], json!(["opencode", "--model", "claude-opus-4"]));
    assert_eq!(v["model"], "claude-opus-4");
}

#[test]
fn start_command_cursor_emits_both_flags_when_supplied() {
    let env = TestEnv::new();
    let out = env.run(&[
        "agent",
        "harness",
        "start-command",
        "cursor",
        "--model",
        "claude-opus-4",
        "--thinking-level",
        "high",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "start-command cursor failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["argv"],
        json!(["cursor", "--model", "claude-opus-4", "--thinking", "high"])
    );
    assert_eq!(v["thinking_level"], "high");
}

#[test]
fn start_command_pi_skips_thinking_even_when_caller_sets_it() {
    // Pi's v1 entry has thinking_flag = None. The registry is
    // caller-driven: even if the caller passes --thinking-level,
    // the harness-side `None` means no flag is appended.
    let env = TestEnv::new();
    let out = env.run(&[
        "agent",
        "harness",
        "start-command",
        "pi",
        "--model",
        "claude-opus-4",
        "--thinking-level",
        "high",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "start-command pi failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["argv"], json!(["pi", "--model", "claude-opus-4"]));
    // The override is still echoed in the report — callers can
    // see what they passed, even when the registry dropped it.
    assert_eq!(v["thinking_level"], "high");
}

#[test]
fn start_command_unknown_harness_exits_non_zero_with_install_hint() {
    let env = TestEnv::new();
    let out = env.run(&[
        "agent",
        "harness",
        "start-command",
        "claude-code",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "unknown harness must exit non-zero");
    // The structured error is on stderr (anyhow path) — find the
    // install hint somewhere in either stream so we tolerate
    // either routing.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("claude-code"),
        "error must name the offender: {combined}"
    );
    assert!(
        combined.contains("opencode") && combined.contains("pi") && combined.contains("cursor"),
        "error must list supported harnesses: {combined}"
    );
    assert!(
        combined.contains("herdr integration install claude-code"),
        "error must suggest the on-ramp install command: {combined}"
    );
}

#[test]
fn harness_list_emits_json_by_default() {
    // Pin: the global --format flag is opt-in; default is JSON.
    // This is part of the AC-02 contract.
    let env = TestEnv::new();
    let out = env.run(&["agent", "harness", "list"]);
    assert!(
        out.status.success(),
        "harness list default-format call failed"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Must look like JSON: a `{` at the start (object envelope).
    assert!(
        stdout.trim_start().starts_with('{'),
        "default output must be JSON, got: {stdout}"
    );
}

#[test]
fn start_command_missing_name_argument_errors() {
    // `mp agent harness start-command` with no NAME fails
    // parsing — clap surfaces a non-zero exit before our code
    // runs. The point is to keep the surface narrow: NAME is
    // positional and required.
    let env = TestEnv::new();
    let out = env.run(&["agent", "harness", "start-command"]);
    assert!(
        !out.status.success(),
        "start-command without NAME must fail"
    );
}
