//! M217 / AC-02 — display polling pauses when unfocused.
//!
//! The distinction this file exists to pin: *display* polling is
//! the only thing the focus gate touches. The headless drive is
//! owned by mp; it keeps running and keeps accumulating session
//! events while raul's Autopilot lane is unfocused (or while raul
//! is not running at all). When the lane regains focus, the
//! poller performs exactly **one** catch-up request and picks up
//! whatever the engine recorded meanwhile — never one request per
//! interval missed.

use raul::tui::app::{App, Lane};
use raul::tui::poll::{AutopilotPoller, PollDecision, Snapshot};
use serde_json::{json, Value};

fn focused() -> AutopilotPoller {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    p
}

/// A session payload at a given event sequence, standing in for
/// the headless engine's progress.
fn session_at(seq: u64, events: u64) -> Value {
    json!({
        "session": {
            "id": "alpha",
            "sequence": seq,
            "revision": seq,
            "events": (0..events).map(|i| json!({"seq": i})).collect::<Vec<_>>(),
        }
    })
}

#[test]
fn m217_ac02_a_fresh_poller_starts_unfocused() {
    let p = AutopilotPoller::new();
    assert!(
        !p.is_focused(),
        "the poller must not poll until a lane focus event tells it to"
    );
    assert_eq!(p.decide(0), PollDecision::Unfocused);
}

#[test]
fn m217_ac02_unfocused_ticks_never_fire() {
    let mut p = AutopilotPoller::new();
    for now in (0..30_000).step_by(250) {
        assert_eq!(p.begin(now), PollDecision::Unfocused);
    }
    assert_eq!(
        p.fired_count(),
        0,
        "an unfocused lane must issue zero requests"
    );
    assert!(p.skipped_unfocused_count() > 100);
}

#[test]
fn m217_ac02_losing_focus_pauses_mid_stream() {
    let mut p = focused();
    p.begin(0);
    p.finish(0);
    assert_eq!(p.begin(2_000), PollDecision::Fire);
    p.finish(2_000);
    p.set_focused(false);
    assert_eq!(
        p.begin(4_000),
        PollDecision::Unfocused,
        "a due tick must not fire once focus is gone"
    );
    assert_eq!(p.fired_count(), 2);
}

#[test]
fn m217_ac02_refocus_resumes_with_exactly_one_request_not_a_burst() {
    let mut p = focused();
    p.begin(0);
    p.finish(0);
    p.set_focused(false);
    // 60 seconds unfocused against a 2s cadence — 30 intervals
    // "missed". None of them may be replayed.
    for now in (1_000..61_000).step_by(250) {
        p.begin(now);
    }
    assert_eq!(p.fired_count(), 1);

    p.set_focused(true);
    assert_eq!(
        p.begin(61_000),
        PollDecision::Fire,
        "refocus must refresh immediately — a stale screen is the whole problem"
    );
    p.finish(61_000);
    assert_eq!(
        p.fired_count(),
        2,
        "refocus after 30 missed intervals must fire once, not 30 times (no catch-up burst)"
    );
    assert_eq!(
        p.begin(61_500),
        PollDecision::NotDue,
        "and the interval must be re-armed from the catch-up request"
    );
}

#[test]
fn m217_ac02_repeated_focus_events_are_idempotent() {
    let mut p = focused();
    p.begin(0);
    p.finish(0);
    // A redundant `set_focused(true)` on every idle tick (the
    // production hook calls it unconditionally) must not re-arm
    // the timer — otherwise the poller would fire on every tick.
    for now in (100..1_900).step_by(100) {
        p.set_focused(true);
        assert_eq!(p.begin(now), PollDecision::NotDue);
    }
    assert_eq!(p.fired_count(), 1);
}

#[test]
fn m217_ac02_headless_drive_accumulates_events_while_display_polling_is_paused() {
    // The "engine" advances on its own clock; the poller only
    // ever reads. While unfocused the poller reads nothing, so
    // the display keeps the sequence it last saw.
    let mut p = focused();
    let at_start = session_at(10, 10);
    p.begin(0);
    let first = p.observe(&at_start, &Value::Null);
    p.finish(0);
    assert!(first.changed());
    assert_eq!(p.last_snapshot().map(|s| s.sequence), Some(10));

    p.set_focused(false);
    // The drive keeps going: sequence 10 → 42, events 10 → 42.
    let while_away = session_at(42, 42);
    for now in (1_000..30_000).step_by(250) {
        assert_eq!(p.begin(now), PollDecision::Unfocused);
    }
    assert_eq!(
        p.last_snapshot().map(|s| s.sequence),
        Some(10),
        "a paused display must not silently advance its snapshot"
    );

    // Refocus: one request picks up everything that accumulated.
    p.set_focused(true);
    assert_eq!(p.begin(30_000), PollDecision::Fire);
    let resumed = p.observe(&while_away, &Value::Null);
    p.finish(30_000);
    let change = resumed
        .state_change()
        .expect("the accumulated progress must surface as one state change");
    assert_eq!(change.from_sequence, Some(10));
    assert_eq!(
        change.to_sequence, 42,
        "the full jump lands in a single update"
    );
    assert_eq!(
        Snapshot::from_payloads(&while_away, &Value::Null).sequence,
        42
    );
}

#[test]
fn m217_ac02_focus_gate_is_driven_by_the_active_lane() {
    // The production idle hook mirrors `app.active_lane` into the
    // poller on every iteration; this pins that mapping so a lane
    // switch is all it takes to pause/resume display polling.
    let mut app = App::new();
    for lane in [Lane::Milestones, Lane::Backlog, Lane::Ideas] {
        app.select_lane(lane);
        app.autopilot_poller
            .set_focused(app.active_lane == Lane::Autopilot);
        assert_eq!(
            app.autopilot_poller.decide(0),
            PollDecision::Unfocused,
            "{lane:?} is not the Autopilot lane"
        );
    }
    app.select_lane(Lane::Autopilot);
    app.autopilot_poller
        .set_focused(app.active_lane == Lane::Autopilot);
    assert_eq!(app.autopilot_poller.decide(0), PollDecision::Fire);
}
