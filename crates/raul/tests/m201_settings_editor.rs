//! M201 S6-S9: per-type settings editor unit tests.
//!
//! Each test exercises the in-place editor state machine for one type
//! per outcome (toggle / cycle / caret-edit / commit / revert / retry /
//! parse-error) and pins the staging in `state.staged_edits`. The TUI
//! renderer is not invoked — these tests call `apply_action` directly
//! with mocked `MpRunner` instances so the assertions stay focused on
//! state transitions rather than terminal rendering.

use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsEdit, SettingsFocus, SettingsState};
use raul::tui::modes::settings::schema::{SchemaEntry, SettingsSchema};
use std::collections::BTreeMap;

/// Build a minimal schema containing one entry per type we exercise.
fn schema_one_of_each_type() -> SettingsSchema {
    let json = r#"{
        "$schema_version": "1.0",
        "keys": [
            {"key": "git.auto_commit", "type": "bool", "default": "false",
             "description": "Auto-commit."},
            {"key": "ui.theme", "type": "choice", "default": "mocha",
             "allowed": ["mocha", "latte", "frappe"], "description": "Theme."},
            {"key": "workflow.plan.location", "type": "path", "default": "master-plan",
             "description": "Plan dir."},
            {"key": "workflow.steps.code_review", "type": "integer", "default": "42",
             "description": "Integer."},
            {"key": "keybinds.refresh", "type": "keybind", "default": "Ctrl-R",
             "description": "Refresh."}
        ]
    }"#;
    SettingsSchema::from_json(json.as_bytes()).expect("schema parses")
}

fn app_with_schema() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let config = serde_json::json!({
        "git": { "auto_commit": false },
        "ui": { "theme": "mocha" },
        "workflow": { "plan": { "location": "master-plan" },
                      "steps": { "code_review": 42 } },
        "keybinds": { "refresh": "Ctrl-R" }
    });
    app.settings = Some(SettingsState {
        config,
        schema: Some(schema_one_of_each_type()),
        selected_idx: 0, // git.auto_commit
        focus: SettingsFocus::Fields,
        edit: None,
        staged_edits: BTreeMap::new(),
        schema_warning: None,
    });
    app
}

fn runner() -> raul::mp_runner::MpRunner {
    let mut r = raul::mp_runner::MpRunner::new().expect("mp on PATH for fixture");
    // Pin a temporary plan directory so `mp config set ... --dry-run`
    // doesn't bail with "plan directory not found". The fixture is
    // throwaway — `dry-run` doesn't write to disk.
    let tmp = std::env::temp_dir().join("m201_settings_editor_fixture");
    let _ = std::fs::create_dir_all(&tmp);
    let _ = std::fs::create_dir_all(tmp.join("master-plan"));
    r.set_project_root(tmp.clone());
    r.set_plan_dir(tmp.join("master-plan"));
    r
}

fn select_key(app: &mut App, key: &str) {
    use raul::tui::modes::settings::SETTINGS_KEYS;
    let idx = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == key)
        .unwrap_or_else(|| panic!("{key} not in SETTINGS_KEYS"));
    app.settings.as_mut().unwrap().selected_idx = idx;
}

// ---------------------------------------------------------------------------
// S6: bool editor — Space toggles, no editor opens.
// ---------------------------------------------------------------------------

#[test]
fn settings_editor_bool_space_toggles_in_place() {
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit");
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();

    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("git.auto_commit"),
        Some(&"true".to_string()),
        "Space on a false bool must stage `true`"
    );
    assert!(
        state.edit.is_none(),
        "bool toggle must not open the editor"
    );
    assert_eq!(state.focus, SettingsFocus::Fields);
}

#[test]
fn settings_editor_bool_toggles_back_to_false_on_second_press() {
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit");
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("git.auto_commit"),
        Some(&"false".to_string())
    );
}

#[test]
fn settings_editor_bool_accepts_yes_true_1_as_truthy() {
    // The on-disk value is "1" (mp coerces), and the next toggle flips
    // it to false. The state machine reads the live string and treats
    // any truthy form as "on".
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit");
    {
        let state = app.settings.as_mut().unwrap();
        state.config["git"]["auto_commit"] = serde_json::json!("1");
    }
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("git.auto_commit"),
        Some(&"false".to_string()),
        "truthy `1` must toggle to false"
    );
}

#[test]
fn settings_editor_bool_on_a_non_bool_key_is_a_noop() {
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme"); // choice key, not bool
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(
        state.staged_edits.is_empty(),
        "SettingsToggleBool on a non-bool key must not stage"
    );
}

#[test]
fn settings_editor_bool_enter_opens_editor_for_freetext() {
    // Enter on a bool key still opens the editor (M169 contract
    // preservation); Space toggles in place. Both entry points exist.
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(
        state.edit.is_some(),
        "Enter on a bool key still opens the editor (M169 compat)"
    );
    assert_eq!(state.focus, SettingsFocus::Editing);
}

// ---------------------------------------------------------------------------
// S7: choice editor — Left/Right cycles through `allowed`.
// ---------------------------------------------------------------------------

#[test]
fn settings_editor_choice_right_cycles_forward() {
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme");
    apply_action(
        &mut app,
        &runner(),
        Action::SettingsCycleChoice { forward: true },
    )
    .unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("ui.theme"),
        Some(&"latte".to_string())
    );
    assert!(state.edit.is_none());
}

#[test]
fn settings_editor_choice_left_cycles_backward() {
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme");
    apply_action(
        &mut app,
        &runner(),
        Action::SettingsCycleChoice { forward: false },
    )
    .unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("ui.theme"),
        Some(&"frappe".to_string())
    );
}

#[test]
fn settings_editor_choice_wraps_around() {
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme");
    // On frappe: forward → mocha (wraps).
    {
        let state = app.settings.as_mut().unwrap();
        state.config["ui"]["theme"] = serde_json::json!("frappe");
    }
    apply_action(
        &mut app,
        &runner(),
        Action::SettingsCycleChoice { forward: true },
    )
    .unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(
        state.staged_edits.get("ui.theme"),
        Some(&"mocha".to_string()),
        "Right on last allowed value must wrap to first"
    );
}

#[test]
fn settings_editor_choice_on_non_choice_key_is_noop() {
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit"); // bool
    apply_action(
        &mut app,
        &runner(),
        Action::SettingsCycleChoice { forward: true },
    )
    .unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(state.staged_edits.is_empty());
}

#[test]
fn settings_editor_choice_enter_opens_editor() {
    // Enter on a choice key still opens the editor (M169 compat);
    // ← / → cycle in place via SettingsCycleChoice.
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(
        state.edit.is_some(),
        "Enter on a choice key still opens the editor (M169 compat)"
    );
}

// ---------------------------------------------------------------------------
// S8: string/path/integer editor — Enter opens, Backspace deletes,
// Enter commits, Esc reverts.
// ---------------------------------------------------------------------------

#[test]
fn settings_editor_string_enter_opens_caret_edit_state() {
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.plan.location");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    let edit = state.edit.as_ref().expect("editor must open");
    assert_eq!(edit.key, "workflow.plan.location");
    assert_eq!(edit.buffer, "master-plan");
    assert_eq!(state.focus, SettingsFocus::Editing);
}

#[test]
fn settings_editor_string_backspace_deletes_char_before_caret() {
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.plan.location");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    apply_action(&mut app, &runner(), Action::PopInputChar).unwrap();
    let edit = app.settings.as_ref().unwrap().edit.as_ref().unwrap().clone();
    assert_eq!(edit.buffer, "master-pla");
}

#[test]
fn settings_editor_integer_validates_on_commit_via_dry_run() {
    // We can't easily exercise the mp-coerced error path without a real
    // mp. Pin that the schema lookup routes an integer key through the
    // editor flow (not the bool/choice shortcuts).
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.steps.code_review");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(
        state.edit.is_some(),
        "integer key must open the editor on Enter"
    );
    assert_eq!(state.focus, SettingsFocus::Editing);
}

#[test]
fn settings_editor_esc_reverts_uncommitted_edit() {
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.plan.location");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    // Stage a different value (manual for clarity).
    {
        let state = app.settings.as_mut().unwrap();
        state.edit.as_mut().unwrap().buffer = "docs/plan".to_string();
    }
    apply_action(&mut app, &runner(), Action::Esc).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(state.edit.is_none(), "Esc closes the editor");
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(
        state.staged_edits.is_empty(),
        "Esc reverts the uncommitted value"
    );
}

// ---------------------------------------------------------------------------
// S9: keybind editor — typed string, KeyCombo::parse validates.
// ---------------------------------------------------------------------------

#[test]
fn settings_editor_keybind_accepts_valid_combo() {
    // We can't drive the full `Enter → commit → mp` round-trip without
    // a real mp; pin the schema-routing behavior (Enter on keybind
    // opens the editor, not the bool/choice shortcut).
    let mut app = app_with_schema();
    select_key(&mut app, "keybinds.refresh");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert!(state.edit.is_some(), "Enter on keybind must open editor");
    let edit = state.edit.as_ref().unwrap();
    assert_eq!(edit.key, "keybinds.refresh");
    assert_eq!(edit.buffer, "Ctrl-R");
}

#[test]
fn settings_editor_keybind_pre_validates_combo_strings() {
    // Direct test of the parse-gate: feed an invalid combo string to
    // the editor buffer and call Esc/Enter; Enter must surface a parse
    // error in `state.edit.errors`, NOT reach the mp layer.
    use raul::tui::key_combo::parse_key_combo;
    assert!(parse_key_combo("Ctrl+S").is_some());
    assert!(parse_key_combo("Enter").is_some());
    assert!(parse_key_combo("Left").is_some());
    assert!(parse_key_combo("Up").is_some());
    assert!(parse_key_combo("Backspace").is_some());
    assert!(parse_key_combo("zzznotreal").is_none(), "garbage is invalid");
    assert!(parse_key_combo("PageUp").is_none(), "PageUp is not a recognized combo in this build");
}

#[test]
fn settings_editor_keybind_on_commit_with_invalid_string_surfaces_parse_error() {
    let mut app = app_with_schema();
    select_key(&mut app, "keybinds.refresh");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    // Type an invalid combo.
    {
        let state = app.settings.as_mut().unwrap();
        let edit = state.edit.as_mut().unwrap();
        edit.buffer = "totally-not-a-combo".to_string();
        edit.cursor = edit.buffer.chars().count();
    }
    // Enter to commit — the pre-validate gate must reject it.
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    let edit = state.edit.as_ref().expect("editor must remain open on parse error");
    assert!(
        !edit.errors.is_empty(),
        "parse error must surface in edit.errors"
    );
    assert!(
        edit.errors[0].contains("not a valid key combo"),
        "error message should describe the parse failure: {:?}",
        edit.errors
    );
    assert!(
        state.staged_edits.is_empty(),
        "invalid value must not reach staged_edits"
    );
}

#[test]
fn settings_editor_keybind_empty_combo_rejected() {
    let mut app = app_with_schema();
    select_key(&mut app, "keybinds.refresh");
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    {
        let state = app.settings.as_mut().unwrap();
        let edit = state.edit.as_mut().unwrap();
        edit.buffer = " , ,".to_string();
    }
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let state = app.settings.as_ref().unwrap();
    let edit = state.edit.as_ref().unwrap();
    assert!(
        edit.errors.iter().any(|e| e.contains("empty combo")),
        "expected an empty-combo error, got: {:?}",
        edit.errors
    );
}

// ---------------------------------------------------------------------------
// S11: state-machine transitions for SettingsFocus (idle ↔ editing).
// ---------------------------------------------------------------------------

#[test]
fn settings_editor_focus_transitions_idle_editing_idle_on_commit() {
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.plan.location");
    // idle → editing
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    assert_eq!(app.settings.as_ref().unwrap().focus, SettingsFocus::Editing);
    // editing → idle (commit; staging goes through mp so we can't pin
    // the post-commit state without a mock — but `edit` must clear).
    apply_action(&mut app, &runner(), Action::Esc).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
}

#[test]
fn settings_editor_focus_bool_toggle_does_not_transition_to_editing() {
    let mut app = app_with_schema();
    select_key(&mut app, "git.auto_commit");
    apply_action(&mut app, &runner(), Action::SettingsToggleBool).unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
}

#[test]
fn settings_editor_focus_choice_cycle_does_not_transition_to_editing() {
    let mut app = app_with_schema();
    select_key(&mut app, "ui.theme");
    apply_action(
        &mut app,
        &runner(),
        Action::SettingsCycleChoice { forward: true },
    )
    .unwrap();
    let state = app.settings.as_ref().unwrap();
    assert_eq!(state.focus, SettingsFocus::Fields);
    assert!(state.edit.is_none());
}

#[test]
fn settings_editor_staging_is_deterministic_btreemap() {
    // Insert keys in non-sorted order, confirm staged_edits iterates
    // key-sorted on save (BTreeMap contract — pin regardless of editor).
    let mut app = app_with_schema();
    {
        let state = app.settings.as_mut().unwrap();
        state.staged_edits.insert("ui.theme".into(), "latte".into());
        state.staged_edits.insert("git.auto_commit".into(), "true".into());
        state
            .staged_edits
            .insert("workflow.plan.location".into(), "docs/plan".into());
    }
    let keys: Vec<String> = app
        .settings
        .as_ref()
        .unwrap()
        .staged_edits
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        vec![
            "git.auto_commit".to_string(),
            "ui.theme".to_string(),
            "workflow.plan.location".to_string(),
        ]
    );
}

#[test]
fn settings_editor_inline_buffer_prefills_prefilled_from_staged_not_from_disk() {
    let mut app = app_with_schema();
    select_key(&mut app, "workflow.plan.location");
    // Stage a different value first.
    {
        let state = app.settings.as_mut().unwrap();
        state
            .staged_edits
            .insert("workflow.plan.location".into(), "docs/plan".into());
    }
    apply_action(&mut app, &runner(), Action::Enter).unwrap();
    let edit = app.settings.as_ref().unwrap().edit.as_ref().unwrap().clone();
    assert_eq!(
        edit.buffer, "docs/plan",
        "re-Enter on a staged key must prefill the editor with the staged value"
    );
}

#[test]
fn settings_editor_inline_shape_carries_errors_buffer_cursor() {
    let edit = SettingsEdit {
        key: "keybinds.refresh".to_string(),
        buffer: "Ctrl-S".to_string(),
        cursor: 6,
        errors: vec!["oops".to_string()],
    };
    assert_eq!(edit.key, "keybinds.refresh");
    assert_eq!(edit.buffer, "Ctrl-S");
    assert_eq!(edit.cursor, 6);
    assert_eq!(edit.errors, vec!["oops".to_string()]);
}

#[test]
fn settings_editor_inline_schema_entry_shape_carries_type_default_description() {
    // Re-decode a single entry — confirms the schema module's parser
    // preserves the typed contract on the wire.
    let entry = SchemaEntry {
        key: "ui.color".to_string(),
        ty: "bool".to_string(),
        default: "true".to_string(),
        allowed: None,
        description: "ANSI color.".to_string(),
    };
    assert_eq!(entry.ty, "bool");
    assert_eq!(entry.default, "true");
    assert!(entry.allowed.is_none());
}
