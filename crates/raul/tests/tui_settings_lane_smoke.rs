//! M164/M169: Settings is a 5th lane with lane-scoped state.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::modes;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn fixture_runner() -> (TempDir, MpRunner) {
    let tmp = TempDir::new().expect("temp");
    let root = tmp.path().to_path_buf();
    // M194: probe release + debug + PATH. CI builds --release so
    // target/debug/mp doesn't exist; the previous lookup assumed debug.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mp_bin = [
        manifest.join("../../target/release/mp"),
        manifest.join("../../target/debug/mp"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .unwrap_or_else(|| PathBuf::from("mp"));
    let status = Command::new(&mp_bin)
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(&root)
        .status()
        .expect("mp init");
    assert!(status.success());
    let mut runner = MpRunner::with_mp_bin(mp_bin);
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

#[test]
fn lane_ordered_has_seven_ending_in_settings() {
    // M184: 7 lanes (Tweaks folded into Backlog; Grooming tab gone).
    // Settings remains the last tab.
    let lanes = Lane::ordered();
    assert_eq!(lanes.len(), 7);
    assert_eq!(lanes[6], Lane::Settings);
    assert_eq!(Lane::Settings.label(), "Settings");
    assert_eq!(Lane::Settings.compact_label(), "Set");
}

#[test]
fn select_settings_lane_loads_settings_state() {
    let (_tmp, runner) = fixture_runner();
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    // M198: JumpLane indices are resolved against the VISIBLE lane
    // list (Watch omitted when `ui.show_autopilot_tab` is off, which is
    // the App::new() default). With Watch hidden, Settings is the
    // last of 6 visible lanes → index 5.
    let idx = Lane::ordered_visible(app.show_autopilot_tab)
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(&mut app, &runner, Action::JumpLane(idx)).expect("jump");
    assert_eq!(app.active_lane, Lane::Settings);
    assert!(app.settings.is_some());
    assert_eq!(app.active_mode, raul::tui::mode::Mode::Normal);
}

#[test]
fn esc_on_settings_lane_is_noop_without_active_edit() {
    let (_tmp, runner) = fixture_runner();
    let mut app = App::new();
    let idx = Lane::ordered_visible(app.show_autopilot_tab)
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(&mut app, &runner, Action::JumpLane(idx)).unwrap();
    let actions = modes::settings::handle_key(key(KeyCode::Esc), &app);
    assert!(actions.is_empty());
    assert_eq!(app.active_lane, Lane::Settings);
}

#[test]
fn jump_lane_index_6_is_settings() {
    // M184: the FULL lane list has 7 lanes; Settings is at index 6
    // (Overview, Milestones, Path, Backlog, Ideas, Watch, Settings).
    let lanes = Lane::ordered();
    assert_eq!(lanes.get(6), Some(&Lane::Settings));
    // M198: the VISIBLE list (Watch hidden) puts Settings at index 5.
    let visible = Lane::ordered_visible(false);
    assert_eq!(visible.get(5), Some(&Lane::Settings));
}
