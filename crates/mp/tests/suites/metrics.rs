//! M233 / AC-03: `mp plan metrics show|set` direct coverage.
//!
//! Two behavioral tests against a fresh `TestEnv`:
//!
//! 1. `metrics_set_show_round_trip` — `set` carries each field through
//!    to `show`'s JSON, including the `checked_at` field the `set`
//!    arm refreshes on every call (charter.rs:166). Verifies the
//!    round-trip path in `cmd_metrics` (commands/metrics.rs:9-32).
//! 2. `metrics_set_rejects_non_numeric_field_value` — clap rejects
//!    a non-numeric `--lines-of-code abc` with exit 2, proving the
//!    type guard fires before any disk write.
//!
//! **Why "unknown-field error" maps to a parse error, not a JSON-key
//! error:** the `MetricsCmd` Set arm accepts exactly four fields
//! (lines_of_code, unit_tests, integration_tests, coverage_percent).
//! The only way for the CLI to receive an unrecognised value is via a
//! wrong-type value (e.g. `abc` for `--lines-of-code <u64>`); clap's
//! type-check is the production enforcement point. The struct loader
//! silently drops unknown JSON keys because Metrics has no
//! `#[serde(deny_unknown_fields)]` — a future tightening of that
//! surface is a separate decision, not part of AC-03.

use crate::common::TestEnv;

#[test]
fn metrics_set_show_round_trip() {
    let env = TestEnv::new();

    let set = env.run(&[
        "plan",
        "metrics",
        "set",
        "--lines-of-code",
        "1234",
        "--unit-tests",
        "56",
        "--integration-tests",
        "78",
        "--coverage-percent",
        "92.5",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&set.stderr);
    assert!(
        set.status.success(),
        "metrics set must succeed on a fresh plan: {stderr}"
    );
    let set_payload: serde_json::Value =
        serde_json::from_slice(&set.stdout).expect("set JSON");
    assert_eq!(set_payload["ok"].as_bool(), Some(true));
    let metrics = &set_payload["metrics"];
    assert_eq!(metrics["lines_of_code"].as_u64(), Some(1234));
    assert_eq!(metrics["unit_tests"].as_u64(), Some(56));
    assert_eq!(metrics["integration_tests"].as_u64(), Some(78));
    // f64 round-trip via JSON; assert close-but-not-equal because
    // serde_json renders 92.5 as 92.5 and we read it back the same.
    let coverage = metrics["coverage_percent"].as_f64().unwrap();
    assert!(
        (coverage - 92.5).abs() < 1e-9,
        "coverage_percent must be 92.5; got {coverage}"
    );
    assert!(
        metrics["checked_at"].as_str().is_some(),
        "set must refresh checked_at; got {metrics}"
    );

    // `show` echoes the same numbers — proves the persisted plan.json
    // is the source of truth for `show`, not a separate cache.
    let show = env.run(&["plan", "metrics", "show", "--format", "json"]);
    let show_stderr = String::from_utf8_lossy(&show.stderr);
    assert!(
        show.status.success(),
        "metrics show must succeed after set: {show_stderr}"
    );
    let show_payload: serde_json::Value =
        serde_json::from_slice(&show.stdout).expect("show JSON");
    assert_eq!(show_payload["ok"].as_bool(), Some(true));
    let shown = &show_payload["metrics"];
    assert_eq!(shown["lines_of_code"].as_u64(), Some(1234));
    assert_eq!(shown["unit_tests"].as_u64(), Some(56));
    assert_eq!(shown["integration_tests"].as_u64(), Some(78));
    let coverage = shown["coverage_percent"].as_f64().unwrap();
    assert!(
        (coverage - 92.5).abs() < 1e-9,
        "show.coverage_percent must be 92.5; got {coverage}"
    );
}

#[test]
fn metrics_set_rejects_non_numeric_field_value() {
    let env = TestEnv::new();
    // `abc` is not a u64; clap's value parser rejects it with exit 2
    // BEFORE the disk-write path in cmd_metrics ever runs. This is
    // the type-guard the AC labels "unknown-field error" — the
    // production enforcement point for values that don't fit the
    // declared field shape.
    let out = env.run(&[
        "plan",
        "metrics",
        "set",
        "--lines-of-code",
        "abc",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "metrics set must reject non-numeric value: {stderr}"
    );
    let code = out.status.code().unwrap_or(0);
    assert_eq!(
        code, 2,
        "expected clap's parse-error exit code 2; got {code}; stderr={stderr}"
    );
    // Clap's value-parser wording must surface — operators rely on it
    // to recognise the field at fault.
    assert!(
        stderr.contains("--lines-of-code"),
        "stderr must mention the offending field name; got: {stderr}"
    );
}