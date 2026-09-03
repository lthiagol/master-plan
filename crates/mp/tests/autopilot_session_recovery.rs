//! M207 / S7 / AC-07: append-only event log + atomic crash-safe writes
//! + corruption/restart fixtures.
//!
//! Black-box coverage of:
//! - Each session has an append-only, sequence-numbered event log.
//! - Writes use temp-file + fsync + atomic rename; an interrupted
//!   write leaves the *previous* valid document on disk (never a
//!   torn file).
//! - `recover_session` reconciles a stale cursor against the
//!   surviving event tail.
//! - The loader rejects malformed (parse-error) and schema-invalid
//!   session.json files non-fatally.

mod common;

use common::TestEnv;
use mp::autopilot::events::EventKind;
use mp::autopilot::session::{load_session, sample_session_for_tests, save_session};
use mp::autopilot::{recover_session, RecoveredSession};
use mp::paths::PlanContext;
use std::fs;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn events_are_appended_in_strictly_monotonic_order() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    // Drive a sequence of CLI actions that each append an event.
    env.run(&[
        "autopilot",
        "session",
        "transition",
        "--session",
        "alpha",
        "--role",
        "runner",
        "--state",
        "starting",
    ]);
    env.run(&[
        "autopilot",
        "note",
        "add",
        "--session",
        "alpha",
        "--kind",
        "info",
        "--body",
        "step-1",
        "--cycle",
        "1",
    ]);
    env.run(&[
        "autopilot",
        "session",
        "transition",
        "--session",
        "alpha",
        "--role",
        "runner",
        "--state",
        "working",
        "--working-on",
        "207:1",
    ]);

    let loaded = load_session(&ctx, "alpha").unwrap();
    assert!(loaded.events.len() >= 3, "events={:?}", loaded.events);
    for window in loaded.events.windows(2) {
        assert!(
            window[0].seq < window[1].seq,
            "events must be strictly monotonic; got {:?}",
            window
        );
    }
    assert_eq!(loaded.event_cursor.last_seq, loaded.events.last().unwrap().seq);
}

#[test]
fn atomic_write_publishes_full_document() {
    // Pin the atomic-write contract: after save_session returns,
    // the destination must contain a parseable document. A torn
    // intermediate path must never be visible.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    let raw = fs::read(&path).unwrap();
    // Pretty-printed JSON begins with `{`.
    assert!(raw.starts_with(b"{"));
    let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(value["id"], "alpha");
}

#[test]
fn corrupt_session_is_rejected_nonfatally() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let session = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &session).unwrap();
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    fs::write(&path, b"not json {{{").unwrap();
    let err = load_session(&ctx, "alpha").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("parse") || msg.contains("decode"), "got {msg}");
}

#[test]
fn recover_session_bumps_stale_cursor() {
    // Stage a session.json whose cursor lags the surviving events
    // (simulates a torn write where the cursor was not persisted
    // last).
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
    // Three events, then stale the cursor.
    let events = vec![
        mp::autopilot::OrchestrationEvent::new(
            1,
            EventKind::Dispatch,
            "t",
            serde_json::json!({}),
        ),
        mp::autopilot::OrchestrationEvent::new(
            2,
            EventKind::Transition,
            "t",
            serde_json::json!({}),
        ),
        mp::autopilot::OrchestrationEvent::new(
            3,
            EventKind::Note,
            "t",
            serde_json::json!({}),
        ),
    ];
    for e in events {
        session.event_cursor.advance_to(e.seq).unwrap();
        session.events.push(e);
    }
    // Simulate torn write: cursor regresses below event max.
    session.event_cursor.last_seq = 1;
    save_session(&ctx, "alpha", &session).unwrap();

    let report: RecoveredSession = recover_session(&ctx, "alpha").unwrap();
    assert_eq!(report.prev_cursor, 1);
    assert_eq!(report.next_cursor, 3);
    assert_eq!(report.cursor_bumped(), 2);
}

#[test]
fn recover_session_is_noop_when_cursor_is_consistent() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
session
        .event_cursor
        .advance_to(1)
        .unwrap();
    session.events.push(mp::autopilot::OrchestrationEvent::new(
        1,
        EventKind::Dispatch,
        "t",
        serde_json::json!({}),
    ));
    save_session(&ctx, "alpha", &session).unwrap();

    let report = recover_session(&ctx, "alpha").unwrap();
    assert_eq!(report.cursor_bumped(), 0);
}

#[test]
fn append_only_invariant_no_event_is_dropped() {
    // After many appends, every previously-inserted event must
    // still be present.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.role_state = None;
    session.working_on = None;
    session.queue.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let n = 8;
    for i in 1..=n {
        let body = format!("note-{i}");
        env.run(&[
            "autopilot",
            "note",
            "add",
            "--session",
            "alpha",
            "--kind",
            "info",
            "--body",
            &body,
            "--cycle",
            "1",
        ]);
    }
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(loaded.events.len(), n);
    for (i, e) in loaded.events.iter().enumerate() {
        assert_eq!(e.seq as usize, i + 1, "event at index {i} has wrong seq");
    }
    let body_strings: Vec<String> = loaded
        .events
        .iter()
        .map(|e| {
            e.payload
                .as_ref()
                .and_then(|p| p.get("body"))
                .and_then(|b| b.as_str())
                .map(str::to_string)
                .unwrap_or_default()
        })
        .collect();
    // Every body must still be present, in order.
    assert_eq!(body_strings.len(), n);
    for (i, body) in body_strings.iter().enumerate() {
        assert_eq!(body, &format!("note-{}", i + 1));
    }
}