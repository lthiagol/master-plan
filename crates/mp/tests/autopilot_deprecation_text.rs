//! M219 / S03: `mp watch` deprecation message text.
//!
//! AC-03 pins the deprecation message byte-for-byte so downstream
//! tooling (warning scrapers, user training, alerting) can rely on
//! the exact wording. Any drift in the message text is a regression.

mod common;

use common::TestEnv;

/// The exact text the deprecation hook must print. Kept as a
/// constant so the test failure message shows both expected and
/// observed values on a regression.
const DEPRECATION_LINE: &str = "mp watch is deprecated; use 'mp autopilot' instead.";

fn run_watch(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["watch".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

fn run_autopilot_start(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["autopilot".to_string(), "start".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

/// Strip the single trailing newline so we can compare the body
/// byte-for-byte. Clap / Rust's `eprintln!` appends exactly one `\n`.
fn strip_trailing_newline(s: &[u8]) -> &[u8] {
    if s.last() == Some(&b'\n') {
        &s[..s.len() - 1]
    } else {
        s
    }
}

/// AC-03 / S03: the deprecation message text matches the documented
/// string byte-for-byte (modulo the trailing newline that `eprintln!`
/// adds). This pins the wording so user-facing tools, scripts, and
/// documentation can rely on the exact line.
#[test]
fn deprecation_message_matches_documented_string_byte_for_byte() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let autopilot = run_autopilot_start(&env, &["--dry-run", "--format", "json"]);

    // Canonical `mp autopilot start` must NOT print the line at all.
    let autopilot_stderr = strip_trailing_newline(&autopilot.stderr);
    assert!(
        autopilot_stderr.is_empty(),
        "canonical `mp autopilot start` must produce empty stderr; got: {}",
        String::from_utf8_lossy(autopilot_stderr)
    );

    // `mp watch` stderr body must equal the documented message
    // byte-for-byte (trailing newline stripped).
    let watch_body = strip_trailing_newline(&watch.stderr);
    assert_eq!(
        watch_body,
        DEPRECATION_LINE.as_bytes(),
        "`mp watch` stderr body must match the documented deprecation line byte-for-byte"
    );
}

/// AC-03 / S03: every variant of `mp watch` invocation prints the
/// exact same message text — including `mp watch` with no args and
/// with arbitrary positional ids. The wording is invariant across
/// invocation shape.
#[test]
fn deprecation_message_is_invariant_across_invocation_shapes() {
    let env = TestEnv::new();

    // No ids, dry-run only.
    let out_a = run_watch(&env, &["--dry-run", "--format", "json"]);
    let a = strip_trailing_newline(&out_a.stderr);
    assert_eq!(a, DEPRECATION_LINE.as_bytes());

    // With a positional id (whatever — dry-run never resolves it).
    let out_b = run_watch(&env, &["--dry-run", "999999", "--format", "json"]);
    let b = strip_trailing_newline(&out_b.stderr);
    assert_eq!(b, DEPRECATION_LINE.as_bytes());
}

/// AC-03 / S03: the documented message uses single quotes around
/// `mp autopilot` (not backticks, not double quotes). This test
/// pins that quote style so a stylistic drift is caught.
#[test]
fn deprecation_message_uses_single_quotes_around_autopilot() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let body = String::from_utf8_lossy(strip_trailing_newline(&watch.stderr)).into_owned();

    // Must contain `'mp autopilot'` with single quotes.
    assert!(
        body.contains("'mp autopilot'"),
        "deprecation line must quote `mp autopilot` with single quotes; got: {body}"
    );

    // Must NOT use backticks around `mp autopilot` (M208 wording).
    assert!(
        !body.contains("`mp autopilot`"),
        "deprecation line must not use backticks around `mp autopilot`; got: {body}"
    );

    // Must NOT use double quotes around `mp autopilot`.
    assert!(
        !body.contains("\"mp autopilot\""),
        "deprecation line must not use double quotes around `mp autopilot`; got: {body}"
    );

    // The full line ends with a period — the message is a complete
    // sentence, not a fragment.
    assert!(
        body.ends_with('.'),
        "deprecation line must end with a period; got: {body}"
    );
}
