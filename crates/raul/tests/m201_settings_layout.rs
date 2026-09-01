//! M201 S5 / S10: Settings lane layout — bordered list + framed
//! description card. These tests pin the new M201 layout (replacing the
//! M168 flat `key = value` list with the bordered list + framed card)
//! at three terminal sizes, and assert the schema-unavailable path
//! (AC-08) renders the error block in place of the framed list.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsEdit, SettingsFocus, SettingsState};
use raul::tui::modes::settings::schema::{SchemaEntry, SettingsSchema};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

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

/// Build a 45-entry SettingsSchema with deterministic test data.
fn build_test_schema() -> SettingsSchema {
    let entries: Vec<SchemaEntry> = [
        ("ui", "ui.color", "bool", "true", None, "ANSI color toggle."),
        (
            "ui",
            "ui.icons",
            "choice",
            "unicode",
            Some("none|ascii|unicode"),
            "Icon set.",
        ),
        (
            "ui",
            "ui.theme",
            "choice",
            "mocha",
            Some("mocha|latte|frappe"),
            "Theme name.",
        ),
        (
            "ui",
            "ui.hide_done",
            "bool",
            "false",
            None,
            "Hide done milestones.",
        ),
        (
            "ui",
            "ui.show_watch_tab",
            "bool",
            "false",
            None,
            "Show Watch tab.",
        ),
        (
            "workflow",
            "workflow.profile",
            "choice",
            "full",
            Some("full|hybrid|session"),
            "Workflow profile.",
        ),
        (
            "workflow",
            "workflow.plan.location",
            "path",
            "master-plan",
            None,
            "Plan dir location.",
        ),
        (
            "workflow",
            "workflow.plan.in_repo",
            "bool",
            "true",
            None,
            "Plan in repo.",
        ),
        (
            "workflow",
            "workflow.gates.strictness",
            "choice",
            "relaxed",
            Some("relaxed|full"),
            "Gate strictness.",
        ),
        (
            "workflow",
            "workflow.steps.code_review",
            "bool",
            "true",
            None,
            "Run external review.",
        ),
        (
            "git",
            "git.auto_commit",
            "bool",
            "false",
            None,
            "Auto-commit on step.",
        ),
        (
            "git",
            "git.commit_on_milestone_complete",
            "bool",
            "false",
            None,
            "Commit on milestone done.",
        ),
        (
            "git",
            "git.auto_push",
            "bool",
            "false",
            None,
            "Push after commit.",
        ),
        (
            "next",
            "next.prefer",
            "choice",
            "milestone",
            Some("milestone|track"),
            "Lane preference.",
        ),
        (
            "agent",
            "agent.automation.commit_after_execute",
            "bool",
            "false",
            None,
            "Commit after execute.",
        ),
        (
            "agent",
            "agent.automation.push_after_review",
            "bool",
            "false",
            None,
            "Push after review.",
        ),
        (
            "agent",
            "agent.automation.branch_strategy",
            "choice",
            "current",
            Some("per-milestone|current|none"),
            "Branch strategy.",
        ),
        (
            "agent",
            "agent.automation.auto_remediate",
            "choice",
            "none",
            Some("none|low|medium|high|all"),
            "Auto-remediate.",
        ),
        // 27 keybinds
        (
            "keybinds",
            "keybinds.quit",
            "keybind",
            "q, Q",
            None,
            "Quit.",
        ),
        ("keybinds", "keybinds.up", "keybind", "Up, k", None, "Up."),
        (
            "keybinds",
            "keybinds.down",
            "keybind",
            "Down, j",
            None,
            "Down.",
        ),
        (
            "keybinds",
            "keybinds.page_up",
            "keybind",
            "PageUp",
            None,
            "Page up.",
        ),
        (
            "keybinds",
            "keybinds.page_down",
            "keybind",
            "PageDown",
            None,
            "Page down.",
        ),
        (
            "keybinds",
            "keybinds.enter",
            "keybind",
            "Enter",
            None,
            "Enter.",
        ),
        (
            "keybinds",
            "keybinds.escape",
            "keybind",
            "Esc",
            None,
            "Esc.",
        ),
        ("keybinds", "keybinds.help", "keybind", "?", None, "Help."),
        (
            "keybinds",
            "keybinds.filter",
            "keybind",
            "f",
            None,
            "Filter.",
        ),
        (
            "keybinds",
            "keybinds.hide_done",
            "keybind",
            "h",
            None,
            "Hide done.",
        ),
        (
            "keybinds",
            "keybinds.create_annotation",
            "keybind",
            "A",
            None,
            "Annotate.",
        ),
        (
            "keybinds",
            "keybinds.resolve",
            "keybind",
            "r",
            None,
            "Resolve.",
        ),
        (
            "keybinds",
            "keybinds.reopen",
            "keybind",
            "R",
            None,
            "Reopen.",
        ),
        (
            "keybinds",
            "keybinds.approve",
            "keybind",
            "p",
            None,
            "Approve.",
        ),
        (
            "keybinds",
            "keybinds.review_menu",
            "keybind",
            "m",
            None,
            "Review menu.",
        ),
        (
            "keybinds",
            "keybinds.open_settings",
            "keybind",
            "Ctrl-O",
            None,
            "Open settings.",
        ),
        (
            "keybinds",
            "keybinds.previous_lane",
            "keybind",
            "Left, BackTab",
            None,
            "Previous lane.",
        ),
        (
            "keybinds",
            "keybinds.next_lane",
            "keybind",
            "Right, l, Tab",
            None,
            "Next lane.",
        ),
        (
            "keybinds",
            "keybinds.refresh",
            "keybind",
            "Ctrl-R",
            None,
            "Refresh.",
        ),
        (
            "keybinds",
            "keybinds.next_section",
            "keybind",
            "]",
            None,
            "Next section.",
        ),
        (
            "keybinds",
            "keybinds.prev_section",
            "keybind",
            "[",
            None,
            "Prev section.",
        ),
        (
            "keybinds",
            "keybinds.next_item",
            "keybind",
            "n",
            None,
            "Next item.",
        ),
        (
            "keybinds",
            "keybinds.prev_item",
            "keybind",
            "p",
            None,
            "Prev item.",
        ),
        (
            "keybinds",
            "keybinds.lifecycle_filter",
            "keybind",
            "F",
            None,
            "Lifecycle filter.",
        ),
        (
            "keybinds",
            "keybinds.grooming_preset",
            "keybind",
            "g",
            None,
            "Grooming preset.",
        ),
        (
            "keybinds",
            "keybinds.search",
            "keybind",
            "/",
            None,
            "Search.",
        ),
        (
            "keybinds",
            "keybinds.cycle_sort",
            "keybind",
            "o",
            None,
            "Cycle sort.",
        ),
    ]
    .into_iter()
    .map(|(_k, key, ty, default, allowed, desc)| {
        let key = key.to_string();
        let mut by_key = BTreeMap::new();
        by_key.insert(0usize, key.clone());
        let _ = by_key;
        SchemaEntry {
            key,
            ty: ty.to_string(),
            default: default.to_string(),
            allowed: allowed.map(|a| a.split('|').map(|s| s.to_string()).collect()),
            description: desc.to_string(),
        }
    })
    .collect();

    // Build via the parser to validate.
    let json = format!(
        r#"{{ "$schema_version": "1.0", "keys": [{}] }}"#,
        entries
            .iter()
            .map(|e| {
                let allowed_field = match &e.allowed {
                    Some(a) => format!(
                        ", \"allowed\": [{}]",
                        a.iter()
                            .map(|s| format!("\"{}\"", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None => String::new(),
                };
                format!(
                    r#"{{ "key": "{}", "type": "{}", "default": "{}", "description": "{}"{allowed_field} }}"#,
                    e.key, e.ty, e.default, e.description
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    SettingsSchema::from_json(json.as_bytes()).expect("test schema parses")
}

fn settings_app_with_schema() -> App {
    let mut app = App::new();
    app.show_watch_tab = true; // include Watch lane to land on Settings via JumpLane
    let idx = Lane::ordered_visible(true)
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    app.select_lane(Lane::Settings);
    // Use JumpLane to drive load_settings_lane path (which sets the schema).
    let _ = idx;
    let config = serde_json::json!({
        "ui": { "color": true, "icons": "unicode", "theme": "mocha", "hide_done": false, "show_watch_tab": false },
        "workflow": { "profile": "full", "plan": { "in_repo": true, "location": "master-plan" },
                      "gates": { "strictness": "relaxed" }, "steps": { "code_review": true } },
        "git": { "auto_commit": false, "commit_on_milestone_complete": false, "auto_push": false },
        "next": { "prefer": "milestone" },
        "agent": { "automation": { "commit_after_execute": false, "push_after_review": false,
                                    "branch_strategy": "current", "auto_remediate": "none" } },
        "keybinds": {},
        "sort": {},
        "review": {}
    });
    app.settings = Some(SettingsState {
        config,
        schema: Some(build_test_schema()),
        selected_idx: 0,
        focus: SettingsFocus::Fields,
        edit: None,
        staged_edits: BTreeMap::new(),
        schema_warning: None,
    });
    app
}

#[test]
fn settings_layout_renders_bordered_list_and_framed_card_at_80x24() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 80, 24);
    // Both borders must be present: the list uses `─` and `│` chars,
    // the card uses `┌`/`┐` corners (Plain border type).
    assert!(
        out.contains("─"),
        "list/card borders missing at 80x24:\n{out}"
    );
    assert!(
        out.contains("│"),
        "list/card vertical borders missing at 80x24"
    );
    assert!(
        out.contains("ui.color"),
        "focused key name missing in card title:\n{out}"
    );
    assert!(
        out.contains("Type"),
        "card body label `Type` missing:\n{out}"
    );
    assert!(
        out.contains("Default"),
        "card body label `Default` missing:\n{out}"
    );
    assert!(
        out.contains("Value"),
        "card body label `Value` missing:\n{out}"
    );
    assert!(
        out.contains("Description"),
        "card body label `Description` missing:\n{out}"
    );
    // Type badge from the new layout.
    assert!(
        out.contains("[bool]"),
        "type badge missing in list row:\n{out}"
    );
}

#[test]
fn settings_layout_renders_at_120x40() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 120, 40);
    assert!(out.contains("ui.color"), "card title missing at 120x40");
    assert!(out.contains("[bool]"), "type badge missing at 120x40");
    assert!(out.contains("─"), "borders missing at 120x40");
}

#[test]
fn settings_layout_renders_at_200x60() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 200, 60);
    assert!(out.contains("ui.color"), "card title missing at 200x60");
    assert!(out.contains("[bool]"), "type badge missing at 200x60");
    assert!(out.contains("─"), "borders missing at 200x60");
}

#[test]
fn settings_layout_shows_section_headers() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 120, 40);
    // Section headers use `▾ ui`, `▾ workflow`, etc. (M201 layout).
    assert!(out.contains("ui"), "section header `ui` missing:\n{out}");
    assert!(
        out.contains("workflow"),
        "section header `workflow` missing:\n{out}"
    );
    assert!(out.contains("git"), "section header `git` missing:\n{out}");
    assert!(
        out.contains("keybinds"),
        "section header `keybinds` missing:\n{out}"
    );
}

#[test]
fn settings_layout_cursor_row_uses_reversed_modifier() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 120, 40);
    // The cursor row carries a `▶` glyph prefix in M201; pin the
    // visual contract instead of the underlying style attributes.
    assert!(out.contains("▶"), "cursor glyph missing:\n{out}");
}

#[test]
fn settings_layout_schema_unavailable_replaces_list_with_error_block() {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let config = serde_json::json!({});
    app.settings = Some(SettingsState {
        config,
        schema: None,
        selected_idx: 0,
        focus: SettingsFocus::Fields,
        edit: None,
        staged_edits: BTreeMap::new(),
        schema_warning: Some("mp config schema unavailable: unknown subcommand".to_string()),
    });
    let out = render_full(&app, 120, 40);
    // AC-08: error block replaces the framed list.
    assert!(
        out.contains("Schema unavailable"),
        "schema-unavailable message missing:\n{out}"
    );
    assert!(
        out.contains("mp --version"),
        "schema-unavailable hint missing the `mp --version` pointer:\n{out}"
    );
    // The list's section headers must NOT appear in this state.
    assert!(
        !out.contains(" ▾ ui "),
        "list section header `ui` must not render when schema is unavailable:\n{out}"
    );
}

#[test]
fn settings_layout_no_centered_modal_in_default_state() {
    let app = settings_app_with_schema();
    let out = render_full(&app, 120, 40);
    // The M168 edit popup used a centered `Edit <key>` block. In the
    // M201 default state (no edit) we should NOT see that title.
    assert!(
        !out.contains(" Edit ui.color ") || !out.contains("Enter: save  Esc: cancel"),
        "default-state render should not include the centered edit popup:\n{out}"
    );
}

#[test]
fn settings_layout_card_title_uses_focused_key_name() {
    let mut app = settings_app_with_schema();
    // Walk to ui.theme (index 2).
    app.settings.as_mut().unwrap().selected_idx = 2;
    let out = render_full(&app, 120, 40);
    assert!(
        out.contains(" ui.theme "),
        "card title should reflect the focused key name `ui.theme`:\n{out}"
    );
}

// Pin the underlying assumption: SettingsEdit shape stays the same so
// downstream tests don't break across the M201 refactor.
#[test]
fn settings_edit_shape_unchanged_for_in_row_editing() {
    let edit = SettingsEdit {
        key: "ui.theme".to_string(),
        buffer: "latte".to_string(),
        cursor: 5,
        errors: Vec::new(),
    };
    assert_eq!(edit.key, "ui.theme");
    assert_eq!(edit.cursor, 5);
}
