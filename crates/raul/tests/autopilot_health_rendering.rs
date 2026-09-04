//! M217 / AC-07 — health + heartbeat rendering.
//!
//! Everything on this surface is *reported*, never *computed*.
//! raul renders the `health` / `heartbeat_at` values that
//! `mp autopilot status` returns; it generates no pulses of its
//! own, does not decide from the local wall clock that an agent
//! has died, and does not escalate. A stale status is displayed
//! as stale because mp classified it that way — and the badge
//! says so, naming mp as the owner of the escalation.
//!
//! The consequence worth pinning: a responsive agent that reports
//! the *same* state twice is still healthy. Sameness is not
//! staleness — that judgement belongs to mp's liveness model
//! (M213), not to the display's diff.

use raul::mp_runner::MpRunner;
use raul::tui::app::App;
use raul::tui::poll::{poll_autopilot_lane, AutopilotPoller, Health};
use serde_json::{json, Value};

fn status(health: &str, heartbeat_age: Option<u64>) -> Value {
    let mut s = json!({
        "run_state": {"kind": "live"},
        "health": health,
        "heartbeat_at": "2026-09-04T12:00:00Z",
    });
    if let Some(age) = heartbeat_age {
        s["heartbeat_age_secs"] = json!(age);
    }
    s
}

// ─── projection ─────────────────────────────────────────────────

#[test]
fn m217_ac07_health_and_heartbeat_are_read_verbatim_from_status() {
    let h = Health::from_status(&status("healthy", Some(3)));
    assert_eq!(h.run_state, "live");
    assert_eq!(h.health, "healthy");
    assert_eq!(h.heartbeat_at, "2026-09-04T12:00:00Z");
    assert_eq!(h.heartbeat_age_secs, Some(3));
    assert!(!h.stale);
}

#[test]
fn m217_ac07_absent_fields_degrade_to_unknown_rather_than_a_guess() {
    let h = Health::from_status(&Value::Null);
    assert_eq!(h.run_state, "unknown");
    assert_eq!(h.health, "unknown");
    assert_eq!(h.heartbeat_at, "");
    assert_eq!(
        h.heartbeat_age_secs, None,
        "raul must not synthesize an age from its own clock"
    );
    assert!(!h.stale, "unknown is not stale — that call belongs to mp");
}

#[test]
fn m217_ac07_nested_health_object_shape_is_accepted() {
    // mp may report health as an object rather than a string.
    let payload = json!({
        "run_state": {"kind": "paused"},
        "health": {"state": "healthy", "heartbeat_at": "2026-09-04T12:00:05Z",
                   "heartbeat_age_secs": 7, "stale": false},
    });
    let h = Health::from_status(&payload);
    assert_eq!(h.run_state, "paused");
    assert_eq!(h.health, "healthy");
    assert_eq!(h.heartbeat_at, "2026-09-04T12:00:05Z");
    assert_eq!(h.heartbeat_age_secs, Some(7));
    assert!(!h.stale);
}

#[test]
fn m217_ac07_status_wrapped_in_a_status_key_is_accepted() {
    let payload = json!({"status": {"run_state": {"kind": "live"}, "health": "healthy"}});
    let h = Health::from_status(&payload);
    assert_eq!(h.run_state, "live");
    assert_eq!(h.health, "healthy");
}

// ─── staleness is mp's call, not raul's ─────────────────────────

#[test]
fn m217_ac07_stale_is_taken_from_mp_not_derived_locally() {
    let explicit = json!({"run_state": {"kind": "live"}, "health": "healthy", "stale": true});
    assert!(
        Health::from_status(&explicit).stale,
        "an explicit stale flag from mp must be honoured even when health reads healthy"
    );

    // A very old heartbeat that mp did *not* flag stays not-stale:
    // raul does not second-guess mp's liveness model.
    let old_but_unflagged = json!({
        "run_state": {"kind": "live"},
        "health": "healthy",
        "heartbeat_at": "1999-01-01T00:00:00Z",
        "heartbeat_age_secs": 999_999,
    });
    assert!(
        !Health::from_status(&old_but_unflagged).stale,
        "raul must not decide staleness from a timestamp mp considers fine"
    );
}

#[test]
fn m217_ac07_mp_reported_stale_health_is_displayed() {
    let h = Health::from_status(&status("stale", Some(120)));
    assert!(h.stale);
    assert_eq!(h.health, "stale");
    let badge = h.badge();
    assert!(badge.contains("stale"), "got {badge:?}");
    assert!(
        badge.contains("escalation: mp"),
        "the badge must name mp as the escalation owner so the operator does not wait on raul; got {badge:?}"
    );
}

#[test]
fn m217_ac07_responsive_same_state_agent_stays_healthy() {
    // Poll the same healthy status repeatedly. The diff reports
    // "unchanged" (no redraw), but the health projection must keep
    // reading healthy — repeating a state is not a liveness fault.
    let mut p = AutopilotPoller::new();
    let session = json!({"session": {"id": "alpha", "sequence": 5, "revision": 1}});
    let st = status("healthy", Some(1));
    for i in 0..20 {
        let outcome = p.observe(&session, &st);
        if i > 0 {
            assert!(
                !outcome.changed(),
                "an unchanged snapshot must not redraw (poll #{i})"
            );
        }
        let h = Health::from_status(&st);
        assert_eq!(h.health, "healthy", "poll #{i}");
        assert!(!h.stale, "poll #{i}: sameness is not staleness");
    }
}

// ─── badge rendering ────────────────────────────────────────────

#[test]
fn m217_ac07_badge_shows_age_when_reported() {
    let badge = Health::from_status(&status("healthy", Some(3))).badge();
    assert!(badge.contains("health: healthy"), "got {badge:?}");
    assert!(badge.contains("heartbeat 3s ago"), "got {badge:?}");
    assert!(
        !badge.contains("escalation"),
        "healthy needs no escalation note"
    );
}

#[test]
fn m217_ac07_badge_falls_back_to_the_timestamp_when_no_age_is_reported() {
    let badge = Health::from_status(&status("healthy", None)).badge();
    assert!(badge.contains("2026-09-04T12:00:00Z"), "got {badge:?}");
}

#[test]
fn m217_ac07_badge_is_stable_for_an_empty_status() {
    let badge = Health::from_status(&Value::Null).badge();
    assert_eq!(badge, "health: unknown");
}

// ─── production wiring ──────────────────────────────────────────

#[test]
fn m217_ac07_lane_state_starts_without_health_and_the_accessor_is_read_only() {
    let app = App::new();
    assert!(
        app.autopilot.health().is_none(),
        "no refresh has happened yet — the renderer must fall back, not show a fabricated 'healthy'"
    );
}

#[test]
fn m217_ac07_poll_populates_the_lane_health_field() {
    // The runner points at a nonexistent binary, so the status
    // payload is Null — the projection must still land (as
    // `unknown`) rather than leaving the field empty, so the
    // renderer has something honest to show.
    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/m217/ac07");
    let mut app = App::new();
    app.autopilot_poller.set_focused(true);
    poll_autopilot_lane(&runner, &mut app, 0);
    let health = app
        .autopilot
        .health()
        .expect("a completed poll must populate the health projection");
    assert_eq!(health.health, "unknown");
    assert!(!health.stale);
}

#[test]
fn m217_ac07_status_pane_renders_the_health_badge() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::Lane;
    use raul::tui::{render, view_state};

    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app.autopilot.health = Some(Health::from_status(&status("stale", Some(90))));

    let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut screen = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            screen.push_str(buf[(x, y)].symbol());
        }
        screen.push('\n');
    }
    assert!(
        screen.contains("health: stale"),
        "the reported health must reach the screen; got:\n{screen}"
    );
    assert!(
        screen.contains("escalation: mp"),
        "the badge must name mp as the escalation owner on screen"
    );
}
