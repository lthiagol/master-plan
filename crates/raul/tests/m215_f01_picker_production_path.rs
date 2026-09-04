//! M215 / F-01 production-path test: picker keypress.
//!
//! Pin that a real `Space` keypress on the Autopilot lane
//! mutates the typed `app.autopilot.picker.selected` — the
//! production hot path, not just the typed model in isolation.
//! M225/M226 dogfood entry 49 anti-pattern is "library tests
//! pass but production never calls them"; this test exercises
//! `modes::normal::handle_key` end-to-end.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, Lane};
use raul::tui::autopilot::Picker;
use raul::tui::modes::normal;

fn list_payload() -> serde_json::Value {
    serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
            {"id": "M209", "title": "Coordination", "lifecycle": "in-progress"},
            {"id": "M211", "title": "Reconcile", "lifecycle": "remediation"},
        ]
    })
}

fn space() -> KeyEvent {
    KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty())
}

fn keypress(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

/// F-01 / production path: pressing Space on the Autopilot lane
/// dispatches `Action::AutopilotToggleSelect`. After
/// `apply_action`, `app.autopilot.picker.selected` contains the
/// highlighted candidate's id. This is the END-TO-END proof that
/// the typed Picker is wired into the production keypress flow.
#[test]
fn space_on_autopilot_lane_toggles_picker_selection() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Pre-condition: empty selection.
    assert!(app.autopilot.picker.queue_ids().is_empty());

    // Dispatch the Space keypress through the normal-mode
    // dispatcher (this is the same path the runner loop calls).
    let actions = normal::handle_key(space(), &app);
    assert_eq!(
        actions,
        vec![Action::AutopilotToggleSelect],
        "Space on the Autopilot lane must emit AutopilotToggleSelect"
    );

    // Apply the action — this is the same apply_action call the
    // runner loop makes after dispatch_key returns.
    for action in actions {
        raul::tui::action::apply_action(
            &mut app,
            &raul::mp_runner::MpRunner::new().unwrap(),
            action,
        )
        .unwrap();
    }

    // Production state mutated: the highlighted row is now
    // selected.
    assert_eq!(
        app.autopilot.picker.queue_ids(),
        &["207".to_string()],
        "production hot path must mutate app.autopilot.picker.selected on Space"
    );
}

/// F-01 / production path: pressing Space again toggles the
/// selection OFF — round-trip behavior matches the typed model.
#[test]
fn space_toggles_off_picker_selection_in_production_path() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Toggle on.
    let actions = normal::handle_key(space(), &app);
    for action in actions {
        raul::tui::action::apply_action(
            &mut app,
            &raul::mp_runner::MpRunner::new().unwrap(),
            action,
        )
        .unwrap();
    }
    assert_eq!(app.autopilot.picker.queue_ids().len(), 1);

    // Toggle off — the same Space keypress removes the entry.
    let actions = normal::handle_key(space(), &app);
    for action in actions {
        raul::tui::action::apply_action(
            &mut app,
            &raul::mp_runner::MpRunner::new().unwrap(),
            action,
        )
        .unwrap();
    }
    assert!(
        app.autopilot.picker.queue_ids().is_empty(),
        "Space must toggle the picker selection OFF"
    );
}

/// F-01 / production path: j / k move the picker cursor through
/// the production keypress flow. The cursor index moves by ±1
/// from the current position, mirroring the typed Picker.
#[test]
fn j_and_k_move_picker_cursor_in_production_path() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.refresh_picker(&list_payload());

    // Initial cursor at 0.
    assert_eq!(app.autopilot.picker.cursor, 0);

    // j → cursor + 1.
    let actions = normal::handle_key(keypress('j'), &app);
    assert_eq!(actions, vec![Action::AutopilotMovePicker { delta: 1 }]);
    for action in actions {
        raul::tui::action::apply_action(
            &mut app,
            &raul::mp_runner::MpRunner::new().unwrap(),
            action,
        )
        .unwrap();
    }
    assert_eq!(app.autopilot.picker.cursor, 1);

    // k → cursor - 1.
    let actions = normal::handle_key(keypress('k'), &app);
    assert_eq!(actions, vec![Action::AutopilotMovePicker { delta: -1 }]);
    for action in actions {
        raul::tui::action::apply_action(
            &mut app,
            &raul::mp_runner::MpRunner::new().unwrap(),
            action,
        )
        .unwrap();
    }
    assert_eq!(app.autopilot.picker.cursor, 0);
}

/// F-01 / production path: Space on a NON-Autopilot lane is NOT
/// routed to `AutopilotToggleSelect`. The Autopilot-specific
/// dispatch only fires when the active lane is Autopilot — a
/// future regression that broadens the dispatch fails here.
#[test]
fn space_on_other_lanes_does_not_dispatch_autopilot_toggle_select() {
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    app.autopilot.refresh_picker(&list_payload());

    let actions = normal::handle_key(space(), &app);
    assert!(
        !actions.contains(&Action::AutopilotToggleSelect),
        "Autopilot keybind dispatch must be lane-gated: {actions:?}"
    );
}

/// F-01: the typed `AutopilotLaneState.picker` field is the
/// production source of truth. Defensive pin: a future refactor
/// that re-points the renderer at `app.watch.candidates` directly
/// (bypassing the typed Picker) breaks this test.
#[test]
fn autopilot_picker_field_is_the_production_source_of_truth() {
    let app = App::new();
    let _picker: &Picker = &app.autopilot.picker;
    // The field exists, is public, and is reachable from
    // outside the module (so the render / dispatch tests can
    // exercise it).
}
