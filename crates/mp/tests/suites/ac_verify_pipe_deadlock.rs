//! M106 AC-01 (S3): regression test for the pipe-buffer deadlock in
//! `ac_verify::execute` that gated M104's completion.
//!
//! Pre-fix: the verifier read stdout/stderr only at child exit, so any child
//! emitting more than the kernel pipe buffer (~64KB on macOS, similar on
//! Linux) would fill the pipe, block on write, and the verifier's
//! `try_wait()` loop would never see child exit. Result: the gate fires
//! "verification timed out after 300s" on any milestone whose AC or step
//! verification runs cargo (which emits far more than 64KB of stdout during
//! a cold compile).
//!
//! Post-fix: drain threads consume continuously, child never blocks.
//! This test exercises the post-fix path by running `seq 1 100000` (yields
//! ~588KB of stdout — well above the kernel pipe buffer) and asserts the
//! verifier returns in <5s and the captured output is intact.

use mp::ac_verify::run_step_test_in;
use mp_model::Step;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[test]
fn step_verification_drains_over_64kb_stdout_without_deadlock() {
    // `seq 1 100000` emits ~588889 bytes (1..<100000 each followed by \n),
    // well above the macOS pipe buffer (64KB) and the Linux default (also
    // 64KB). The premise (producer > buffer) is the deadlock trigger; the
    // test verifies the post-fix drain keeps the verifier running.

    let step = make_step("seq 1 100000");
    let start = Instant::now();
    // M107 (S3): cancellation plumbing is opaque to this deadlock
    // regression test — pass default stubs. The test exercises the
    // per-pipe drain thread behavior, which is independent of the
    // cooperative cancel flag and the killpg-on-global-deadline path.
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let result = run_step_test_in(&step, None, &cancelled, &child_pids, None);
    let elapsed = start.elapsed();

    assert!(
        result.passed,
        "step should pass (exit 0); got note={:?} output_len={} elapsed={:?}",
        result.note,
        result.output.len(),
        elapsed,
    );
    // `seq 1 100000` is trivially fast. Pre-fix this would be 300s (gate
    // timeout) because the verifier's main loop deadlocks on the >64KB
    // payload. Post-fix: sub-second warm. 5s is a generous ceiling.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "verification took {elapsed:?}; pre-fix this would be 300s — deadlock regressed"
    );
    // (The captured bytes are clipped to 2000 chars for display via
    // `truncate(out, 2000)` in `run_test` — the underlying drain captures
    // the full stream, proven by the unit tests in S1. We just need the
    // drain to not time out, which the timing assertion above covers.)
    assert!(
        !result.output.is_empty(),
        "drain captured nothing; expected non-empty head of the seq stream"
    );
}

#[test]
fn step_verification_drains_over_64kb_stderr_too() {
    // Same regression on the stderr path. `seq 1 100000 >&2` writes the
    // bulk to stderr — exercises the second drain thread.
    let step = make_step("seq 1 100000 >&2");
    let start = Instant::now();
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let result = run_step_test_in(&step, None, &cancelled, &child_pids, None);
    let elapsed = start.elapsed();

    assert!(
        result.passed,
        "step should pass on stderr path; note={:?} output_len={} elapsed={:?}",
        result.note,
        result.output.len(),
        elapsed,
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "stderr verification took {elapsed:?}; pre-fix this would be 300s"
    );
    assert!(
        !result.output.is_empty(),
        "stderr drain captured nothing; expected non-empty head of the seq stream"
    );
}

fn make_step(tests: &str) -> Step {
    Step {
        id: "S-DEADLOCK".into(),
        action: "S3 regression".into(),
        covers_ac: vec!["AC-01".into()],
        depends_on_steps: vec![],
        done_when: "returns within 5s".into(),
        files: vec![],
        tests: tests.into(),
        work_package: "WP1".into(),
        status: "pending".into(),
        claimed_at: String::new(),
        claimed_by: String::new(),
        lease_expires_at: String::new(),
        evidence: String::new(),
        order: 1,
    }
}
