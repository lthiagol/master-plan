//! M147/M169: Settings lane automation — agent block + batch save dry-run.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::SettingsFocus;
use raul::tui::modes::settings::{value_for_key, SETTINGS_KEYS};
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
    assert!(status.success(), "mp init failed");
    let mut runner = MpRunner::with_mp_bin(mp_bin());
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

fn config_path(tmp: &TempDir) -> PathBuf {
    tmp.path().join("master-plan/config.json")
}

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

fn section_index(name: &str) -> Option<usize> {
    SETTINGS_KEYS.iter().position(|(s, _)| *s == name)
}

#[test]
fn settings_sections_register_agent_automation_block() {
    let idx = section_index("agent").expect("`agent` section must exist");
    let expected = [
        "agent.automation.commit_after_execute",
        "agent.automation.push_after_review",
        "agent.automation.branch_strategy",
        "agent.automation.auto_remediate",
    ];
    for (offset, want) in expected.iter().enumerate() {
        let (s, k) = SETTINGS_KEYS[idx + offset];
        assert_eq!((s, k), ("agent", *want));
    }
}

#[test]
fn settings_modal_surfaces_agent_section_via_navigation() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    let max_steps = SETTINGS_KEYS.len() + 2;
    let mut reached = false;
    for _ in 0..max_steps {
        if app
            .settings
            .as_ref()
            .map(|s| SETTINGS_KEYS.get(s.selected_idx).map(|(sec, _)| *sec) == Some("agent"))
            .unwrap_or(false)
        {
            reached = true;
            break;
        }
        apply_action(&mut app, &runner, Action::Down).unwrap();
    }
    assert!(
        reached,
        "Settings flat list must surface the `agent` section"
    );

    apply_action(&mut app, &runner, Action::Enter).unwrap();
    let state = app.settings.as_ref().expect("settings");
    assert_eq!(state.focus, SettingsFocus::Editing);
    assert!(state.edit.is_some());
}

#[test]
fn settings_edit_persists_commit_after_execute_to_disk() {
    let (tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    let target = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "agent.automation.commit_after_execute")
        .expect("key must exist");
    for _ in 0..target {
        apply_action(&mut app, &runner, Action::Down).unwrap();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "true".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();

    let state = app.settings.as_ref().expect("settings");
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
    let v = value_for_key(&state.config, "agent.automation.commit_after_execute");
    assert_eq!(v, "true");

    let raw = std::fs::read_to_string(config_path(&tmp)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["agent"]["automation"]["commit_after_execute"], true);
}

#[test]
fn settings_automation_branch_strategy_field_accepts_all_enum_values() {
    let (tmp, _runner) = fixture_env();
    for v in ["per-milestone", "current", "none"] {
        let out = Command::new(mp_bin())
            .args([
                "config",
                "set",
                "agent.automation.branch_strategy",
                v,
                "--format",
                "json",
            ])
            .current_dir(tmp.path())
            .output()
            .expect("mp config set");
        assert!(out.status.success());
    }
    let raw = std::fs::read_to_string(config_path(&tmp)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["agent"]["automation"]["branch_strategy"], "none");
}

#[test]
fn settings_automation_invalid_value_rejected_by_dry_run() {
    let (_tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    let target = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "agent.automation.branch_strategy")
        .expect("key must exist");
    for _ in 0..target {
        apply_action(&mut app, &runner, Action::Down).unwrap();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "foo".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    let edit = app
        .settings
        .as_ref()
        .unwrap()
        .edit
        .as_ref()
        .expect("invalid value must keep edit open");
    assert!(!edit.errors.is_empty());
}

#[test]
fn save_dry_run_first_then_commit_per_staged_edit() {
    let (tmp, runner) = fixture_env();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    // Stage ui.hide_done = true
    let target = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "ui.hide_done")
        .unwrap();
    for _ in 0..target {
        apply_action(&mut app, &runner, Action::Down).unwrap();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "true".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    // Stage ui.color = false (walk back to top)
    for _ in 0..target {
        apply_action(&mut app, &runner, Action::Up).unwrap();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();

    assert_eq!(app.settings.as_ref().unwrap().staged_edits.len(), 2);
    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();
    assert!(app.settings.as_ref().unwrap().staged_edits.is_empty());

    let raw = std::fs::read_to_string(config_path(&tmp)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ui"]["hide_done"], true);
    assert_eq!(v["ui"]["color"], false);
}
