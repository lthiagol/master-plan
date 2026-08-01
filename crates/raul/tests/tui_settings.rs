//! M140/M169: Settings lane integration tests (render + dispatch).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::{Mode, SettingsFocus, SettingsState};
use raul::tui::modes::settings::SETTINGS_KEYS;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn mp_bin() -> PathBuf {
    // M194: probe both release and debug profiles. CI builds --release
    // so  doesn't exist; the previous lookup assumed
    // debug and silently fell back to PATH (where  isn't installed).
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
    assert!(status.success());
    let mut runner = MpRunner::with_mp_bin(mp_bin());
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

fn ctrl_o() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
}

#[test]
fn jump_lane_opens_settings_and_loads_config() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    assert_eq!(app.active_lane, Lane::Settings);
    assert_eq!(app.active_mode, Mode::Normal);
    assert!(app.settings.is_some());
    let state = app.settings.as_ref().unwrap();
    assert_eq!(state.focus, SettingsFocus::Fields);
}

#[test]
fn ctrl_o_from_normal_jumps_to_settings_lane() {
    let app = App::new();
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    let actions = raul::tui::modes::normal::handle_key(ctrl_o(), &app);
    assert_eq!(actions, vec![Action::JumpLane(idx)]);
}

#[test]
fn enter_opens_edit_then_stages_on_second_enter() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    apply_action(&mut app, &runner, Action::Enter).unwrap();
    assert_eq!(app.settings.as_ref().unwrap().focus, SettingsFocus::Editing);

    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.staged_edits.contains_key("ui.color"));
}

#[test]
fn settings_save_commits_staged_edits() {
    let (tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();

    let raw = std::fs::read_to_string(tmp.path().join("master-plan/config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ui"]["color"], false);
}

#[test]
fn esc_cancels_active_edit_only() {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    app.settings = Some(SettingsState::new(serde_json::json!({})));
    app.settings.as_mut().unwrap().edit = Some(raul::tui::mode::SettingsEdit {
        key: SETTINGS_KEYS[0].1.to_string(),
        buffer: "x".to_string(),
        cursor: 1,
        errors: Vec::new(),
    });
    app.settings.as_mut().unwrap().focus = SettingsFocus::Editing;

    let actions = raul::tui::modes::settings::handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        &app,
    );
    assert_eq!(actions, vec![Action::Esc]);
}

#[test]
fn s_on_settings_lane_emits_settings_save() {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    app.settings = Some(SettingsState::new(serde_json::json!({})));
    let actions = raul::tui::modes::settings::handle_key(
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty()),
        &app,
    );
    assert_eq!(actions, vec![Action::SettingsSave]);
}
