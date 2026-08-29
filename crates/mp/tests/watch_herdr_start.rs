//! M149 S3 / AC-02, AC-04 + M197 WP2 / AC-03: herdr agent start
//! abstraction.
//!
//! Strategy: write a fake `herdr` shell script and pass its path
//! directly to `spawn_pane` / `ensure_pane`. The fake records argv to
//! a file and emits a deterministic pane-id. This verifies mp builds
//! the correct `herdr pane split … && herdr agent start … --kind …
//! --pane …` two-step shape and parses the pane id from herdr's
//! output, without requiring a real herdr server or PATH
//! manipulation. Pure helpers (kind resolution, list-output parsing,
//! label format) are covered by unit tests in
//! `crates/mp/src/watch/herdr.rs`.
//!
//! M197 change: the legacy `agent start <label> --cwd <root> --
//! <harness argv>` shape is gone. The fake script now has to handle
//! `pane split` (returns a new pane id) and `agent start` (takes
//! `--kind` and `--pane` instead of `--cwd` and `--`).

mod common;

use crate::common::TestEnv;
use mp::config::RoleConfig;
use mp::watch::{
    ensure_pane, find_existing_pane, parse_pane_id_from_start_output, resolve_harness_kind,
    spawn_pane, Role,
};
use std::fs;
use std::path::{Path, PathBuf};

/// Install a fake `herdr` script at `<dir>/herdr` and return the path.
/// The script branches on `$2` (the herdr subcommand: list / start /
/// split / anything-else) and emits the canned shapes the tests need.
/// Every invocation is appended to `<log>` so argv-shape assertions
/// can read it back. M197: the fake handles `pane split` (returns a
/// new pane id) and `agent start` (expects `--kind` / `--pane`).
fn install_fake_herdr(dir: &Path, log: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
echo "argv: $*" >> "{log}"
case "$2" in
  list)
    echo '{{"agents":[{{"name":"role-runner-1","pane_id":"%fake-runner"}},{{"name":"role-coordinator-1","pane_id":"%fake-coord"}}]}}'
    ;;
  split)
    echo '{{"pane_id":"%new-pane-7"}}'
    ;;
  start)
    echo '{{"pane_id":"%spawned-42","status":"started"}}'
    ;;
  *)
    echo '{{}}'
    ;;
esac
"#,
        log = log.display()
    );
    let bin = dir.join("herdr");
    fs::write(&bin, script).unwrap();
    set_executable(&bin);
    bin
}

fn install_custom_fake(dir: &Path, name: &str, body: &str) -> PathBuf {
    let script = format!("#!/bin/sh\n{body}\n");
    let bin = dir.join(name);
    fs::write(&bin, script).unwrap();
    set_executable(&bin);
    bin
}

fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

#[test]
fn spawn_pane_invokes_herdr_agent_start_with_correct_argv() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = env.tmp.path().join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(&bin_dir, &log);

    let kind = resolve_harness_kind(&RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    });
    let handle = spawn_pane(&bin, "role-runner-1", &kind, "%new-pane-7", &[]).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-42");

    let log_text = fs::read_to_string(&log).unwrap();
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
    let log = env.tmp.path().join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(&bin_dir, &log);

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    };
    let handle = ensure_pane(&bin, Role::Runner, 1, &rc, env.tmp.path()).unwrap();
    assert!(
        handle.reused,
        "ensure_pane should reuse when label exists: {:?}",
        handle
    );
    assert_eq!(handle.pane_id, "%fake-runner");
    // M197: when reusing, no `pane split` or `agent start` should
    // be called — the list hit is the entire lifecycle.
    let log_text = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !log_text.contains("agent start") && !log_text.contains("pane split"),
        "reuse path must not call pane split / agent start: {log_text}"
    );
}

#[test]
fn ensure_pane_spawns_via_pane_split_then_agent_start() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = env.tmp.path().join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    // Custom fake whose `list` returns no agents, so ensure_pane
    // must take the spawn path. M197: spawn now means
    // `pane split --cwd <path>` followed by
    // `agent start <label> --kind <kind> --pane <pane-id>`.
    let bin = install_custom_fake(
        &bin_dir,
        "herdr",
        &format!(
            r#"echo "argv: $*" >> "{log}"
case "$2" in
  list) echo '{{"agents":[]}}';;
  split) echo '{{"pane_id":"%new-pane-9"}}';;
  start) echo '{{"pane_id":"%spawned-99"}}';;
esac"#,
            log = log.display()
        ),
    );

    let rc = RoleConfig {
        harness: Some("cursor".into()),
        ..Default::default()
    };
    let handle = ensure_pane(&bin, Role::Coordinator, 1, &rc, env.tmp.path()).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-99");

    let log_text = fs::read_to_string(&log).unwrap();
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
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_custom_fake(
        &bin_dir,
        "herdr",
        r#"echo "agent booting (no pane id here)""#,
    );

    let kind = "opencode".to_string();
    let handle = spawn_pane(&bin, "role-runner-1", &kind, "%pane-id", &[]).unwrap();
    assert_eq!(
        handle.pane_id, "role-runner-1",
        "label fallback when pane id can't be parsed"
    );
}

#[test]
fn list_panes_failure_does_not_block_spawn() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // Custom fake whose `list` exits non-zero. ensure_pane should
    // fall through to spawn rather than error out — a transient
    // list failure must not block the run.
    let bin = install_custom_fake(
        &bin_dir,
        "herdr",
        r#"case "$2" in
  list) echo "list failed" 1>&2; exit 2 ;;
  split) echo '{"pane_id":"%p"}' ;;
  start) echo '{"pane_id":"%ok"}' ;;
  *) echo '{}' ;;
esac"#,
    );

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    };
    let handle = ensure_pane(&bin, Role::Runner, 1, &rc, env.tmp.path()).unwrap();
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
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_custom_fake(&bin_dir, "herdr", r#"echo "boom" 1>&2; exit 1"#);

    let err = spawn_pane(&bin, "role-runner-1", "opencode", "%pane-id", &[]).unwrap_err();
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
