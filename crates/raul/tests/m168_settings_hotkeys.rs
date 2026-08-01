//! M169: Settings lane hotkey semantics (no BF-05 Tab hijack, Esc no-op on lane).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsFocus, SettingsState};
use raul::tui::modes::settings::SETTINGS_KEYS;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn open_settings_at(idx: usize) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let mut state = SettingsState::new(serde_json::json!({}));
    state.selected_idx = idx.min(SETTINGS_KEYS.len().saturating_sub(1));
    app.settings = Some(state);
    app
}

fn open_settings_editing(buffer: &str) -> App {
    let mut app = open_settings_at(0);
    if let Some(state) = app.settings.as_mut() {
        state.edit = Some(raul::tui::mode::SettingsEdit {
            key: SETTINGS_KEYS[0].1.to_string(),
            cursor: buffer.chars().count(),
            buffer: buffer.to_string(),
            errors: Vec::new(),
        });
        state.focus = SettingsFocus::Editing;
    }
    app
}

#[test]
fn esc_closes_settings_twice_is_not_required() {
    // M169: Esc on the Settings lane cancels an active edit only; a second
    // Esc is not required to "close" Settings — the lane stays put.
    let app = open_settings_at(0);
    assert_eq!(app.active_lane, Lane::Settings);

    // Esc on flat list: no-op (lane unchanged).
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Esc), &app);
    assert!(actions.is_empty(), "Esc on flat list must be a no-op");
    assert_eq!(app.active_lane, Lane::Settings);

    // Esc during edit cancels the edit popup only.
    let mut app = open_settings_editing("foo");
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Esc), &app);
    assert_eq!(actions, vec![Action::Esc]);
    let runner = MpRunner::new().expect("mp");
    apply_action(&mut app, &runner, Action::Esc).unwrap();
    assert_eq!(app.active_lane, Lane::Settings);
    let state = app.settings.as_ref().expect("settings");
    assert!(state.edit.is_none());
    assert_eq!(state.focus, SettingsFocus::Fields);

    // Second Esc still a no-op — no double-Esc modal escape.
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Esc), &app);
    assert!(actions.is_empty());
    assert_eq!(app.active_lane, Lane::Settings);
}

#[test]
fn tab_cycles_lanes_from_flat_list() {
    let target = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "workflow.profile")
        .expect("workflow.profile must exist");
    let app = open_settings_at(target);
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Tab), &app);
    assert!(
        actions.is_empty(),
        "Tab on flat list must fall through to normal lane cycling"
    );
    let normal = raul::tui::modes::normal::handle_key(key(KeyCode::Tab), &app);
    assert_eq!(normal, vec![Action::NextLane]);
}

#[test]
fn tab_commits_edit_instead_of_cycling_lanes() {
    let app = open_settings_editing("foo");
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Tab), &app);
    assert_eq!(
        actions,
        vec![Action::Enter],
        "Tab inside edit popup must commit the edit"
    );
}

#[test]
fn h_is_noop_while_settings_is_open() {
    let app = open_settings_at(0);
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Char('h')), &app);
    assert_eq!(actions, Vec::new());

    let app = open_settings_editing("foo");
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Char('h')), &app);
    assert_eq!(actions, Vec::new());
    let edit = app.settings.as_ref().unwrap().edit.as_ref().unwrap();
    assert_eq!(edit.buffer, "foo");
}

#[test]
fn printable_chars_still_route_to_edit_buffer() {
    use raul::tui::action::apply_action;

    let mut app = open_settings_editing("");
    let actions = raul::tui::modes::settings::handle_key(key(KeyCode::Char('a')), &app);
    assert_eq!(actions, vec![Action::PushInputChar('a')]);
    let runner = MpRunner::new().expect("mp");
    apply_action(&mut app, &runner, Action::PushInputChar('a')).unwrap();
    let edit = app.settings.as_ref().unwrap().edit.as_ref().unwrap();
    assert_eq!(edit.buffer, "a");
}
