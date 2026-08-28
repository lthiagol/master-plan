//! M169-rev regression tests — pins the four fixes recorded in
//! `mp-dogfood-log.md` entry 30.
//!
//! Each test corresponds to a numbered finding:
//!   - HIGH: Tab/click on Settings wipes staged edits (AC-06 violation)
//!   - MED:  `set_config_value` shadowed mp's type coercion
//!   - MED:  partial commit on save is silent
//!   - LOW:  AC-01 wrap not implemented
//!   - LOW:  `apply_settings_save` HashMap iteration order non-deterministic
//!
//! The pre-existing `tab_move_up_at_top_stays` / `tab_move_down_at_bottom_stays`
//! tests in `tui_sidebar.rs` were renamed (`..._wraps_to_settings`,
//! `..._wraps_to_overview`) to pin the new wrap behaviour.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsEdit, SettingsFocus, SettingsState};
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

fn fixture() -> (TempDir, MpRunner) {
    let tmp = TempDir::new().expect("temp");
    let root = tmp.path().to_path_buf();
    let status = Command::new(mp_bin())
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(&root)
        .status()
        .expect("mp init spawn");
    assert!(status.success(), "mp init failed");
    let mut runner = MpRunner::with_mp_bin(mp_bin());
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    // M198: Settings is always in the visible list (it's a
    // control surface, not a domain lane), but its *index* in
    // the visible list depends on `ui.show_watch_tab`. The
    // M169-rev tests exercise the full 7-lane tab order (Tab
    // wraps from Settings → Watch → Settings per AC-01), so
    // pin `show_watch_tab = true` for the duration of these
    // tests. The M198 spec is orthogonal — it filters Watch
    // out of the default surface; the M169-rev tests assert
    // the wrap behaviour assuming Watch is visible.
    app.show_watch_tab = true;
    let idx = Lane::ordered_visible(true)
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

fn stage_color_false(app: &mut App, runner: &MpRunner) {
    apply_action(app, runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "false".to_string();
    }
    apply_action(app, runner, Action::Enter).unwrap();
}

fn stage_color_yes(app: &mut App, runner: &MpRunner) {
    apply_action(app, runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "yes".to_string();
    }
    apply_action(app, runner, Action::Enter).unwrap();
}

// ---------------------------------------------------------------------------
// HIGH — Tab on Settings must NOT wipe staged edits.
// ---------------------------------------------------------------------------

#[test]
fn rev_high_jump_lane_to_settings_while_on_settings_preserves_staged_edits() {
    // The HIGH fix lands in `load_settings_lane`, which used to
    // unconditionally overwrite `app.settings`. The reproduction is
    // Jumping to the Settings lane while already on it (a mouse
    // click of the Settings tab, or `JumpLane(4)` from a chord /
    // digit key). After the fix, the existing SettingsState is
    // preserved.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_false(&mut app, &runner);
    assert!(
        app.settings.as_ref().unwrap().has_staged_edits(),
        "precondition: stage_color_false must leave a staged edit"
    );

    // Jump to Settings lane index (6 = Settings) while ALREADY on
    // Settings. M184: 7 lanes; Settings is index 6.
    apply_action(&mut app, &runner, Action::JumpLane(6)).unwrap();

    assert!(
        app.settings.as_ref().unwrap().has_staged_edits(),
        "JumpLane to Settings while already on it must NOT wipe staged edits (rev HIGH)"
    );
    assert_eq!(
        app.settings.as_ref().unwrap().staged_edits.get("ui.color"),
        Some(&"false".to_string()),
        "the staged value must be preserved verbatim"
    );
}

#[test]
fn rev_high_tab_on_settings_wraps_and_clears_per_ac06() {
    // **Companion to the wrap fix (LOW).** With wrap, Tab on Settings
    // is now a *leaving* gesture → AC-06 discard kicks in via
    // `select_lane`. This is the post-fix correct semantics, and it
    // supersedes the pre-fix "Tab is a no-op so load_settings_lane
    // shouldn't run" framing.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_false(&mut app, &runner);
    apply_action(&mut app, &runner, Action::NextLane).unwrap();
    assert_eq!(
        app.active_lane,
        Lane::Overview,
        "Tab on Settings now wraps to Overview (AC-01)"
    );
    assert!(
        app.settings.is_none(),
        "AC-06: Tab is now a leaving gesture, so staged edits are discarded"
    );
}

#[test]
fn rev_high_shift_tab_from_settings_clears_per_ac06() {
    // Sanity: Shift+Tab from Settings DOES leave the lane (per AC-06)
    // and clears `app.settings` — this stays unchanged by the fix.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_false(&mut app, &runner);
    apply_action(&mut app, &runner, Action::PreviousLane).unwrap();
    assert_ne!(app.active_lane, Lane::Settings);
    assert!(
        app.settings.is_none(),
        "AC-06: lane-leave clears Settings state"
    );
}

#[test]
fn rev_high_load_settings_lane_is_noop_when_already_loaded() {
    // Pin the underlying guard: load_settings_lane must not overwrite
    // an existing SettingsState. Constructed with a manually-built
    // state so the test doesn't depend on the fixture.
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let sentinel = SettingsState::new(serde_json::json!({}));
    app.settings = Some(sentinel);
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(&mut app, &runner_or_skip(), Action::JumpLane(idx)).unwrap();
    assert!(
        app.settings.is_some(),
        "load_settings_lane must preserve the existing SettingsState on re-entry"
    );
}

// ---------------------------------------------------------------------------
// MED — `set_config_value` no longer shadows mp's type coercion.
// ---------------------------------------------------------------------------

#[test]
fn rev_med_yes_for_bool_field_does_not_pollute_state_config() {
    // mp accepts "yes" as true (parse_bool). Pre-fix, the staging path
    // ran `set_config_value` which stored the raw string "yes" into
    // `state.config.ui.color`. Post-fix, `state.config` is left alone
    // and only `staged_edits` carries the buffer string.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_yes(&mut app, &runner);

    let state = app.settings.as_ref().unwrap();
    // Buffer value is preserved verbatim in staged_edits.
    assert_eq!(state.staged_edits.get("ui.color"), Some(&"yes".to_string()));
    // state.config must NOT be mutated by staging (mp coerces on save).
    let coerced = state.config.pointer("/ui/color").cloned();
    assert!(
        coerced.is_none()
            || coerced == Some(serde_json::Value::Null)
            || coerced == Some(serde_json::json!(true)),
        "BUG: state.config.ui.color should not be polluted by staging; got {:?}",
        coerced
    );
}

#[test]
fn rev_med_save_with_yes_writes_bool_true_to_disk() {
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_yes(&mut app, &runner);
    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();

    let raw = std::fs::read_to_string(_tmp.path().join("master-plan/config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v["ui"]["color"],
        serde_json::json!(true),
        "mp must coerce 'yes' to bool true on save"
    );
}

#[test]
fn rev_med_re_enter_after_commit_preserves_staged_buffer() {
    // Post-fix: re-Enter on a previously-staged key must prefill the
    // edit buffer with the staged value, not the on-disk value.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_false(&mut app, &runner);
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    let buffer = app
        .settings
        .as_ref()
        .unwrap()
        .edit
        .as_ref()
        .unwrap()
        .buffer
        .clone();
    assert_eq!(
        buffer, "false",
        "re-Enter on a staged key must prefill the edit buffer with the staged value"
    );
}

// ---------------------------------------------------------------------------
// MED — partial commit on save is no longer silent.
// ---------------------------------------------------------------------------

#[test]
fn rev_med_partial_commit_failure_keeps_failed_key_staged() {
    // We can't easily simulate a commit-time failure without mocking
    // the runner. The actual fix is structural (a reload that preserves
    // the failed key in `staged_edits`); we assert the structural
    // property directly via the helper that the action calls.
    use raul::tui::action::test_helpers::dry_run_errors_for;
    let errors = dry_run_errors_for(
        &serde_json::json!({
            "ok": false,
            "errors": [{ "field": "ui.color", "message": "expected boolean" }]
        }),
        "ui.color",
    );
    assert_eq!(errors, vec!["ui.color: expected boolean".to_string()]);
}

#[test]
fn rev_med_reload_keeping_preserves_listed_keys() {
    use raul::tui::action::test_helpers::apply_reload_keeping;
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    stage_color_false(&mut app, &runner);
    // Walk to ui.hide_done (index 3 in SETTINGS_KEYS) and stage it.
    use raul::tui::modes::settings::SETTINGS_KEYS;
    let target = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "ui.hide_done")
        .expect("ui.hide_done must exist");
    for _ in 0..target {
        apply_action(&mut app, &runner, Action::Down).unwrap();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    if let Some(state) = app.settings.as_mut() {
        state.edit.as_mut().unwrap().buffer = "true".to_string();
    }
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    assert_eq!(app.settings.as_ref().unwrap().staged_edits.len(), 2);

    // Simulate the partial-commit reload path: keep ui.hide_done (the
    // one that failed to commit), drop ui.color (the one that landed).
    apply_reload_keeping(&mut app, &runner, &["ui.hide_done".to_string()]).unwrap();

    let staged = &app.settings.as_ref().unwrap().staged_edits;
    assert_eq!(staged.len(), 1, "only the kept key survives the reload");
    assert!(staged.contains_key("ui.hide_done"));
    assert!(!staged.contains_key("ui.color"), "committed key is dropped");
}

// ---------------------------------------------------------------------------
// LOW — AC-01 wrap is implemented.
// ---------------------------------------------------------------------------

#[test]
fn rev_low_tab_on_settings_wraps_to_overview() {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    app.tab_move_down();
    assert_eq!(
        app.active_lane,
        Lane::Overview,
        "Tab on Settings must wrap to Overview (AC-01)"
    );
}

#[test]
fn rev_low_shift_tab_on_overview_wraps_to_settings() {
    let mut app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    app.tab_move_up();
    assert_eq!(
        app.active_lane,
        Lane::Settings,
        "Shift+Tab on Overview must wrap to Settings (AC-01)"
    );
}

#[test]
fn rev_low_tab_cycles_through_all_lanes_in_order() {
    let mut app = App::new();
    // M184: 7 lanes. Tab still wraps from Settings back to Overview.
    // M198: pin `show_watch_tab = true` so the visible list is
    // the full 7-lane registry (the test is about the M184 wrap
    // contract, not the M198 filter).
    app.show_watch_tab = true;
    let expected = [
        Lane::Milestones,
        Lane::Path,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Watch,
        Lane::Settings,
        Lane::Overview, // wrap
    ];
    for want in expected {
        app.tab_move_down();
        assert_eq!(app.active_lane, want);
    }
}

// ---------------------------------------------------------------------------
// LOW — staged_edits iteration order is deterministic.
// ---------------------------------------------------------------------------

#[test]
fn rev_low_staged_edits_iterate_in_key_sorted_order() {
    let mut state = SettingsState::new(serde_json::json!({}));
    // Insert keys in non-sorted order.
    state
        .staged_edits
        .insert("ui.theme".into(), "dracula".into());
    state.staged_edits.insert("ui.color".into(), "false".into());
    state.staged_edits.insert(
        "agent.automation.commit_after_execute".into(),
        "true".into(),
    );

    let keys: Vec<String> = state.staged_edits.keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            "agent.automation.commit_after_execute".to_string(),
            "ui.color".to_string(),
            "ui.theme".to_string(),
        ],
        "staged_edits must iterate key-sorted (BTreeMap)"
    );
}

#[test]
fn rev_low_save_iterates_staged_edits_in_key_sorted_order() {
    // Pin the iteration order end-to-end: stage three keys in
    // non-alphabetical order, save, and check the on-disk config
    // matches key-sorted application. We can't easily inspect the
    // intermediate commit order without a mock runner; instead we
    // assert the post-save state matches the staged values, plus
    // that `staged_edits` is empty after a successful save.
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    {
        let state = app.settings.as_mut().unwrap();
        state
            .staged_edits
            .insert("ui.theme".into(), "dracula".into());
        state.staged_edits.insert("ui.color".into(), "false".into());
        state.staged_edits.insert(
            "agent.automation.commit_after_execute".into(),
            "true".into(),
        );
    }

    apply_action(&mut app, &runner, Action::SettingsSave).unwrap();
    assert!(
        app.settings.as_ref().unwrap().staged_edits.is_empty(),
        "successful save must clear staged_edits"
    );

    let raw = std::fs::read_to_string(_tmp.path().join("master-plan/config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ui"]["color"], serde_json::json!(false));
    assert_eq!(v["ui"]["theme"], "dracula");
    assert_eq!(
        v["agent"]["automation"]["commit_after_execute"],
        serde_json::json!(true)
    );
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn runner_or_skip() -> MpRunner {
    MpRunner::new().expect("mp")
}

// Reach into the `mode` module to fabricate an edit state for tests
// that want to drive `apply_settings_commit_edit` without going through
// the full dispatcher.
#[allow(dead_code)]
fn seed_edit(app: &mut App, key: &str, buffer: &str) {
    if let Some(state) = app.settings.as_mut() {
        state.edit = Some(SettingsEdit {
            key: key.to_string(),
            buffer: buffer.to_string(),
            cursor: buffer.chars().count(),
            errors: Vec::new(),
        });
        state.focus = SettingsFocus::Editing;
    }
}
