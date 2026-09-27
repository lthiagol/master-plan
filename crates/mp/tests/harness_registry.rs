//! M151 S1 / AC-01 — integration tests for the `HarnessRegistry`
//! surface. The registry must be data-driven: every v1 entry
//! resolves via `HarnessRegistry::v1().get(name)`, the
//! `(model, thinking) -> argv` translation applies the entry's
//! own flag strings, and unknown ids return a structured
//! `HarnessError::Unsupported` that names every supported harness
//! and the on-ramp `herdr integration install <name>` command.

use mp::harness::{HarnessError, HarnessRegistry};

/// Public-API stable surface: `mp::harness::*` re-exports
/// `HarnessRegistry` from `crates/mp/src/harness/registry.rs`.
fn reg() -> HarnessRegistry {
    HarnessRegistry::v1()
}

#[test]
fn v1_registry_lists_opencode_pi_cursor() {
    assert_eq!(reg().supported_names(), vec!["opencode", "pi", "cursor"]);
}

#[test]
fn each_v1_entry_resolves_to_a_well_formed_command() {
    let r = reg();
    for id in ["opencode", "pi", "cursor"] {
        let entry = r.get(id).expect("v1 entry must resolve");
        assert_eq!(entry.id, id);
        assert!(
            !entry.command.is_empty(),
            "{id} must declare a non-empty command"
        );
        // 2026-09-27 (opencode CLI reality): opencode does NOT
        // accept `--model` at the top level. cursor and pi do.
        // Per-harness model support is now heterogeneous; this
        // test pins the corrected contract.
        if id == "opencode" {
            assert_eq!(
                entry.model_flag, None,
                "opencode must NOT emit --model at the top level"
            );
        } else {
            assert_eq!(
                entry.model_flag,
                Some("--model"),
                "{id} must accept --model in v1"
            );
        }
    }
}

#[test]
fn cursor_is_the_only_v1_entry_with_thinking_flag() {
    assert_eq!(
        reg().get("cursor").unwrap().thinking_flag,
        Some("--thinking"),
        "cursor is the only v1 harness that exposes --thinking"
    );
    for id in ["opencode", "pi"] {
        assert_eq!(
            reg().get(id).unwrap().thinking_flag,
            None,
            "{id} must NOT carry a thinking flag in v1"
        );
    }
}

#[test]
fn resolve_argv_base_case_emits_only_the_command() {
    assert_eq!(
        reg().resolve_argv("opencode", None, None).unwrap(),
        vec!["opencode".to_string()]
    );
    assert_eq!(
        reg().resolve_argv("pi", None, None).unwrap(),
        vec!["pi".to_string()]
    );
    assert_eq!(
        reg().resolve_argv("cursor", None, None).unwrap(),
        vec!["cursor".to_string()]
    );
}

#[test]
fn resolve_argv_appends_model_for_each_supported_harness() {
    // 2026-09-27 (opencode CLI reality): opencode's CLI does NOT
    // accept `--model` at the top level (see registry.rs:opencode
    // entry — `model_flag: None`). cursor and pi still accept it.
    // Pinning both shapes here so a future regression that
    // re-adds `--model` for opencode (or drops it for cursor/pi)
    // trips this test.
    let model = "claude-opus-4";
    // opencode: model is recorded but NOT appended to argv.
    assert_eq!(
        reg().resolve_argv("opencode", Some(model), None).unwrap(),
        vec!["opencode".to_string()]
    );
    // pi: --model is appended.
    assert_eq!(
        reg().resolve_argv("pi", Some(model), None).unwrap(),
        vec!["pi".to_string(), "--model".to_string(), model.to_string()]
    );
    // cursor: --model is appended.
    assert_eq!(
        reg().resolve_argv("cursor", Some(model), None).unwrap(),
        vec![
            "cursor".to_string(),
            "--model".to_string(),
            model.to_string()
        ]
    );
}

#[test]
fn resolve_argv_cursor_with_thinking_emits_both_flags() {
    let argv = reg()
        .resolve_argv("cursor", Some("claude-opus-4"), Some("high"))
        .unwrap();
    assert_eq!(
        argv,
        vec![
            "cursor".to_string(),
            "--model".to_string(),
            "claude-opus-4".to_string(),
            "--thinking".to_string(),
            "high".to_string()
        ]
    );
}

#[test]
fn resolve_argv_skips_thinking_flag_when_caller_omits_it() {
    // Caller-side None wins over harness-side Some — useful when
    // a user has set a model but not a thinking level; we
    // never invent flags the user did not ask for.
    let argv = reg()
        .resolve_argv("cursor", Some("claude-opus-4"), None)
        .unwrap();
    assert_eq!(
        argv,
        vec![
            "cursor".to_string(),
            "--model".to_string(),
            "claude-opus-4".to_string()
        ]
    );
}

#[test]
fn unknown_harness_returns_structured_unsupported_error() {
    let err = reg().get("claude-code").unwrap_err();
    let msg = format!("{err}");
    // Display contract: every supported name + the install hint.
    assert!(msg.contains("claude-code"), "names the offender: {msg}");
    assert!(msg.contains("opencode"), "lists opencode: {msg}");
    assert!(
        msg.contains("'pi'") || msg.contains("|pi|") || msg.contains(" pi "),
        "lists pi: {msg}"
    );
    assert!(msg.contains("cursor"), "lists cursor: {msg}");
    assert!(
        msg.contains("herdr integration install claude-code"),
        "points at the on-ramp: {msg}"
    );
}

#[test]
fn unknown_harness_never_panics_on_pathological_input() {
    let r = reg();
    for name in ["", "../", "OPENCODE", "open code", "🎉", "\nrm -rf /"] {
        // Each call must return Err — never a panic, never Ok.
        let result = r.get(name);
        assert!(result.is_err(), "{name:?} must not match");
    }
}

#[test]
fn registry_iter_yields_one_entry_per_v1_harness() {
    let names: Vec<&str> = reg().iter().map(|e| e.id).collect();
    assert_eq!(names, vec!["opencode", "pi", "cursor"]);
}

#[test]
fn registry_round_trips_through_serde() {
    // The `HarnessEntry` shape underpins `mp agent harness list`
    // JSON output. Pinning the field names here means a future
    // refactor that silently renames a field will trip the
    // integration test instead of the user-facing CLI.
    //
    // 2026-09-27 (opencode CLI reality): `model_flag` is now
    // `None` for opencode (its CLI rejects `--model`). Pin both
    // shapes — cursor/pi carry the string, opencode carries null.
    let v_cursor = serde_json::to_value(reg().get("cursor").unwrap()).unwrap();
    assert!(v_cursor["id"].is_string());
    assert!(v_cursor["display_name"].is_string());
    assert!(v_cursor["command"].is_string());
    assert!(v_cursor["model_flag"].is_string());
    assert!(v_cursor["thinking_flag"].is_string());

    let v_opencode = serde_json::to_value(reg().get("opencode").unwrap()).unwrap();
    assert!(v_opencode["id"].is_string());
    assert!(v_opencode["display_name"].is_string());
    assert!(v_opencode["command"].is_string());
    assert!(v_opencode["model_flag"].is_null());
    assert!(v_opencode["thinking_flag"].is_null());
}

#[test]
fn resolve_argv_for_unknown_harness_returns_named_error() {
    let err = reg()
        .resolve_argv("aider", Some("claude-opus-4"), None)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("aider"), "must name the bad id: {msg}");
    assert!(
        matches!(err, HarnessError::Unsupported { .. }),
        "must be the Unsupported variant"
    );
}
