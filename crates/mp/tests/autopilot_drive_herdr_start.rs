//! M149 S3 / AC-02, AC-04 + M197 WP2 / AC-03: herdr agent start
//! abstraction.
//!
//! Strategy: install a fake `herdr` shell script via the shared
//! [`crate::common::fake_herdr`] harness and pass its path directly
//! to `spawn_pane` / `ensure_pane`. The fake records argv to a file
//! and emits deterministic pane ids, which lets tests assert the
//! argv shape without requiring a real herdr server or PATH
//! manipulation. Pure helpers (kind resolution, list-output parsing,
//! label format) are covered by unit tests in
//! `crates/mp/src/autopilot/drive/herdr.rs`.
//!
//! M197 change: the legacy `agent start <label> --cwd <root> --
//! <harness argv>` shape is gone. The fake script now has to handle
//! `pane split` (returns a new pane id) and `agent start` (takes
//! `--kind` and `--pane` instead of `--cwd` and `--`).
//!
//! M227 / WP1: the per-test fake-herdr shell-script builders
//! scattered across watch_herdr_wait / watch_herdr_start /
//! watch_bridge_report are consolidated into the shared harness so
//! future autopilot suites can compose off the same primitive.

mod common;

use crate::common::fake_herdr::FakeHerdrBuilder;
use crate::common::TestEnv;
use mp::autopilot::drive::{
    ensure_pane, find_existing_pane, parse_pane_id_from_start_output, resolve_harness_kind,
    spawn_pane, Role,
};
use mp::config::RoleConfig;

#[test]
fn spawn_pane_invokes_herdr_agent_start_with_correct_argv() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .pane_split_response(r#"{"pane_id":"%new-pane-7"}"#)
        .agent_start_response(r#"{"pane_id":"%spawned-42","status":"started"}"#)
        .install(&bin_dir);

    let kind = resolve_harness_kind(&RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    });
    let handle = spawn_pane(fake.path(), "role-runner-1", &kind, "%new-pane-7", &[]).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-42");

    let log_text = fake.read_log();
    assert!(
        log_text.contains("agent start role-runner-1"),
        "expected `agent start <label>` in herdr argv log: {log_text}"
    );
    assert!(
        log_text.contains("--kind opencode"),
        "expected `--kind opencode` flag in herdr argv log: {log_text}"
    );
    assert!(
        log_text.contains("--pane %new-pane-7"),
        "expected `--pane <id>` flag in herdr argv log: {log_text}"
    );
    // M197: the legacy --cwd and `--` separator must NOT appear on
    // the agent start call (cwd lives on `herdr pane split`).
    assert!(
        !log_text.contains("--cwd"),
        "agent start must not carry --cwd (cwd belongs to pane split): {log_text}"
    );
    assert!(
        !log_text.contains("-- opencode"),
        "agent start must not carry the `-- <argv>` separator (herdr 0.7.x uses --kind): {log_text}"
    );
}

#[test]
fn ensure_pane_reuses_existing_when_label_matches() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_list_response(
            r#"{"agents":[{"name":"role-runner-1","pane_id":"%fake-runner"},{"name":"role-coordinator-1","pane_id":"%fake-coord"}]}"#,
        )
        .install(&bin_dir);

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    };
    let handle = ensure_pane(fake.path(), Role::Runner, 1, &rc, env.tmp.path()).unwrap();
    assert!(
        handle.reused,
        "ensure_pane should reuse when label exists: {:?}",
        handle
    );
    assert_eq!(handle.pane_id, "%fake-runner");
    // M197: when reusing, no `pane split` or `agent start` should
    // be called — the list hit is the entire lifecycle.
    let log_text = fake.read_log();
    assert!(
        !log_text.contains("agent start") && !log_text.contains("pane split"),
        "reuse path must not call pane split / agent start: {log_text}"
    );
}

#[test]
fn ensure_pane_spawns_via_pane_split_then_agent_start() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    // Custom fake whose `list` returns no agents, so ensure_pane
    // must take the spawn path. M197: spawn now means
    // `pane split --cwd <path>` followed by
    // `agent start <label> --kind <kind> --pane <pane-id>`.
    let fake = FakeHerdrBuilder::new()
        .agent_list_response(r#"{"agents":[]}"#)
        .pane_split_response(r#"{"pane_id":"%new-pane-9"}"#)
        .agent_start_response(r#"{"pane_id":"%spawned-99","status":"started"}"#)
        .install(&bin_dir);

    let rc = RoleConfig {
        harness: Some("cursor".into()),
        ..Default::default()
    };
    let handle = ensure_pane(fake.path(), Role::Coordinator, 1, &rc, env.tmp.path()).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-99");

    let log_text = fake.read_log();
    // The two-step shape:
    //   1) `pane split --cwd <project_root>`
    //   2) `agent start <label> --kind cursor --pane %new-pane-9`
    assert!(
        log_text.contains("pane split"),
        "ensure_pane must call pane split when spawning: {log_text}"
    );
    assert!(
        log_text.contains("agent start role-coordinator-1"),
        "ensure_pane must call agent start with the right label: {log_text}"
    );
    assert!(
        log_text.contains("--kind cursor"),
        "ensure_pane must pass --kind cursor (the harness config): {log_text}"
    );
    assert!(
        log_text.contains("--pane %new-pane-9"),
        "ensure_pane must pass --pane <id> from the prior pane split: {log_text}"
    );
    // pane split must carry --cwd (agent start no longer does).
    let split_line = log_text
        .lines()
        .find(|l| l.contains("pane split"))
        .expect("pane split argv line");
    assert!(
        split_line.contains("--cwd"),
        "pane split must carry --cwd: {split_line}"
    );
}

#[test]
fn spawn_pane_falls_back_to_label_when_output_unparseable() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    // Unparseable `agent start` output → spawn_pane falls back to
    // the label as the pane id.
    let fake = FakeHerdrBuilder::new()
        .agent_start_response("agent booting (no pane id here)")
        .install(&bin_dir);

    let kind = "opencode".to_string();
    let handle = spawn_pane(fake.path(), "role-runner-1", &kind, "%pane-id", &[]).unwrap();
    assert_eq!(
        handle.pane_id, "role-runner-1",
        "label fallback when pane id can't be parsed"
    );
}

#[test]
fn list_panes_failure_does_not_block_spawn() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    // Custom fake whose `list` exits non-zero. ensure_pane should
    // fall through to spawn rather than error out — a transient
    // list failure must not block the run.
    let fake = FakeHerdrBuilder::new()
        .agent_list_failure(2, "list failed")
        .pane_split_response(r#"{"pane_id":"%p"}"#)
        .agent_start_response(r#"{"pane_id":"%ok","status":"started"}"#)
        .install(&bin_dir);

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    };
    let handle = ensure_pane(fake.path(), Role::Runner, 1, &rc, env.tmp.path()).unwrap();
    assert_eq!(handle.pane_id, "%ok");
    assert!(!handle.reused);
}

#[test]
fn parse_pane_id_handles_real_herdr_output_shapes() {
    // Pin a few real observed herdr output shapes so a herdr version
    // bump does not silently break the parser.
    assert_eq!(
        parse_pane_id_from_start_output(r#"{"pane_id":"%3"}"#),
        Some("%3".into())
    );
    assert_eq!(
        parse_pane_id_from_start_output(r#"{"agent":{"id":"ag-9"}}"#),
        Some("ag-9".into())
    );
    assert_eq!(
        parse_pane_id_from_start_output("started role-runner-1 pane=%7"),
        Some("%7".into())
    );
    // M197: the new pane shape returns a top-level `{"pane":{"id":...}}`
    // envelope — make sure the parser picks that up too.
    assert_eq!(
        parse_pane_id_from_start_output(r#"{"pane":{"id":"%new-pane-7"}}"#),
        Some("%new-pane-7".into())
    );
}

#[test]
fn find_pane_returns_none_for_empty_or_unrelated_list() {
    assert_eq!(
        find_existing_pane("role-runner-1", r#"{"agents":[]}"#),
        None
    );
    assert_eq!(
        find_existing_pane(
            "role-runner-1",
            r#"{"agents":[{"name":"other","pane_id":"%1"}]}"#
        ),
        None
    );
}

#[test]
fn spawn_pane_propagates_non_zero_exit_as_error() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_start_failure(1, "boom")
        .install(&bin_dir);

    let err = spawn_pane(fake.path(), "role-runner-1", "opencode", "%pane-id", &[]).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("failed") && msg.contains("boom"),
        "error should surface herdr stderr: {msg}"
    );
}

#[test]
fn resolve_harness_kind_uses_config_when_set() {
    assert_eq!(
        resolve_harness_kind(&RoleConfig {
            harness: Some("pi".into()),
            ..Default::default()
        }),
        "pi"
    );
    assert_eq!(
        resolve_harness_kind(&RoleConfig {
            harness: Some("cursor".into()),
            ..Default::default()
        }),
        "cursor"
    );
}

#[test]
fn resolve_harness_kind_defaults_to_opencode_when_unset() {
    assert_eq!(resolve_harness_kind(&RoleConfig::default()), "opencode");
}
