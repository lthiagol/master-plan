//! M106 AC-05 (S13): regression test for the drain-thread join deadlock in
//! `ac_verify::execute`'s timeout path.
//!
//! Pre-fix: when a verifier invocation exceeded the per-AC timeout
//! (`MP_VERIFY_TIMEOUT_SECS`), the verifier spawned `child.kill()` +
//! `child.wait()` to reap the child, then `let _ = h.join();` on the
//! drain threads. On macOS, the reader thread could remain blocked in
//! `read()` after the writer's pipe end was closed — `JoinHandle::join`
//! has no interrupt path, so the verifier hung indefinitely past its own
//! timeout.
//!
//! Post-fix: `execute()` polls `JoinHandle::is_finished()` against a ~2s
//! deadline before joining; on overflow, the handle is dropped (thread
//! detached, runs to natural exit). The verifier returns cleanly.
//!
//! This test exercises the post-fix path by running a `sleep 30` step
//! with `MP_VERIFY_TIMEOUT_SECS=2` and asserts that the verifier returns
//! in ≤ 8s total wall time (2s timeout + 2s bounded join slack + budget).

use mp::ac_verify::run_step_test_in;
use mp_model::Step;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn verify_returns_promptly_when_step_exceeds_timeout() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MP_VERIFY_TIMEOUT_SECS", "2");

    let step = sleep_step();
    let start = Instant::now();
    // M107 (S3): cancellation plumbing is opaque to the bounded-join
    // timeout regression test. Pass default stubs; the test exercises
    // the per-AC timeout + bounded-join race, not the new global-
    // deadline path.
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let result = run_step_test_in(&step, None, &cancelled, &child_pids, None);
    let elapsed = start.elapsed();

    std::env::remove_var("MP_VERIFY_TIMEOUT_SECS");

    assert!(
        !result.passed,
        "30s sleep should not pass per-step gate (this test ensures the \
         timeout path actually fires; if it 'passes' the test isn't doing its job)"
    );
    assert!(
        result.note.contains("timed out"),
        "expected timeout note, got: {}",
        result.note
    );
    // With the bounded-join fix, wall time should be: per-AC timeout (~2s)
    // + bounded-join slack (~≤2s) + ms-level overhead. Budget 8s.
    assert!(
        elapsed < Duration::from_secs(8),
        "verifier took {elapsed:?}; the bounded-join fix should keep this ≤8s. \
         Pre-fix this would be 300s (the default MP_VERIFY_TIMEOUT_SECS)."
    );
}

fn sleep_step() -> Step {
    Step {
        id: "S-DEADLOCK-TIMEOUT".into(),
        action: "sleep beyond timeout".into(),
        covers_ac: vec!["AC-05".into()],
        depends_on_steps: vec![],
        done_when: "never".into(),
        files: vec![],
        tests: "sleep 30".into(),
        work_package: "WP-fix".into(),
        status: "pending".into(),
        claimed_at: String::new(),
        claimed_by: String::new(),
        lease_expires_at: String::new(),
        evidence: String::new(),
        order: 13,
    }
}
