//! M116 CR regression pin: `--files` value-format acceptance and rejection
//! enforced end-to-end through `mp milestone step add/update`. The shipped
//! M116 unit tests cover the parser in isolation (see `cli::files_value_parser`
//! in `cli.rs`); these CLI tests exercise the parser through the clap
//! dispatcher and confirm the milestone on disk carries the correct
//! `files: Vec<String>` payload (or rejects the input before any write).
//!
//! Background: M116 shipped a value parser that rejected JSON-array-shaped
//! strings (`["a.rs"]`) but only the `starts_with('[') && ends_with(']')`
//! pattern. A subsequent external review (dogfood log entry 30) reproduced
//! that `[a.rs`, `a.rs]`, and `{"a.rs"}` slipped through and corrupted
//! `step.files`. The parser was broadened to also reject any input whose
//! first non-whitespace character is `[` or `{` (with a JSON-parse attempt
//! for a precise error message) — and these integration tests pin the
//! behavior at the CLI level.

use crate::common::lib_api;
use crate::common::TestEnv;

fn create_milestone_with_step(env: &TestEnv) -> String {
    let create = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--title",
            "files value parser fixture",
            "--json",
            r#"{
            "title":"files value parser fixture",
            "intent":{"outcome":"cover --files value format on step add/update"},
            "problem":{"description":"M116 CR: parser was too narrow"},
            "scope":{"in_scope":["cli value parser"],"out_of_scope":["other","things"]},
            "acceptance_criteria":[{"description":"files payload round-trips correctly","verification":"manual: ok"}]
        }"#,
        ],
    );
    assert!(
        create.status.success(),
        "milestone create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout)
        .expect("create returned json")["milestone"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    lib_api::run(env, &["milestone", "approve", &id]);
    lib_api::run(
        env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
    );
    lib_api::run(
        env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "test",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    id
}

fn files_for_step(env: &TestEnv, milestone: &str, step: &str) -> Vec<String> {
    let show = lib_api::run(env, &["show", "milestone", milestone, "--format", "json"]);
    assert!(
        show.status.success(),
        "show milestone failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show returns json");
    let steps = json["steps"].as_array().expect("steps array");
    let step = steps
        .iter()
        .find(|s| s["id"].as_str() == Some(step))
        .expect("step present");
    step["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|v| v.as_str().expect("file is a string").to_string())
        .collect()
}

#[test]
fn step_add_accepts_bare_path() {
    let env = TestEnv::new();
    let _id = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--title",
            "bare-path",
            "--json",
            r#"{
        "title":"bare-path",
        "intent":{"outcome":"x"},
        "problem":{"description":"y"},
        "scope":{"in_scope":["a"],"out_of_scope":["b","c"]},
        "acceptance_criteria":[{"description":"ac","verification":"manual: ok"}]
    }"#,
        ],
    );
    let id = serde_json::from_slice::<serde_json::Value>(&_id.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    lib_api::run(&env, &["milestone", "approve", &id]);
    lib_api::run(
        &env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
    );
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "single file",
            "--files",
            "crates/mp/src/main.rs",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let files = files_for_step(&env, &id, "S1");
    assert_eq!(files, vec!["crates/mp/src/main.rs".to_string()]);
}

#[test]
fn step_add_accepts_comma_separated_list() {
    let env = TestEnv::new();
    let _id = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--title",
            "csv-list",
            "--json",
            r#"{
        "title":"csv-list",
        "intent":{"outcome":"x"},
        "problem":{"description":"y"},
        "scope":{"in_scope":["a"],"out_of_scope":["b","c"]},
        "acceptance_criteria":[{"description":"ac","verification":"manual: ok"}]
    }"#,
        ],
    );
    let id = serde_json::from_slice::<serde_json::Value>(&_id.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    lib_api::run(&env, &["milestone", "approve", &id]);
    lib_api::run(
        &env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
    );
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "multi file",
            "--files",
            "a.rs,b.rs,c.rs",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let files = files_for_step(&env, &id, "S1");
    assert_eq!(
        files,
        vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()]
    );
}

#[test]
fn step_add_rejects_quoted_json_array() {
    let env = TestEnv::new();
    let _id = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--title",
            "json-arr",
            "--json",
            r#"{
        "title":"json-arr",
        "intent":{"outcome":"x"},
        "problem":{"description":"y"},
        "scope":{"in_scope":["a"],"out_of_scope":["b","c"]},
        "acceptance_criteria":[{"description":"ac","verification":"manual: ok"}]
    }"#,
        ],
    );
    let id = serde_json::from_slice::<serde_json::Value>(&_id.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    lib_api::run(&env, &["milestone", "approve", &id]);
    lib_api::run(
        &env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
    );
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "json-array attempt",
            "--files",
            "[\"a.rs\"]",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    assert!(!add.status.success(), "quoted JSON array must be rejected");
    let stderr = String::from_utf8_lossy(&add.stderr);
    assert!(
        stderr.contains("JSON literal") || stderr.contains("looks like"),
        "error should mention JSON literal detection, got: {stderr}"
    );
}

#[test]
fn step_update_rejects_unclosed_json_array() {
    // M116 CR regression pin: the original M116 parser accepted `[a.rs`
    // (starts with `[` but doesn't end with `]`) and corrupted `step.files`.
    let env = TestEnv::new();
    let id = create_milestone_with_step(&env);
    let bad = lib_api::run(
        &env,
        &["milestone", "step", "update", &id, "S1", "--files", "[a.rs"],
    );
    assert!(
        !bad.status.success(),
        "unclosed JSON array must be rejected, got: status={:?}\nstderr={}",
        bad.status.code(),
        String::from_utf8_lossy(&bad.stderr)
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("JSON parse error")
            || stderr.contains("looks like a JSON")
            || stderr.contains("JSON literal"),
        "error should mention JSON parse / rejection, got: {stderr}"
    );

    // Pin that the milestone was NOT corrupted by the rejected call.
    let files = files_for_step(&env, &id, "S1");
    assert!(
        files.is_empty(),
        "step.files should still be empty after rejection, got: {files:?}"
    );
}

#[test]
fn step_update_rejects_object_literal() {
    // M116 CR regression pin: `{"a.rs"}` is not valid JSON (no `key: value`),
    // but it still smells like a JSON literal so the parser must reject it
    // (rather than accept it as a bare path — that would corrupt step.files).
    let env = TestEnv::new();
    let id = create_milestone_with_step(&env);
    let bad = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            &id,
            "S1",
            "--files",
            "{\"a.rs\"}",
        ],
    );
    assert!(
        !bad.status.success(),
        "object literal must be rejected, got: status={:?}\nstderr={}",
        bad.status.code(),
        String::from_utf8_lossy(&bad.stderr)
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("JSON parse error")
            || stderr.contains("JSON literal")
            || stderr.contains("looks like a JSON"),
        "error should mention JSON parse / rejection, got: {stderr}"
    );

    let files = files_for_step(&env, &id, "S1");
    assert!(
        files.is_empty(),
        "step.files should still be empty after rejection, got: {files:?}"
    );
}

#[test]
fn step_update_rejects_empty_files() {
    let env = TestEnv::new();
    let id = create_milestone_with_step(&env);
    let bad = lib_api::run(
        &env,
        &["milestone", "step", "update", &id, "S1", "--files", ""],
    );
    assert!(
        !bad.status.success(),
        "empty --files must be rejected, got: status={:?}\nstderr={}",
        bad.status.code(),
        String::from_utf8_lossy(&bad.stderr)
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("cannot be empty"),
        "error should mention empty rejection, got: {stderr}"
    );
}

#[test]
fn step_update_persists_bare_path() {
    let env = TestEnv::new();
    let id = create_milestone_with_step(&env);
    let upd = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            &id,
            "S1",
            "--files",
            "crates/mp/src/foo.rs",
        ],
    );
    assert!(
        upd.status.success(),
        "{}",
        String::from_utf8_lossy(&upd.stderr)
    );
    let files = files_for_step(&env, &id, "S1");
    assert_eq!(files, vec!["crates/mp/src/foo.rs".to_string()]);
}

#[test]
fn step_update_persists_comma_separated() {
    let env = TestEnv::new();
    let id = create_milestone_with_step(&env);
    let upd = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            &id,
            "S1",
            "--files",
            "a.rs,b.rs",
        ],
    );
    assert!(
        upd.status.success(),
        "{}",
        String::from_utf8_lossy(&upd.stderr)
    );
    let files = files_for_step(&env, &id, "S1");
    assert_eq!(files, vec!["a.rs".to_string(), "b.rs".to_string()]);
}
