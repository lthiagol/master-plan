//! M233 / AC-06: negative integration tests driving the `?`-propagated
//! error paths in `crates/mp/src/digest.rs` to non-zero exits.
//!
//! Three tests covering three distinct production bail sites:
//!
//!   1. `digest_rejects_conflicting_since_handoff_and_days_flags`
//!      — `mp digest --since-handoff --days 7` hits the
//!      `bail!("use only one of --since-handoff, --since, or --days")`
//!      arm at digest.rs:31.
//!   2. `digest_since_handoff_without_prior_handoff_exits_with_no_handoff_message`
//!      — `mp digest --since-handoff` on a fresh plan (no handoff
//!      recorded) hits the `bail!("no handoff recorded; ...")` at
//!      digest.rs:92.
//!   3. `digest_rejects_invalid_since_duration_string`
//!      — `mp digest --since not-a-duration` hits the
//!      `.with_context(|| format!("invalid --since {since}; ..."))`
//!      at digest.rs:307. The pre-existing
//!      `digest_rejects_invalid_since` test in suites/digest.rs
//!      covers the same exit-code surface; this test additionally
//!      asserts on the parse-error wording so the AC's negative
//!      path is documented as a single grep-able error class.
//!
//! **No production-code changes** — these tests only assert on the
//! already-existing error branches.

mod common;

use crate::common::TestEnv;

#[test]
fn digest_rejects_conflicting_since_handoff_and_days_flags() {
    let env = TestEnv::new();
    let out = env.run(&[
        "digest",
        "--since-handoff",
        "--days",
        "7",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "digest with both --since-handoff and --days must exit non-zero; stderr={stderr}"
    );
    // The literal wording at digest.rs:31.
    assert!(
        stderr.contains("use only one of --since-handoff, --since, or --days"),
        "stderr must surface the exclusivity bail; got: {stderr}"
    );
}

#[test]
fn digest_since_handoff_without_prior_handoff_exits_with_no_handoff_message() {
    let env = TestEnv::new();
    let out = env.run(&["digest", "--since-handoff", "--format", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "digest --since-handoff on a fresh plan must exit non-zero; stderr={stderr}"
    );
    // The literal wording at digest.rs:92.
    assert!(
        stderr.contains("no handoff recorded"),
        "stderr must surface the 'no handoff recorded' bail; got: {stderr}"
    );
}

#[test]
fn digest_rejects_invalid_since_duration_string() {
    let env = TestEnv::new();
    let out = env.run(&[
        "digest",
        "--since",
        "not-a-duration",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "digest with invalid --since must exit non-zero; stderr={stderr}"
    );
    // The wording produced by the parse-error bail at digest.rs:307.
    assert!(
        stderr.contains("invalid --since"),
        "stderr must surface the parse-error wording; got: {stderr}"
    );
    assert!(
        stderr.contains("not-a-duration"),
        "stderr must echo the offending input; got: {stderr}"
    );
}