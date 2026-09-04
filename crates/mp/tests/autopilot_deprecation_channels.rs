//! M219 / S01 + S02: `mp watch` deprecation channels.
//!
//! The legacy `mp watch` command emits exactly one documented warning
//! on stderr per invocation; canonical `mp autopilot` invocations
//! never warn; stdout and the exit code are unchanged.
//!
//! Two ACs share this file:
//! - AC-01 (S01): warning lands on stderr only; stdout and exit code
//!   are preserved (canonical autopilot invocation is the control).
//! - AC-02 (S02): `mp autopilot --help` (and the canonical command
//!   tree) never print the deprecation warning.

mod common;

use common::TestEnv;

/// Run `mp watch <args>` and return (status, stdout, stderr).
fn run_watch(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["watch".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

/// Run `mp autopilot <args>` and return (status, stdout, stderr).
fn run_autopilot(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["autopilot".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

/// Trim trailing newlines so the comparison is not sensitive to
/// cosmetic whitespace differences.
fn trim_trailing_newlines(s: &[u8]) -> Vec<u8> {
    let mut v = s.to_vec();
    while matches!(v.last(), Some(b'\n') | Some(b'\r')) {
        v.pop();
    }
    v
}

// ─── AC-01: warning lands on stderr only ─────────────────────────────

/// S01 / AC-01: `mp watch --dry-run` writes the deprecation line to
/// stderr; stdout is identical to the canonical `mp autopilot start`
/// invocation and the exit code is unchanged.
#[test]
fn watch_warning_is_on_stderr_only_and_exit_code_preserved() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let autopilot = run_autopilot(&env, &["start", "--dry-run", "--format", "json"]);

    // Exit code unchanged between the two commands.
    assert_eq!(
        watch.status.code(),
        autopilot.status.code(),
        "exit codes must match (warning is on stderr only)"
    );

    // Stdout is identical (modulo trailing newlines) — the deprecation
    // message does NOT pollute stdout, so JSON consumers still see a
    // clean payload.
    assert_eq!(
        trim_trailing_newlines(&watch.stdout),
        trim_trailing_newlines(&autopilot.stdout),
        "stdout bytes must be identical (modulo trailing newlines)"
    );

    // Stderr contains the deprecation line. We check the substring
    // here (byte-for-byte matching lives in autopilot_deprecation_text).
    let watch_stderr = String::from_utf8_lossy(&watch.stderr);
    assert!(
        watch_stderr.contains("deprecated"),
        "`mp watch` stderr must contain the deprecation line; got: {watch_stderr}"
    );

    // Stderr does NOT leak to stdout — stdout must NOT contain the
    // warning text. Use the spec'd wording as the marker.
    let watch_stdout = String::from_utf8_lossy(&watch.stdout);
    assert!(
        !watch_stdout.contains("deprecated"),
        "`mp watch` stdout must NOT carry the deprecation line; got: {watch_stdout}"
    );
}

/// S01 / AC-01: `mp autopilot start` (the canonical name) emits an
/// empty stderr for the dry-run path. If anything leaks through here
/// it is a real regression — the alias contract is that canonical
/// invocations never warn.
#[test]
fn autopilot_canonical_invocation_emits_empty_stderr() {
    let env = TestEnv::new();
    let autopilot = run_autopilot(&env, &["start", "--dry-run", "--format", "json"]);

    let autopilot_stderr = trim_trailing_newlines(&autopilot.stderr);
    assert!(
        autopilot_stderr.is_empty(),
        "`mp autopilot start --dry-run` must not write to stderr; got: {}",
        String::from_utf8_lossy(&autopilot_stderr)
    );
}

/// S01 / AC-01: the warning fires exactly once per invocation. Two
/// separate invocations each emit one line — no per-session caching
/// or duplicate emission within a single process.
#[test]
fn watch_warning_emits_exactly_one_line_per_invocation() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);

    // Strip trailing newline so we can count lines in the body.
    let body = trim_trailing_newlines(&watch.stderr);
    assert!(
        !body.is_empty(),
        "expected the deprecation line on stderr; got empty"
    );
    let s = String::from_utf8_lossy(&body);
    let line_count = s.lines().count();
    assert_eq!(
        line_count, 1,
        "expected exactly one deprecation line per invocation; got {line_count}: {s}"
    );
}

/// S01 / AC-01: prior `mp watch` invocation does NOT suppress the
/// warning on a later canonical autopilot invocation (no side effects
/// carried across processes). Each process is its own emission scope.
#[test]
fn prior_watch_invocation_does_not_affect_autopilot_invocation() {
    let env = TestEnv::new();

    // First invocation is the legacy alias — warning fires.
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let watch_stderr = String::from_utf8_lossy(&watch.stderr);
    assert!(
        watch_stderr.contains("deprecated"),
        "first `mp watch` invocation must warn; got: {watch_stderr}"
    );

    // Subsequent canonical invocation in a fresh process — NO warning.
    let autopilot = run_autopilot(&env, &["start", "--dry-run", "--format", "json"]);
    let autopilot_stderr = String::from_utf8_lossy(&autopilot.stderr);
    assert!(
        !autopilot_stderr.contains("deprecated"),
        "canonical `mp autopilot start` must never warn; got: {autopilot_stderr}"
    );
}

// ─── AC-02: `mp autopilot --help` does NOT print the warning ─────────

/// S02 / AC-02: `mp autopilot --help` (the canonical command tree)
/// does NOT print the deprecation warning. Clap's auto-derived help
/// runs in the canonical path, and our deprecation hook only fires
/// for `Commands::Watch`, never for `Commands::Autopilot`.
#[test]
fn autopilot_help_does_not_print_deprecation_warning() {
    let env = TestEnv::new();
    let out = run_autopilot(&env, &["--help"]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("deprecated"),
        "`mp autopilot --help` must NOT print the deprecation warning; got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("mp watch"),
        "`mp autopilot --help` must NOT mention the legacy alias; got stderr: {stderr}"
    );
}

/// S02 / AC-02: `mp autopilot` (no subcommand) does NOT print the
/// deprecation warning either — clap's usage error surfaces on its
/// own without our warning polluting stderr.
#[test]
fn autopilot_no_subcommand_does_not_print_deprecation_warning() {
    let env = TestEnv::new();
    let out = run_autopilot(&env, &[]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("deprecated"),
        "`mp autopilot` (no subcommand) must NOT print the deprecation warning; got stderr: {stderr}"
    );
}

/// S02 / AC-02: every canonical autopilot subcommand that does not
/// involve the legacy alias (status, stop, output, result, session,
/// note, config, migrate) must remain warning-free. The hook is
/// scoped to `Commands::Watch` only — never to `Commands::Autopilot`.
#[test]
fn canonical_autopilot_subcommands_are_warning_free() {
    let env = TestEnv::new();
    // Each subcommand exercised with --help so we get a clean exit
    // without exercising real plan state. The deprecation warning
    // must never leak into any of these.
    for sub in [
        "start",
        "status",
        "stop",
        "output",
        "result",
        "session",
        "note",
        "config",
        "migrate",
    ] {
        let out = run_autopilot(&env, &[sub, "--help"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("deprecated"),
            "`mp autopilot {sub} --help` must NOT print the deprecation warning; got stderr: {stderr}"
        );
    }
}
