//! M233 / AC-06: negative integration tests driving the `?`-propagated
//! error paths in `crates/mp/src/config_cmd.rs` to non-zero exits.
//!
//! Three tests, each one targets an existing production branch:
//!
//! 1. `config_get_unknown_key_exits_nonzero_with_unknown_config_key_message`
//!    — `mp config get unknown.foo.bar` hits the catch arm at
//!    `crates/mp/src/config_cmd.rs:186` (`bail!("unknown config key: ...")`).
//! 2. `config_set_unknown_key_exits_nonzero_with_unknown_config_key_message`
//!    — `mp config set unknown.foo bar` hits the same catch arm via
//!    the `apply_config_set` dispatch (line 382).
//! 3. `config_set_rejects_non_boolean_for_boolean_field` — passing
//!    `not-a-bool` to a boolean-typed key surfaces clap's parse
//!    error (line 729: `bail!(\"expected boolean, got {value}\")`),
//!    proving the typed-coercion path fires.
//!
//! **No production-code changes** — these tests only assert on the
//! already-existing error branches.

mod common;

use crate::common::TestEnv;

#[test]
fn config_get_unknown_key_exits_nonzero_with_unknown_config_key_message() {
    let env = TestEnv::new();
    let out = env.run(&["config", "get", "unknown.foo.bar", "--format", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "config get unknown key must exit non-zero; stderr={stderr}"
    );

    // The literal wording the production code uses at line 186
    // (`bail!("unknown config key: {key}")`) surfaces in stderr via
    // main.rs's Error printer — `mp config get` does NOT return a
    // JSON envelope, so the wording is in stderr.
    assert!(
        stderr.contains("unknown config key"),
        "stderr must surface the 'unknown config key' wording; got: {stderr}"
    );
    // The actual key surfaces so operators see which key was wrong.
    assert!(
        stderr.contains("unknown.foo.bar"),
        "stderr must echo the offending key; got: {stderr}"
    );
}

#[test]
fn config_set_unknown_key_exits_nonzero_with_unknown_config_key_message() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "unknown.foo.bar",
        "baz",
        "--format",
        "json",
    ]);

    assert!(
        !out.status.success(),
        "config set unknown key must exit non-zero"
    );

    // `mp config set` returns a structured envelope with `ok=false`
    // and `errors[].message` carrying the wording; the production
    // bail at config_cmd.rs:382 surfaces there rather than in stderr.
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("config set JSON");
    assert_eq!(payload["ok"].as_bool(), Some(false));
    let errors = payload["errors"].as_array().expect("errors array");
    let joined = errors
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("unknown config key"),
        "errors[].message must contain 'unknown config key'; got: {joined}"
    );
    assert!(
        joined.contains("unknown.foo.bar"),
        "errors[].message must echo the offending key; got: {joined}"
    );
}

#[test]
fn config_set_rejects_non_boolean_for_boolean_field() {
    let env = TestEnv::new();
    // `git.auto_push` is a boolean; passing `not-a-bool` triggers the
    // typed-coercion bail at config_cmd.rs:729 (`bail!("expected
    // boolean, got {value}")`). The wording surfaces in the
    // structured envelope rather than stderr.
    let out = env.run(&[
        "config",
        "set",
        "git.auto_push",
        "not-a-bool",
        "--format",
        "json",
    ]);

    assert!(
        !out.status.success(),
        "config set non-boolean must exit non-zero"
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("config set JSON");
    assert_eq!(payload["ok"].as_bool(), Some(false));
    let errors = payload["errors"].as_array().expect("errors array");
    let joined = errors
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("expected boolean"),
        "errors[].message must surface 'expected boolean'; got: {joined}"
    );
}
