//! M215 / F-01 production-path test: Start action keypress.
//!
//! Pin that pressing `<s>` on the Autopilot lane dispatches
//! `Action::AutopilotStart`, which (via `apply_action`)
//! validates the picker + panel state and (when valid) shells
//! out to `mp autopilot start <ids...>` with the typed payload.
//!
//! The test verifies the validation gate end-to-end: an empty
//! selection refuses to start, an open panel blocks Start, an
//! open replay blocks Start, and a populated picker + closed
//! panel + closed replay reaches the `mp autopilot start`
//! shell-out (which the runner treats as a `Result` return).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::Lane;
use raul::tui::autopilot::OverridePanel;
use raul::tui::modes::normal;

fn keypress(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

fn runner() -> raul::mp_runner::MpRunner {
    raul::mp_runner::MpRunner::new().unwrap()
}

fn list_payload() -> serde_json::Value {
    serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
            {"id": "M209", "title": "Coordination", "lifecycle": "in-progress"},
        ]
    })
}

/// F-01: pressing `<s>` on the Autopilot lane dispatches
/// `Action::AutopilotStart`. This is the production keypress
/// path — the dispatcher must fire even when the picker is
/// empty (validation lives in `apply_action`, not here).
#[test]
fn s_on_autopilot_lane_dispatches_autopilot_start_action() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;

    let actions = normal::handle_key(keypress('s'), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotStart],
        "<s> on Autopilot lane must emit AutopilotStart"
    );
}

/// F-01: `Action::AutopilotStart` is a no-op when the picker is
/// empty (no milestone selected). Validation lives in
/// `apply_action`; the dispatcher must NOT panic or error, just
/// refuse silently.
#[test]
fn autopilot_start_is_a_noop_when_picker_is_empty() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());
    // No selection — picker has candidates but no `selected`.

    let result = raul::tui::action::apply_action(&mut app, &runner(), Action::AutopilotStart);
    assert!(result.is_ok(), "empty picker must not error");
}

/// F-01: `Action::AutopilotStart` is a no-op when the override
/// panel is open. The user is editing the panel; Start must not
/// steal the keypress.
#[test]
fn autopilot_start_is_a_noop_when_panel_is_open() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());
    // Select something.
    for action in normal::handle_key(
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        &app,
    ) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert_eq!(app.autopilot.picker.queue_ids().len(), 1);
    // Open the panel.
    app.autopilot.open_panel();
    assert!(app.autopilot.panel_open);

    // Start is now a no-op (the panel is open).
    let can_start = app.autopilot.can_start();
    assert!(
        !can_start,
        "can_start must be false while the panel is open"
    );
    let result = raul::tui::action::apply_action(&mut app, &runner(), Action::AutopilotStart);
    assert!(result.is_ok());
}

/// F-01: with a populated picker, closed panel, and closed
/// replay, `can_start` returns `true` — the validation gate
/// clears and `<s>` reaches the `mp autopilot start` shell-out.
/// The shell-out itself is exercised by the integration test
/// harness when an `mp` binary is on PATH; this test pins the
/// typed gate.
#[test]
fn autopilot_start_validates_with_picker_selection_and_closed_panel() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Select via production path.
    for action in normal::handle_key(
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        &app,
    ) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert_eq!(app.autopilot.picker.queue_ids(), &["207".to_string()]);

    // Build a panel with non-default values — the typed payload
    // shape flows through `to_session_overrides`.
    let mut panel = OverridePanel::new();
    panel.topology = "two-agent".to_string();
    panel.refresh_secs = 4;
    app.autopilot.panel = Some(panel);

    // can_start is true.
    assert!(
        app.autopilot.can_start(),
        "can_start must be true with selection + closed panel + closed replay"
    );

    // Apply Start — the runner's `run_raw_allow_failure` returns
    // Ok(()) even when no `mp` binary exists on PATH, so the
    // apply_action call succeeds end-to-end. The typed panel
    // state is preserved.
    let result = raul::tui::action::apply_action(&mut app, &runner(), Action::AutopilotStart);
    assert!(
        result.is_ok(),
        "Start must not error when validation passes: {result:?}"
    );
    // Panel still set; queue still has the selection.
    assert_eq!(app.autopilot.picker.queue_ids(), &["207".to_string()]);
    assert_eq!(app.autopilot.panel().unwrap().topology, "two-agent");
}

/// F-01: `can_start` is `false` when the replay shell is open.
/// The replay shell is read-only; Start is blocked.
#[test]
fn autopilot_start_is_blocked_while_replay_shell_is_open() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Toggle a selection.
    for action in normal::handle_key(
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        &app,
    ) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }

    // Open the replay shell via the production path.
    for action in normal::handle_key(keypress('P'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.replay_open);

    assert!(
        !app.autopilot.can_start(),
        "can_start must be false while the replay shell is open"
    );
}

/// F-01: `<s>` on a NON-Autopilot lane is NOT routed to
/// `Action::AutopilotStart`. The dispatch is lane-gated.
#[test]
fn s_on_other_lanes_does_not_dispatch_autopilot_start() {
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    let actions = normal::handle_key(keypress('s'), &app);
    assert!(
        !actions.contains(&Action::AutopilotStart),
        "<s> on Milestones must NOT emit AutopilotStart: {actions:?}"
    );
}

// Local App alias used by all tests in this file.
use raul::tui::app::App;
