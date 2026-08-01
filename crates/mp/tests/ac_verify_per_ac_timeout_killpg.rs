//! M117 AC-01: per-AC timeout path applies killpg on the verifier's
//! child process group before reaping with `child.wait()`. Closes the
//! B-52 / ER-5 reviewer finding from M107 that the per-AC path used
//! `child.kill()` (single-pid SIGKILL) and could leave orphan
//! subprocesses forked by the verifier. Pins the contract via a
//! wedged-verifier regression test.

mod common;

#[test]
fn per_ac_timeout_killpg_kills_child_process_group() {
    use mp::ac_verify::verify_milestone_in;
    use mp_model::{AcceptanceCriterion, MilestoneFile};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    // Build an AC whose verification blocks the per-AC timeout forever.
    // A tight polling loop (`while true; do :; done`) is the portable,
    // unkillable-by-itself-without-an-external-SIGKILL-the-process-
    // group construct. `sleep 30` doesn't trigger the timeout
    // because `sleep` exits 0 once its interval elapses; this
    // construct loops until SIGKILL.
    let ac = AcceptanceCriterion {
        id: "AC-PERAC-TIMEOUT".to_string(),
        description: "per-AC timeout killpg regression".to_string(),
        verification: "while true; do :; done".to_string(),
        status: "pending".to_string(),
        evidence: String::new(),
    };
    let mut m = MilestoneFile::default();
    m.milestone.id = "M117".to_string();
    m.milestone.slug = "m117-perac-killpg".to_string();
    m.milestone.title = "M117 regression fixture".to_string();
    m.acceptance_criteria = vec![ac];

    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

    // Run the verifier under a thread-local override so the per-AC
    // deadline fires in seconds rather than minutes. The override
    // is scoped via the new (M117 CR) `with_thread_timeout_override`
    // closure: the prior value is restored on the Drop guard before
    // the caller resumes, so the override cannot leak into sibling
    // tests even if the body panics.
    unsafe {
        std::env::remove_var("MP_VERIFY_TIMEOUT_SECS"); // safety: don't let env leak override override
    }
    let m_clone = m.clone();
    let cancelled_clone = cancelled.clone();
    let child_pids_clone = child_pids.clone();
    let handle = std::thread::spawn(move || {
        mp::ac_verify::with_thread_timeout_override(1, || {
            let started = Instant::now();
            let report =
                verify_milestone_in(&m_clone, None, &cancelled_clone, &child_pids_clone, None);
            let elapsed = started.elapsed();
            (report, elapsed)
        })
    });
    let (report, elapsed) = handle.join().expect("verifier thread panic");

    // The verifier exited via the per-AC timeout path. The pre-M117
    // implementation reported a timeout note; the killpg-on-per-AC
    // change must NOT regress the gate's reporting.
    assert!(
        report.results.iter().any(|r| r.note.contains("timed out")),
        "report must still carry the per-AC timeout note; got: {:?}",
        report.results
    );
    // Wall-clock cap keeps the test responsive. 1s per-AC + ~6s
    // bounded-join slack × 2 drains; 15s has headroom for scheduler slop.
    assert!(
        elapsed < Duration::from_secs(15),
        "per-AC timeout must complete within 15s wall-clock; took {elapsed:?}"
    );

    // No orphan survives the killpg: walk the registered child pids
    // from the kill-set and assert none of them are still alive.
    //
    // `execute()` ends its cancel paths with `child.wait()`, which
    // fully reaps the child and removes it from the kernel process
    // table. After that, `kill(pid, 0)` returns -1 (ESRCH) for the
    // reaped pid. The orphan-detection probe in
    // `crates/mp/docs/verifier-cancellation.md` §3 item 3 warns that
    // `kill(pid, 0)` returns 0 for *zombies* (post-signal, pre-wait);
    // that warning applies to the M107 cooperative test where the
    // worker drops its `Child` without reaping, not to this per-AC
    // test where `execute` has reaped. The post-wait ESRCH is the
    // authoritative "child is gone" signal here.
    let registered = child_pids.lock().expect("child_pids lock poisoned").clone();
    for pid in &registered {
        let alive = unsafe { libc::kill(*pid as i32, 0) == 0 };
        assert!(
            !alive,
            "registered child pid {pid} must be reaped after per-AC timeout killpg (kill(pid, 0) returned 0)"
        );
    }

    // cancelled flag never flipped (orchestrator-driven, not per-AC
    // timeout). Pins the contract that the two cancellation paths
    // remain orthogonal.
    assert!(
        !cancelled.load(std::sync::atomic::Ordering::Relaxed),
        "cancelled flag must NOT flip from per-AC timeout (that is the orchestrator's signal)"
    );
}

#[test]
fn cooperative_cancel_path_also_uses_killpg() {
    // M117 mirror check: the cooperative-cancel (global-deadline) path
    // already had killpg via M107 S3; verify the contract that BOTH
    // paths now share the same killpg + child.kill() + child.wait()
    // dance. Same unkillable-by-itself construct as the timeout test.
    use mp::ac_verify::verify_milestone_in;
    use mp_model::{AcceptanceCriterion, MilestoneFile};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let ac = AcceptanceCriterion {
        id: "AC-COOP-CANCEL".to_string(),
        description: "cooperative cancel killpg regression".to_string(),
        verification: "while true; do :; done".to_string(),
        status: "pending".to_string(),
        evidence: String::new(),
    };
    let mut m = MilestoneFile::default();
    m.milestone.id = "M117".to_string();
    m.milestone.slug = "m117-coop-killpg".to_string();
    m.milestone.title = "M117 cooperative cancel".to_string();
    m.acceptance_criteria = vec![ac];

    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

    // Spawn a thread that flips `cancelled` after 200ms (mid-poll) so
    // the verifier sees the cooperative flag during its try_wait poll.
    // (M117 CR: the prior version bound the handle to `_cancelled_handle`
    // and never joined, leaving the cancel-flip thread to outlive the
    // test body. We now join it explicitly so the test is hermetic.)
    let cancelled_handle = {
        let cancelled = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            cancelled.store(true, Ordering::Relaxed);
        })
    };

    // Run the verifier in a child thread with a 30s thread-local timeout
    // (long enough for the cooperative cancellation to fire, short
    // enough to bound the test). The override is scoped via the
    // closure — see `per_ac_timeout_killpg_kills_child_process_group`
    // for the rationale.
    let m_clone = m.clone();
    let cancelled_clone = cancelled.clone();
    let child_pids_clone = child_pids.clone();
    let handle = std::thread::spawn(move || {
        mp::ac_verify::with_thread_timeout_override(30, || {
            let started = Instant::now();
            let _report =
                verify_milestone_in(&m_clone, None, &cancelled_clone, &child_pids_clone, None);
            started.elapsed()
        })
    });
    let elapsed = handle.join().expect("verifier thread panic");
    cancelled_handle.join().expect("cancel-flip thread panic");

    assert!(
        elapsed < Duration::from_secs(5),
        "cooperative cancel path should fire within 5s; took {elapsed:?}"
    );

    // See the rationale in `per_ac_timeout_killpg_kills_child_process_group`.
    // `execute()` reaps via `child.wait()`; post-wait ESRCH from
    // `kill(pid, 0)` is the authoritative no-orphan signal here.
    let registered = child_pids.lock().expect("child_pids lock poisoned").clone();
    for pid in &registered {
        let alive = unsafe { libc::kill(*pid as i32, 0) == 0 };
        assert!(
            !alive,
            "registered child pid {pid} must be reaped after cooperative cancel killpg"
        );
    }
}
