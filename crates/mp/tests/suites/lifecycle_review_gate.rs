//! M196: the review gate. `mp milestone complete` on a non-track
//! milestone with no recorded `mp reviews pass --verdict ok` row
//! reaches `executed` (the executor's end-state), NOT terminal
//! `complete`. Tracks (`change_kind: "track"`) bypass the gate.
//! `--skip-review` is the recorded-debt escape hatch and writes
//! `[skip-review]` into evidence. `--force` does NOT bypass the
//! review gate (per F-01).
//!
//! These tests are the AC-01 + AC-02 contracts for the review gate.

use crate::common::TestEnv;

#[path = "lifecycle_review_gate_helpers.rs"]
mod helpers;
use helpers::{create_open_milestone, drive_to_in_progress, read_evidence, read_lifecycle};

/// AC-01: non-track milestone with no recorded review reaches
/// `executed` (the executor's end-state), NOT terminal `complete`.
/// This is the heart of the review gate.
#[test]
fn non_track_complete_without_review_lands_on_executed() {
    let env = TestEnv::new();
    let id = create_open_milestone(&env, None);
    drive_to_in_progress(&env, &id);

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "test evidence: no review recorded",
    ]);
    assert!(
        out.status.success(),
        "complete must succeed (it lands on `executed`, not refuse); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(read_lifecycle(&env, &id), "executed");
}

/// AC-01: a recorded `mp reviews pass --verdict ok` promotes the
/// milestone from `executed` to terminal `complete`. Without it,
/// the milestone stays at `executed`.
#[test]
fn reviews_pass_promotes_executed_to_complete() {
    let env = TestEnv::new();
    let id = create_open_milestone(&env, None);
    drive_to_in_progress(&env, &id);

    // First: complete without review → executed.
    let first = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "first: work done, awaiting review",
    ]);
    assert!(first.status.success());
    assert_eq!(read_lifecycle(&env, &id), "executed");

    // Then: reviewer passes → complete.
    let pass = env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "alice",
    ]);
    assert!(
        pass.status.success(),
        "reviews pass failed: {}",
        String::from_utf8_lossy(&pass.stderr)
    );
    assert_eq!(read_lifecycle(&env, &id), "complete");
}

/// AC-02: track fast-path bypasses the review gate. `change_kind:
/// track` reaches terminal `complete` without a review.
#[test]
fn track_kind_complete_skips_review_gate() {
    let env = TestEnv::new();
    let id = create_open_milestone(&env, Some("track"));
    drive_to_in_progress(&env, &id);

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "track fast-path: no review needed",
    ]);
    assert!(
        out.status.success(),
        "track complete must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(read_lifecycle(&env, &id), "complete");
}

/// AC-02: `--skip-review` is the recorded-debt escape hatch. It
/// bypasses the review gate but writes `[skip-review]` into evidence
/// so the bypass is auditable.
#[test]
fn skip_review_bypasses_gate_and_records_debt() {
    let env = TestEnv::new();
    let id = create_open_milestone(&env, None);
    drive_to_in_progress(&env, &id);

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: intentional skip-review for emergency deploy",
        "--skip-review",
    ]);
    assert!(out.status.success());
    assert_eq!(read_lifecycle(&env, &id), "complete");
    let evidence = read_evidence(&env, &id);
    assert!(
        evidence.contains("[skip-review:"),
        "evidence must carry the skip-review annotation; got: {evidence:?}"
    );
}

/// F-01: `--force` bypasses only the AC verification gate. It does
/// NOT bypass the review gate; a force-bypassed milestone still ends
/// at `executed` without a review or `--skip-review`.
#[test]
fn force_does_not_bypass_review_gate() {
    let env = TestEnv::new();
    let id = create_open_milestone(&env, None);
    drive_to_in_progress(&env, &id);

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "force-bypassed AC verify but no review",
        "--force",
    ]);
    assert!(out.status.success());
    assert_eq!(
        read_lifecycle(&env, &id),
        "executed",
        "--force alone must NOT reach terminal `complete`; needs a review or --skip-review"
    );
}
