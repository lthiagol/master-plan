//! M207 / S4 / AC-04: typed runner notes with cycle derivation.
//!
//! Black-box coverage of:
//! - `mp autopilot note add` accepts `--kind`, `--body`, optional
//!   `--cycle`, optional `--milestone`.
//! - Cycle is required or derived from the session's active queue
//!   item; ambiguous notes are rejected (no implicit cycle 1).
//! - Notes are persisted into `runner_notes` and an event is appended
//!   to the session's event log.

mod common;

use common::TestEnv;
use mp::autopilot::notes::{build_note, derive_cycle, NoteError, NoteKind};
use mp::autopilot::session::{load_session, save_session, QueueItem, Stage};
use mp::paths::PlanContext;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn note_add_with_explicit_cycle_persists() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = mp::autopilot::sample_session_for_tests("alpha");
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "info",
        "--body",
        "explicit-cycle note",
        "--cycle",
        "3",
    ]);
    assert!(
        out.status.success(),
        "note add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.runner_notes.len(), 1);
    let note = &loaded.runner_notes[0];
    assert_eq!(note.cycle, 3);
    assert_eq!(note.kind, NoteKind::Info);
    assert_eq!(note.body, "explicit-cycle note");
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].kind, mp::autopilot::EventKind::Note);
}

#[test]
fn note_add_without_cycle_derives_from_session_context() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = mp::autopilot::sample_session_for_tests("alpha");
    // The sample has working_on set with cycle=1; the note should
    // pick that up automatically.
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "warn",
        "--body",
        "auto-cycle note",
    ]);
    assert!(
        out.status.success(),
        "note add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.runner_notes[0].cycle, 1);
    assert_eq!(loaded.runner_notes[0].kind, NoteKind::Warn);
}

#[test]
fn note_add_rejected_when_no_cycle_context() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = mp::autopilot::sample_session_for_tests("alpha");
    // Clear both working_on and queue so derivation has nothing
    // to anchor on.
    session.working_on = None;
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "info",
        "--body",
        "no-context note",
    ]);
    assert!(
        !out.status.success(),
        "expected rejection; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn note_add_rejected_when_cycle_is_zero() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = mp::autopilot::sample_session_for_tests("alpha");
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "info",
        "--body",
        "zero-cycle",
        "--cycle",
        "0",
    ]);
    assert!(!out.status.success(), "expected rejection for cycle=0");
}

#[test]
fn note_add_rejected_when_body_is_empty() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = mp::autopilot::sample_session_for_tests("alpha");
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "info",
        "--body",
        "    ",
        "--cycle",
        "1",
    ]);
    assert!(!out.status.success(), "expected rejection for empty body");
}

#[test]
fn note_add_rejected_when_kind_unknown() {
    let env = TestEnv::new();
    let out = env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "bogus-kind",
        "--body",
        "x",
        "--cycle",
        "1",
    ]);
    assert!(!out.status.success(), "expected rejection for unknown kind");
}

#[test]
fn library_derive_cycle_rejects_ambiguous() {
    // Library-level: two in-progress queue items with different
    // cycles — derivation must reject.
    let mut s = mp::autopilot::AutopilotSession::blank("s1");
    s.queue.push(QueueItem {
        milestone_id: "01".to_string(),
        stage: Stage::InProgress,
        cycle: 1,
        last_notify: None,
        verifier_verdict: None,
        evidence_refs: None,
    });
    s.queue.push(QueueItem {
        milestone_id: "02".to_string(),
        stage: Stage::InProgress,
        cycle: 2,
        last_notify: None,
        verifier_verdict: None,
        evidence_refs: None,
    });
    let err = derive_cycle(&s, None).unwrap_err();
    assert_eq!(err, NoteError::AmbiguousCycle);
}

#[test]
fn library_build_note_attaches_session_milestone() {
    let mut s = mp::autopilot::AutopilotSession::blank("s1");
    s.queue.push(QueueItem {
        milestone_id: "207".to_string(),
        stage: Stage::InProgress,
        cycle: 1,
        last_notify: None,
        verifier_verdict: None,
        evidence_refs: None,
    });
    let note = build_note(&s, NoteKind::Info, "ok", Some(1), None).unwrap();
    assert_eq!(note.milestone_id.as_deref(), Some("207"));
    assert_eq!(note.cycle, 1);
}
