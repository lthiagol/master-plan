//! M215 / F-01 production-path test: replay shell keypress.
//!
//! Pin that pressing capital P on the Autopilot lane dispatches
//! `Action::AutopilotOpenReplay`, which (via `apply_action`)
//! populates `app.autopilot.replay_shell` and sets
//! `replay_open`. The shell is built from the typed
//! `ReplayShell::from_session_list_entry` constructor — the
//! production shell, not a test stub.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::Lane;
use raul::tui::modes::normal;

fn keypress(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

fn runner() -> raul::mp_runner::MpRunner {
    raul::mp_runner::MpRunner::new().unwrap()
}

/// F-01: capital P on the Autopilot lane dispatches
/// `Action::AutopilotOpenReplay`. After apply_action, the
/// replay_shell is `Some` (possibly empty if no past sessions
/// exist) and `replay_open == true`.
#[test]
fn capital_p_on_autopilot_lane_opens_replay_shell() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    assert!(!app.autopilot.replay_open);
    assert!(app.autopilot.replay_shell.is_none());

    let actions = normal::handle_key(keypress('P'), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotOpenReplay],
        "capital P on Autopilot lane must emit AutopilotOpenReplay"
    );
    for action in actions {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }

    assert!(
        app.autopilot.replay_open,
        "replay_open must be true after AutopilotOpenReplay"
    );
    // The shell may have empty session_id when no mp binary is on
    // PATH (test env) — what matters is the typed-shell slot is
    // populated and `replay_open == true`.
    assert!(
        app.autopilot.replay_shell.is_some(),
        "replay_shell must be populated by AutopilotOpenReplay"
    );
}

/// F-01: capital P on a NON-Autopilot lane is NOT routed to
/// `AutopilotOpenReplay`. The dispatch is lane-gated.
#[test]
fn capital_p_on_other_lanes_does_not_open_replay() {
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    let actions = normal::handle_key(keypress('P'), &app);
    assert!(
        !actions.contains(&Action::AutopilotOpenReplay),
        "capital P on Milestones must NOT emit AutopilotOpenReplay: {actions:?}"
    );
}

/// F-01: pressing Esc while the replay shell is open closes it
/// (rather than bubbling up to the global Esc handler). The
/// shell value persists across close → reopen.
#[test]
fn esc_closes_the_open_replay_shell() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;

    // Open via the production path.
    for action in normal::handle_key(keypress('P'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.replay_open);
    let shell_snapshot = app.autopilot.replay_shell.clone();
    assert!(shell_snapshot.is_some());

    // Esc closes — the Autopilot handler must run first so the
    // generic Esc action doesn't fire instead.
    let actions = normal::handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotCloseReplay],
        "Esc must close the replay shell when it is open"
    );
    for action in actions {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(!app.autopilot.replay_open);
    assert_eq!(
        app.autopilot.replay_shell, shell_snapshot,
        "shell value must persist across close"
    );
}

/// F-01: while the replay shell is open, Space does NOT toggle
/// the picker (the picker is hidden behind the shell). This
/// pins the cross-contamination guard.
#[test]
fn space_is_blocked_while_replay_shell_is_open() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Open the replay shell.
    for action in normal::handle_key(keypress('P'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.replay_open);

    let actions = normal::handle_key(
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        &app,
    );
    assert!(
        !actions.contains(&Action::AutopilotToggleSelect),
        "Space must be blocked while the replay shell is open: {actions:?}"
    );
}

fn list_payload() -> serde_json::Value {
    serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
        ]
    })
}

// Local App alias used by all tests in this file.
use raul::tui::app::App;
