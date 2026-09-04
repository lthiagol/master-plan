//! M217 / AC-05 — snapshot diffing for rendering only.
//!
//! The poller compares each fetched pair of payloads against the
//! last one it rendered and emits a **local UI** state-change
//! event carrying the session's sequence and revision. Two
//! properties matter:
//!
//! * **Unchanged snapshots cost nothing.** An idle drive polled
//!   every 2s must not re-render, and must not bump the dirty
//!   version that drives `run_loop`'s redraw.
//! * **The transition is legible.** sequence/revision are
//!   preserved verbatim, so a snapshot *older* than the one on
//!   screen is flagged (`stale_display`) instead of being
//!   rendered as forward progress.
//!
//! The events are UI-local: nothing here writes session state and
//! nothing relays progress to the orchestrator (AC-06 pins that
//! side of the contract mechanically).

use raul::mp_runner::MpRunner;
use raul::tui::app::App;
use raul::tui::poll::{poll_autopilot_lane, AutopilotPoller, PollOutcome, Snapshot};
use serde_json::{json, Value};

fn session(seq: u64, rev: u64) -> Value {
    json!({
        "session": {
            "id": "alpha",
            "sequence": seq,
            "revision": rev,
            "queue": [{"id": "217", "status": "in_progress"}],
        }
    })
}

fn status(run_state: &str) -> Value {
    json!({"run_state": {"kind": run_state}})
}

// ─── snapshot extraction ────────────────────────────────────────

#[test]
fn m217_ac05_snapshot_preserves_session_id_sequence_and_revision() {
    let s = Snapshot::from_payloads(&session(12, 3), &status("live"));
    assert_eq!(s.session_id, "alpha");
    assert_eq!(s.sequence, 12);
    assert_eq!(s.revision, 3);
}

#[test]
fn m217_ac05_snapshot_tolerates_an_unwrapped_envelope() {
    let unwrapped = json!({"id": "beta", "seq": 4, "rev": 2});
    let s = Snapshot::from_payloads(&unwrapped, &Value::Null);
    assert_eq!(s.session_id, "beta");
    assert_eq!(s.sequence, 4);
    assert_eq!(s.revision, 2);
}

#[test]
fn m217_ac05_snapshot_degrades_to_zero_rather_than_guessing() {
    let s = Snapshot::from_payloads(&Value::Null, &Value::Null);
    assert_eq!(s.session_id, "");
    assert_eq!(s.sequence, 0);
    assert_eq!(s.revision, 0);
}

// ─── diffing ────────────────────────────────────────────────────

#[test]
fn m217_ac05_first_observation_is_always_a_change() {
    let mut p = AutopilotPoller::new();
    let outcome = p.observe(&session(1, 1), &status("live"));
    let change = outcome
        .state_change()
        .expect("the first snapshot must render");
    assert_eq!(change.from_sequence, None, "there is no previous sequence");
    assert_eq!(change.to_sequence, 1);
    assert_eq!(change.from_revision, None);
    assert_eq!(change.to_revision, 1);
    assert!(!change.stale_display);
    assert_eq!(change.session_id, "alpha");
}

#[test]
fn m217_ac05_identical_snapshots_report_unchanged() {
    let mut p = AutopilotPoller::new();
    p.observe(&session(5, 2), &status("live"));
    for _ in 0..10 {
        assert_eq!(
            p.observe(&session(5, 2), &status("live")),
            PollOutcome::Unchanged,
            "an idle drive polled repeatedly must not re-render"
        );
    }
}

#[test]
fn m217_ac05_sequence_advance_reports_the_transition() {
    let mut p = AutopilotPoller::new();
    p.observe(&session(5, 2), &status("live"));
    let change = p
        .observe(&session(6, 2), &status("live"))
        .state_change()
        .expect("a sequence advance is a change")
        .clone();
    assert_eq!(change.from_sequence, Some(5));
    assert_eq!(change.to_sequence, 6);
    assert_eq!(change.from_revision, Some(2));
    assert_eq!(change.to_revision, 2);
    assert!(!change.stale_display);
}

#[test]
fn m217_ac05_revision_advance_alone_is_a_change() {
    let mut p = AutopilotPoller::new();
    p.observe(&session(5, 2), &status("live"));
    let change = p
        .observe(&session(5, 3), &status("live"))
        .state_change()
        .expect("a revision bump must redraw")
        .clone();
    assert_eq!(change.from_revision, Some(2));
    assert_eq!(change.to_revision, 3);
}

#[test]
fn m217_ac05_status_only_change_is_still_a_change() {
    // The status envelope moves independently of the session's
    // sequence (e.g. `live` → `paused`). The digest covers
    // everything the lane renders, so this still redraws.
    let mut p = AutopilotPoller::new();
    p.observe(&session(5, 2), &status("live"));
    assert!(
        p.observe(&session(5, 2), &status("paused")).changed(),
        "a run_state transition must reach the screen"
    );
}

#[test]
fn m217_ac05_body_change_without_a_sequence_bump_is_still_a_change() {
    // Defensive: if a payload's contents move without the
    // sequence advancing, the display must not go blind.
    let mut p = AutopilotPoller::new();
    let before = json!({"session": {"id": "alpha", "sequence": 5, "revision": 2, "queue": []}});
    let after = json!({
        "session": {"id": "alpha", "sequence": 5, "revision": 2,
                    "queue": [{"id": "217", "status": "failed"}]}
    });
    p.observe(&before, &Value::Null);
    assert!(p.observe(&after, &Value::Null).changed());
}

#[test]
fn m217_ac05_older_snapshot_is_flagged_as_a_stale_display() {
    let mut p = AutopilotPoller::new();
    p.observe(&session(9, 4), &status("live"));
    let change = p
        .observe(&session(7, 4), &status("live"))
        .state_change()
        .expect("a regression is still a change")
        .clone();
    assert!(
        change.stale_display,
        "a snapshot older than the rendered one must be flagged, not shown as progress"
    );
    assert_eq!(change.from_sequence, Some(9));
    assert_eq!(change.to_sequence, 7);
}

#[test]
fn m217_ac05_last_snapshot_tracks_the_most_recent_observation() {
    let mut p = AutopilotPoller::new();
    assert!(p.last_snapshot().is_none());
    p.observe(&session(1, 1), &Value::Null);
    assert_eq!(p.last_snapshot().map(|s| s.sequence), Some(1));
    p.observe(&session(2, 1), &Value::Null);
    assert_eq!(p.last_snapshot().map(|s| s.sequence), Some(2));
    // Even an unchanged observation leaves the snapshot in place.
    p.observe(&session(2, 1), &Value::Null);
    assert_eq!(p.last_snapshot().map(|s| s.sequence), Some(2));
}

// ─── the diff gates the redraw on the production path ───────────

#[test]
fn m217_ac05_unchanged_poll_does_not_bump_the_dirty_version() {
    // `run_loop` redraws when `app.version()` moves. The runner
    // points at a nonexistent binary so every payload is Null —
    // the first poll still establishes a snapshot (a change), and
    // every subsequent poll must be free.
    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/m217/ac05");
    let mut app = App::new();
    app.autopilot_poller.set_focused(true);

    let start = app.version();
    poll_autopilot_lane(&runner, &mut app, 0);
    let after_first = app.version();
    assert_ne!(after_first, start, "the first snapshot must render");

    for (i, now) in (10_000..60_000).step_by(10_000).enumerate() {
        poll_autopilot_lane(&runner, &mut app, now);
        assert_eq!(
            app.version(),
            after_first,
            "poll #{} of an unchanged session must not redraw",
            i + 2
        );
    }
    assert!(
        app.autopilot_poller.fired_count() >= 6,
        "the polls must actually have happened; got {}",
        app.autopilot_poller.fired_count()
    );
}

#[test]
fn m217_ac05_diffing_never_mutates_the_session_payloads() {
    // `observe` takes the payloads by reference and must leave
    // them untouched — the poller is a reader.
    let mut p = AutopilotPoller::new();
    let show = session(5, 2);
    let st = status("live");
    let show_before = show.clone();
    let status_before = st.clone();
    p.observe(&show, &st);
    p.observe(&show, &st);
    assert_eq!(show, show_before);
    assert_eq!(st, status_before);
}
