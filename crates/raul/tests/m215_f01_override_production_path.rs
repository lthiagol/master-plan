//! M215 / F-01 production-path test: override panel keypress.
//!
//! Pin that pressing `<o>` on the Autopilot lane toggles
//! `app.autopilot.panel_open` and lazily constructs the typed
//! `OverridePanel`. The panel persists across close/reopen
//! cycles so user-typed values survive.

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

/// F-01: pressing `<o>` on the Autopilot lane dispatches
/// `Action::AutopilotTogglePanel`. After apply_action, the panel
/// is open and `app.autopilot.panel` is `Some`. The dispatcher
/// flows through `modes::normal::handle_key` — the production
/// path, not a direct mutator call.
#[test]
fn o_on_autopilot_lane_opens_override_panel() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    assert!(!app.autopilot.panel_open);
    assert!(app.autopilot.panel.is_none());

    let actions = normal::handle_key(keypress('o'), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotTogglePanel],
        "<o> on Autopilot lane must emit AutopilotTogglePanel"
    );
    for action in actions {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }

    assert!(
        app.autopilot.panel_open,
        "panel must be visible after `<o>` opens it"
    );
    assert!(
        app.autopilot.panel.is_some(),
        "panel must be lazily constructed on first open"
    );
    // Default values: 3-agent, 2s refresh.
    assert_eq!(app.autopilot.panel().unwrap().topology, "three-agent");
    assert_eq!(app.autopilot.panel().unwrap().refresh_secs, 2);
}

/// F-01: a second `<o>` press closes the panel. The panel's
/// `Some` survives — closing is a visibility flag flip, not a
/// drop.
#[test]
fn second_o_closes_the_panel_and_preserves_typed_values() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;

    // Open.
    for action in normal::handle_key(keypress('o'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.panel_open);

    // Mutate the panel.
    if let Some(panel) = app.autopilot.panel.as_mut() {
        panel.topology = "two-agent".to_string();
        panel.refresh_secs = 5;
    }
    let snapshot = app.autopilot.panel.clone();

    // Close.
    for action in normal::handle_key(keypress('o'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(!app.autopilot.panel_open, "second <o> closes the panel");
    assert_eq!(
        app.autopilot.panel, snapshot,
        "panel values must survive close → reopen"
    );

    // Reopen — same values still there.
    for action in normal::handle_key(keypress('o'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.panel_open);
    assert_eq!(app.autopilot.panel().unwrap().topology, "two-agent");
    assert_eq!(app.autopilot.panel().unwrap().refresh_secs, 5);
}

/// F-01: while the panel is open, Space toggling the picker
/// selection is BLOCKED. The Autopilot handler returns the
/// toggle action only when neither the panel nor the replay
/// shell is open — this prevents cross-contamination between
/// list navigation and form editing.
#[test]
fn space_is_blocked_while_override_panel_is_open() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&sample_list());

    // Open the panel.
    for action in normal::handle_key(keypress('o'), &app) {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(app.autopilot.panel_open);

    // Space must NOT toggle the picker while the panel is open.
    let actions = normal::handle_key(
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        &app,
    );
    assert!(
        !actions.contains(&Action::AutopilotToggleSelect),
        "Space must be blocked while the panel is open: {actions:?}"
    );

    let _ = sample_list;
}

/// F-01: `<o>` on a NON-Autopilot lane is NOT routed to
/// `AutopilotTogglePanel`. The Autopilot-specific dispatch only
/// fires when the active lane is Autopilot — the global
/// `keybinds.cycle_sort` (default plain `o`) still wins on
/// other lanes. This pins the lane gate.
#[test]
fn o_on_other_lanes_does_not_dispatch_autopilot_toggle_panel() {
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    let actions = normal::handle_key(keypress('o'), &app);
    assert!(
        !actions.contains(&Action::AutopilotTogglePanel),
        "<o> on Milestones must NOT emit AutopilotTogglePanel: {actions:?}"
    );
}

/// F-01: pressing Esc while the panel is open closes it (rather
/// than bubbling up to the global Esc handler). The behavior is
/// the toggle action's mirror image.
#[test]
fn esc_closes_the_open_panel() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.open_panel();
    assert!(app.autopilot.panel_open);

    let actions = normal::handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotTogglePanel],
        "Esc must close the panel when it is open"
    );
    for action in actions {
        raul::tui::action::apply_action(&mut app, &runner(), action).unwrap();
    }
    assert!(!app.autopilot.panel_open);
}

// Helper outside the test attribute so the `#[test]` body above
// can reference it via `App::new()`.
use raul::tui::app::App;

fn sample_list() -> serde_json::Value {
    serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
            {"id": "M209", "title": "Coordination", "lifecycle": "in-progress"},
        ]
    })
}
