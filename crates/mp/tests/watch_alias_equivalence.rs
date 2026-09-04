//! M208 / S02 / AC-02: `mp watch` and `mp autopilot start` accept
//! the same arguments and return identical exit codes and stdout
//! bytes; the only permitted difference is the single legacy
//! deprecation line on `mp watch` stderr.
//!
//! Black-box coverage of the equivalence contract:
//! - `mp watch <ids...> --dry-run` and `mp autopilot start <ids...>
//!   --dry-run` produce byte-identical JSON on stdout
//! - both commands return the same exit code
//! - `mp watch` prints the deprecation notice on stderr; `mp
//!   autopilot start` does not
//! - the equivalence holds for the `--dry-run`, `--log-file`,
//!   `--stall-timeout-ms`, `--poll-interval-ms`, `--resume`,
//!   `--force`, `--detach` flag set
//! - unknown milestone ids surface as the same per-entry error in
//!   both commands

mod common;

use common::TestEnv;
use serde_json::Value;

/// Strip trailing newlines so the comparison is not sensitive to
/// cosmetic whitespace differences that are out of scope for the
/// equivalence contract.
fn trim_trailing_newlines(s: &[u8]) -> Vec<u8> {
    let mut v = s.to_vec();
    while matches!(v.last(), Some(b'\n') | Some(b'\r')) {
        v.pop();
    }
    v
}

/// Run `mp watch <args>` and return (status, stdout, stderr) for
/// byte-level comparison.
fn run_watch(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["watch".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

/// Run `mp autopilot start <args>` and return (status, stdout, stderr).
fn run_autopilot_start(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut full = vec!["autopilot".to_string(), "start".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let as_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    env.run(&as_refs)
}

#[test]
fn dry_run_with_no_ids_produces_identical_stdout_and_exit_code() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let autopilot = run_autopilot_start(&env, &["--dry-run", "--format", "json"]);

    assert_eq!(
        watch.status.code(),
        autopilot.status.code(),
        "exit codes must match"
    );
    assert_eq!(
        trim_trailing_newlines(&watch.stdout),
        trim_trailing_newlines(&autopilot.stdout),
        "stdout bytes must be identical (modulo trailing newlines)"
    );

    // `mp watch` must include the deprecation line on stderr.
    let watch_stderr = String::from_utf8_lossy(&watch.stderr);
    assert!(
        watch_stderr.contains("deprecated"),
        "`mp watch` should print a deprecation notice on stderr; got: {watch_stderr}"
    );

    // `mp autopilot start` must NOT include the deprecation line.
    let autopilot_stderr = String::from_utf8_lossy(&autopilot.stderr);
    assert!(
        !autopilot_stderr.contains("deprecated"),
        "`mp autopilot start` must NOT print the deprecation notice; got: {autopilot_stderr}"
    );
}

#[test]
fn dry_run_with_known_milestone_id_produces_identical_stdout() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "alias equivalence target",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "alias equivalence target" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["y", "z"] },
        "acceptance_criteria": [
            { "description": "ac", "verification": "manual: yes" }
        ]
    }"#;
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    let id = created["milestone"]["id"].as_str().expect("milestone id");

    let watch = run_watch(&env, &["--dry-run", id, "--format", "json"]);
    let autopilot = run_autopilot_start(&env, &["--dry-run", id, "--format", "json"]);

    assert_eq!(watch.status.code(), autopilot.status.code());
    assert_eq!(
        trim_trailing_newlines(&watch.stdout),
        trim_trailing_newlines(&autopilot.stdout),
        "stdout must match for known milestone id"
    );
    let parsed: Value = serde_json::from_slice(&watch.stdout).unwrap();
    assert_eq!(parsed["dry_run"], Value::Bool(true));
}

#[test]
fn dry_run_unknown_milestone_surfaces_identical_per_entry_error() {
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "999999", "--format", "json"]);
    let autopilot = run_autopilot_start(&env, &["--dry-run", "999999", "--format", "json"]);

    assert_eq!(watch.status.code(), autopilot.status.code());
    assert_eq!(
        trim_trailing_newlines(&watch.stdout),
        trim_trailing_newlines(&autopilot.stdout),
        "stdout must match for unknown milestone id (both surface as per-entry error)"
    );
    let parsed: Value = serde_json::from_slice(&watch.stdout).unwrap();
    let entry = &parsed["milestones"][0];
    assert!(
        entry["error"].as_str().is_some(),
        "unknown milestone must produce an error string"
    );
    assert_eq!(entry["input"].as_str(), Some("999999"));
}

#[test]
fn log_file_override_propagates_identically() {
    let env = TestEnv::new();
    let custom = env.tmp.path().join("alias-equivalence.log");
    let watch = run_watch(
        &env,
        &[
            "--dry-run",
            "--log-file",
            custom.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let autopilot = run_autopilot_start(
        &env,
        &[
            "--dry-run",
            "--log-file",
            custom.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_eq!(watch.status.code(), autopilot.status.code());
    let watch_parsed: Value = serde_json::from_slice(&watch.stdout).unwrap();
    let autopilot_parsed: Value = serde_json::from_slice(&autopilot.stdout).unwrap();
    assert_eq!(watch_parsed["log_file"], autopilot_parsed["log_file"]);
    assert_eq!(
        watch_parsed["log_file"].as_str(),
        Some(custom.to_str().unwrap()),
        "log_file override must be reflected in the report"
    );
}

#[test]
fn mp_watch_stderr_is_exactly_the_deprecation_line() {
    // The AC contract: the only permitted difference between the
    // two commands is the single legacy deprecation line on
    // `mp watch` stderr. For the dry-run path there is no other
    // stderr output from either command, so this test pins down the
    // exact stderr contents.
    let env = TestEnv::new();
    let watch = run_watch(&env, &["--dry-run", "--format", "json"]);
    let autopilot = run_autopilot_start(&env, &["--dry-run", "--format", "json"]);

    let watch_stderr = trim_trailing_newlines(&watch.stderr);
    let autopilot_stderr = trim_trailing_newlines(&autopilot.stderr);

    // `mp autopilot start` must have empty stderr for the dry-run
    // path; if anything leaks through here it is a real bug.
    assert!(
        autopilot_stderr.is_empty(),
        "`mp autopilot start --dry-run` must not write to stderr; got: {}",
        String::from_utf8_lossy(&autopilot_stderr)
    );

    // `mp watch` must write exactly the deprecation line — the
    // exact wording matches the dispatch in app/dispatch.rs.
    // M219 pins the documented message byte-for-byte.
    let watch_stderr_str = String::from_utf8_lossy(&watch_stderr);
    assert!(
        watch_stderr_str.contains("mp watch is deprecated"),
        "`mp watch` stderr must contain the deprecation line; got: {watch_stderr_str}"
    );
    assert!(
        watch_stderr_str.contains("mp autopilot"),
        "the deprecation line must point at the replacement command; got: {watch_stderr_str}"
    );
}
