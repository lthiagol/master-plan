//! M149 S3 / AC-02, AC-04: herdr agent start abstraction.
//!
//! Strategy: write a fake `herdr` shell script and pass its path
//! directly to `spawn_pane` / `ensure_pane`. The fake records argv to
//! a file and emits a deterministic pane-id. This verifies mp builds
//! the correct `herdr agent start … -- <argv>` command and parses the
//! pane id from herdr's output, without requiring a real herdr server
//! or PATH manipulation. Pure helpers (argv resolution, list-output
//! parsing, label format) are covered by unit tests in
//! `crates/mp/src/watch/herdr.rs`.

mod common;

use crate::common::TestEnv;
use mp::config::RoleConfig;
use mp::watch::{
    ensure_pane, find_existing_pane, parse_pane_id_from_start_output, resolve_harness_argv,
    spawn_pane, Role,
};
use std::fs;
use std::path::{Path, PathBuf};

/// Install a fake `herdr` script at `<dir>/herdr` and return the path.
/// The script branches on `$2` (the herdr subcommand: list / start /
/// anything-else) and emits the canned shapes the tests need.
/// Every invocation is appended to `<log>` so argv-shape assertions
/// can read it back.
fn install_fake_herdr(dir: &Path, log: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
echo "argv: $*" >> "{log}"
case "$2" in
  list)
    echo '{{"agents":[{{"name":"role-runner-1","pane_id":"%fake-runner"}},{{"name":"role-coordinator-1","pane_id":"%fake-coord"}}]}}'
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

    let argv = resolve_harness_argv(&RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    })
    .unwrap();
    let handle = spawn_pane(&bin, "role-runner-1", env.tmp.path(), &argv).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-42");

    let log_text = fs::read_to_string(&log).unwrap();
    assert!(
        log_text.contains("agent start role-runner-1"),
        "expected `agent start <label>` in herdr argv log: {log_text}"
    );
    assert!(
        log_text.contains("--cwd"),
        "expected --cwd flag in herdr argv log: {log_text}"
    );
    assert!(
        log_text.contains("-- opencode"),
        "expected `-- opencode` harness argv in log: {log_text}"
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
}

#[test]
fn ensure_pane_spawns_when_no_existing_match() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = env.tmp.path().join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    // Custom fake whose `list` returns no agents.
    let bin = install_custom_fake(
        &bin_dir,
        "herdr",
        r#"case "$2" in
  list) echo '{"agents":[]}';;
  start) echo '{"pane_id":"%spawned-42"}';;
esac"#,
    );

    let rc = RoleConfig {
        harness: Some("cursor".into()),
        ..Default::default()
    };
    let handle = ensure_pane(&bin, Role::Coordinator, 1, &rc, env.tmp.path()).unwrap();
    assert!(!handle.reused);
    assert_eq!(handle.pane_id, "%spawned-42");
    let log_text = fs::read_to_string(&log).unwrap_or_default();
    let _ = log_text; // log not written for this custom fake
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

    let argv = vec!["opencode".to_string()];
    let handle = spawn_pane(&bin, "role-runner-1", env.tmp.path(), &argv).unwrap();
    assert_eq!(
        handle.pane_id, "role-runner-1",
        "label fallback when pane id can't be parsed"
    );
}

#[test]
fn ensure_pane_uses_explicit_command_over_harness_default() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = env.tmp.path().join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    // Custom fake whose `list` returns empty so the spawn path runs.
    let bin = install_custom_fake(
        &bin_dir,
        "herdr",
        &format!(
            r#"echo "argv: $*" >> "{log}"
case "$2" in
  list) echo '{{"agents":[]}}';;
  start) echo '{{"pane_id":"%spawned-42"}}';;
esac"#,
            log = log.display()
        ),
    );

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        command: Some(vec!["my-custom-runner".into(), "--debug".into()]),
        ..Default::default()
    };
    let _ = ensure_pane(&bin, Role::Runner, 1, &rc, env.tmp.path()).unwrap();
    let log_text = fs::read_to_string(&log).unwrap();
    assert!(
        log_text.contains("-- my-custom-runner --debug"),
        "explicit command should override harness default: {log_text}"
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

    let argv = vec!["opencode".to_string()];
    let err = spawn_pane(&bin, "role-runner-1", env.tmp.path(), &argv).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("failed") && msg.contains("boom"),
        "error should surface herdr stderr: {msg}"
    );
}
