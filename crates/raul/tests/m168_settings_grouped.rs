//! M168 BF-04: Settings flat list groups by section.

use raul::tui::app::{App, Lane};
use raul::tui::mode::SettingsState;
use raul::tui::modes::settings::SETTINGS_KEYS;

fn open_settings() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    app.settings = Some(SettingsState::new(serde_json::json!({})));
    app
}

fn selected_key(app: &App) -> &'static str {
    let state = app.settings.as_ref().expect("expected settings lane state");
    SETTINGS_KEYS[state.selected_idx].1
}

fn set_selected(app: &mut App, key: &str) {
    let idx = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == key)
        .unwrap_or_else(|| panic!("{key} must exist in SETTINGS_KEYS"));
    let state = app.settings.as_mut().expect("expected settings lane state");
    state.selected_idx = idx;
}

#[test]
fn down_skips_section_headers_lands_on_next_key() {
    let mut app = open_settings();
    set_selected(&mut app, "ui.color");
    app.move_down();
    assert_eq!(selected_key(&app), "ui.icons");
}

#[test]
fn down_from_last_key_of_section_jumps_to_first_key_of_next() {
    let mut app = open_settings();
    // M198: `ui.show_autopilot_tab` is the last key of the ui section.
    set_selected(&mut app, "ui.show_autopilot_tab");
    app.move_down();
    assert_eq!(selected_key(&app), "workflow.profile");
}

#[test]
fn up_clamps_to_first_key_not_to_a_section_header() {
    let mut app = open_settings();
    set_selected(&mut app, "ui.color");
    app.move_up();
    assert_eq!(selected_key(&app), "ui.color");
}

#[test]
fn page_down_jumps_one_page_size_within_keys() {
    let mut app = open_settings();
    set_selected(&mut app, "ui.color");
    app.move_page_down();
    let target = App::PAGE_SIZE.min(SETTINGS_KEYS.len() - 1);
    let state = app.settings.as_ref().expect("expected settings lane state");
    assert_eq!(
        state.selected_idx, target,
        "PageDown should land at min(PAGE_SIZE, last) = {target}"
    );
}
