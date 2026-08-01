//! ac_verify subprocess timeout — hung commands must fail within bounded wall clock.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use mp::ac_verify::run_one_in;
use mp::model::AcceptanceCriterion;

static TIMEOUT_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn hung_verification_times_out() {
    let _lock = TIMEOUT_ENV_LOCK.lock().unwrap();
    std::env::set_var("MP_VERIFY_TIMEOUT_SECS", "2");

    let ac = AcceptanceCriterion {
        id: "AC-01".to_string(),
        description: "should time out".to_string(),
        verification: "sleep 30".to_string(),
        status: "pending".to_string(),
        evidence: String::new(),
    };

    let started = std::time::Instant::now();
    // M107 (S3): cancellation plumbing is opaque to this timeout test
    // — pass default stubs so the timeout behavior under test is
    // unchanged. (Cooperative cancel doesn't apply; the per-AC timeout
    // is what fires first.)
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let result = run_one_in(&ac, None, &cancelled, &child_pids, None);
    let elapsed = started.elapsed();

    std::env::remove_var("MP_VERIFY_TIMEOUT_SECS");

    assert!(!result.passed, "hung command should not pass");
    assert!(
        result.note.contains("timed out"),
        "expected timeout note, got: {}",
        result.note
    );
    assert!(
        elapsed.as_secs() < 10,
        "timeout should fire quickly, took {:?}",
        elapsed
    );
}
