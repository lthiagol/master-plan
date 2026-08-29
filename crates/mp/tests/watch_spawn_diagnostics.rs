//! M197 WP3 / AC-04: spawn-diagnostics contract tests.
//!
//! Covers the structured `SpawnFailure` contract:
//! - `pane_split` and `spawn_pane` return a [`SpawnFailure`] (not
//!   just a stringy anyhow error) when herdr exits non-zero.
//! - The `extract_spawn_failure` helper unwraps it from an
//!   `anyhow::Error` chain.
//! - The fields on `SpawnFailure` match the on-the-wire
//!   `spawn_error` log entry (command, argv, exit_code, stdout,
//!   stderr).
//!
//! The watch log + sequencer integration tests in
//! `watch_herdr_start.rs` cover the live `pane split` /
//! `agent start` two-step. This file pins the diagnostic
//! contract separately so a refactor of the herdr layer cannot
//! silently drop the spawn error fields.

mod common;

use mp::watch::herdr::{
    build_pane_split_args, build_start_args, extract_spawn_failure, SpawnFailure,
};

#[test]
fn spawn_failure_carries_command_argv_exit_stdout_stderr() {
    // Construct a SpawnFailure the way `pane_split` /
    // `spawn_pane` would on a non-zero exit, then assert each
    // field survives the round-trip through anyhow.
    let failure = SpawnFailure {
        command: "pane split".into(),
        argv: build_pane_split_args(std::path::Path::new("/repo")),
        exit_code: Some(2),
        stdout: "stdout line\n".into(),
        stderr: "herdr: workspace full\n".into(),
    };
    let err: anyhow::Error = anyhow::Error::new(failure.clone());
    let extracted = extract_spawn_failure(&err).expect("SpawnFailure in chain");
    assert_eq!(extracted.command, "pane split");
    assert_eq!(extracted.exit_code, Some(2));
    assert_eq!(extracted.stdout, "stdout line\n");
    assert_eq!(extracted.stderr, "herdr: workspace full\n");
    assert!(
        extracted.argv.contains(&"--cwd".to_string())
            && extracted.argv.contains(&"/repo".to_string()),
        "argv should carry the pane-split flags: {:?}",
        extracted.argv
    );
}

#[test]
fn spawn_failure_for_agent_start_preserves_kind_and_pane() {
    let failure = SpawnFailure {
        command: "agent start".into(),
        argv: build_start_args("role-runner-1", "opencode", "%7"),
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "herdr: pane not found\n".into(),
    };
    let err: anyhow::Error = anyhow::Error::new(failure.clone());
    let extracted = extract_spawn_failure(&err).expect("SpawnFailure in chain");
    assert_eq!(extracted.command, "agent start");
    assert_eq!(extracted.exit_code, Some(1));
    assert!(
        extracted.argv.contains(&"--kind".to_string())
            && extracted.argv.contains(&"opencode".to_string())
            && extracted.argv.contains(&"--pane".to_string())
            && extracted.argv.contains(&"%7".to_string()),
        "argv should carry the agent-start flags: {:?}",
        extracted.argv
    );
}

#[test]
fn extract_spawn_failure_returns_none_for_unrelated_errors() {
    // A plain anyhow::Error that does NOT contain a
    // SpawnFailure must not be downcast to one. Otherwise the
    // sequencer would map an unrelated herdr / I/O error to
    // RunOutcome::SpawnFailed and confuse the operator.
    let err: anyhow::Error = anyhow::anyhow!("plain error with no spawn context");
    assert!(extract_spawn_failure(&err).is_none());

    let err: anyhow::Error = anyhow::Error::msg("another plain error").context("nested context");
    assert!(extract_spawn_failure(&err).is_none());
}

#[test]
fn extract_spawn_failure_walks_context_wrapped_chain() {
    // F-06 / L17 + L28: production `pane_split` and `spawn_pane`
    // wrap the SpawnFailure in `.context("herdr … exec failure")`
    // so the operator sees a contextual message. The earlier
    // `downcast_ref`-only implementation silently returned None
    // when the wrapper was layered on top, which masked the
    // binary-missing failure mode behind a generic `stale`
    // state — exactly the failure mode AC-02 / AC-04 promise to
    // surface loudly. Pin the chain-walk contract here so a
    // future refactor can't reintroduce the bug.
    let failure = SpawnFailure {
        command: "pane split".into(),
        argv: build_pane_split_args(std::path::Path::new("/repo")),
        exit_code: None,
        stdout: String::new(),
        stderr:
            "failed to exec /usr/bin/herdr: not found (is the herdr binary on PATH and executable?)"
                .into(),
    };
    let err: anyhow::Error =
        anyhow::Error::new(failure.clone()).context("herdr pane split exec failure");
    let extracted = extract_spawn_failure(&err)
        .expect("SpawnFailure must survive .context() wrappers on the chain");
    assert_eq!(extracted.command, "pane split");
    assert_eq!(extracted.exit_code, None);
    assert!(
        extracted.stderr.contains("not found"),
        "stderr must survive the wrapper: {:?}",
        extracted.stderr
    );
}

#[test]
fn spawn_failure_display_message_mentions_command_and_exit() {
    let failure = SpawnFailure {
        command: "agent start".into(),
        argv: vec!["agent".into(), "start".into(), "label".into()],
        exit_code: Some(3),
        stdout: String::new(),
        stderr: "boom".into(),
    };
    let msg = format!("{failure}");
    assert!(
        msg.contains("agent start"),
        "Display should mention command: {msg}"
    );
    assert!(msg.contains("3"), "Display should mention exit code: {msg}");
    assert!(msg.contains("boom"), "Display should mention stderr: {msg}");
}
