//! M168/M169: Settings lane flat list with [section] groups and staged edits.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsFocus, SettingsState};
use raul::tui::modes::settings::SETTINGS_KEYS;
use raul::tui::render;
use raul::tui::view_state;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn render_full(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

fn app_with_settings(config: serde_json::Value) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let mut state = SettingsState::new(config);
    state.selected_idx = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "workflow.profile")
        .expect("workflow.profile must exist");
    app.settings = Some(state);
    app
}

fn mp_bin() -> PathBuf {
    // M194: probe both release and debug profiles. CI builds --release
    // so `target/debug/mp` doesn't exist; the previous lookup assumed
    // debug and silently fell back to PATH (where `mp` isn't installed).
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("../../target/release/mp"),
        manifest.join("../../target/debug/mp"),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("mp"))
}

fn fixture_env() -> (TempDir, MpRunner) {
    let tmp = TempDir::new().expect("temp");
    let root = tmp.path().to_path_buf();
    let status = Command::new(mp_bin())
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(&root)
        .status()
        .expect("mp init");
    assert!(status.success(), "mp init failed");
    let mut runner = MpRunner::with_mp_bin(mp_bin());
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .expect("Settings lane");
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

fn ctrl_o() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
}

#[test]
fn help_box_shows_prose_for_focused_key() {
    let app = app_with_settings(serde_json::json!({
        "workflow": { "profile": "full" }
    }));
    let s = render_full(&app, 120, 30);
    assert!(
        s.contains("workflow.profile"),
        "help box should show the focused key; got:\n{s}"
    );
}

#[test]
fn flat_list_groups_by_section() {
    let app = app_with_settings(serde_json::json!({}));
    let s = render_full(&app, 160, 200);
    assert!(
        s.contains("ui.color"),
        "missing first key ui.color; got:\n{s}"
    );
    assert!(
        s.contains("keybinds.prev_item"),
        "missing last key keybinds.prev_item; got:\n{s}"
    );
    for section in &["ui", "workflow", "git", "next", "agent", "keybinds"] {
        assert!(
            s.contains(&format!("--- {section} ---")),
            "missing `--- {section} ---` header row; got:\n{s}"
        );
    }
}

#[test]
fn opening_focus_is_fields_not_sections() {
    let app = app_with_settings(serde_json::json!({}));
    let state = app.settings.as_ref().expect("settings lane state");
    assert_eq!(
        state.focus,
        SettingsFocus::Fields,
        "Settings must open in Fields (flat list) focus"
    );
}

#[test]
fn enter_in_flat_list_opens_edit_for_focused_key() {
    let app = app_with_settings(serde_json::json!({
        "ui": { "color": true }
    }));
    let state = app.settings.as_ref().expect("settings");
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
    assert_eq!(SETTINGS_KEYS[state.selected_idx].1, "workflow.profile");
}

#[test]
fn s_opens_settings_lane_once() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    assert_ne!(app.active_lane, Lane::Settings);

    let actions = raul::tui::modes::normal::handle_key(ctrl_o(), &app);
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    assert_eq!(actions, vec![Action::JumpLane(idx)]);
    apply_action(&mut app, &runner, Action::JumpLane(idx)).unwrap();
    assert_eq!(app.active_lane, Lane::Settings);
    assert!(app.settings.is_some());

    // Re-press Ctrl-O while already on Settings is a no-op.
    let again = raul::tui::modes::normal::handle_key(ctrl_o(), &app);
    assert!(again.is_empty());
}

#[test]
fn enter_stages_value_then_enter_commits_via_mp_config_set() {
    let (tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    apply_action(&mut app, &runner, Action::Enter).unwrap();
    {
        let state = app.settings.as_ref().expect("settings");
        assert_eq!(state.focus, SettingsFocus::Editing);
        assert!(state.edit.is_some());
    }

    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    let state = app.settings.as_ref().expect("settings");
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
    assert_eq!(
        state.staged_edits.get("ui.color"),
        Some(&"false".to_string())
    );

    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();

    let raw = std::fs::read_to_string(tmp.path().join("master-plan/config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ui"]["color"], serde_json::json!(false));
}

#[test]
fn leaving_settings_lane_discards_staged_edits() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    apply_action(&mut app, &runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    assert!(app.settings.as_ref().unwrap().has_staged_edits());

    apply_action(&mut app, &runner, Action::JumpLane(0)).unwrap();
    assert_eq!(app.active_lane, Lane::Overview);
    assert!(app.settings.is_none());

    open_settings_lane(&mut app, &runner);
    let state = app.settings.as_ref().expect("reloaded settings");
    assert!(state.staged_edits.is_empty());
    let displayed = raul::tui::modes::settings::value_for_key(&state.config, "ui.color");
    assert_ne!(displayed, "false");
}
