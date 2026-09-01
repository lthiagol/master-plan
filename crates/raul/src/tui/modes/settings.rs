//! M169: Settings lane handler — project config flat list.
//!
//! Pure key → action mapping. Config load/save and dry-run validation live
//! in `apply_action` (`SettingsSave`, `Enter`, `Esc` on the lane).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::action::Action;
use crate::tui::app::{App, Lane};
use crate::tui::keybinds::any_matches;
use crate::tui::mode::SettingsFocus;

pub mod help;
pub mod schema;

/// Flat list of configurable keys. Each row is `(section, key)`.
pub const SETTINGS_KEYS: &[(&str, &str)] = &[
    // ui
    ("ui", "ui.color"),
    ("ui", "ui.icons"),
    ("ui", "ui.theme"),
    ("ui", "ui.hide_done"),
    // M198 WP1: when `false` (the default), raul's tab bar
    // filters the Watch lane out (see S3 / `compute_tab_bar_layout`).
    // The `mp` binary's `mp watch` command is independent of this
    // flag — only the human-facing TUI surface reacts. Operators
    // opt in via this row.
    ("ui", "ui.show_watch_tab"),
    // workflow
    ("workflow", "workflow.profile"),
    ("workflow", "workflow.plan.location"),
    ("workflow", "workflow.plan.in_repo"),
    ("workflow", "workflow.gates.strictness"),
    ("workflow", "workflow.steps.code_review"),
    // git
    ("git", "git.auto_commit"),
    ("git", "git.commit_on_milestone_complete"),
    ("git", "git.auto_push"),
    // next
    ("next", "next.prefer"),
    // agent
    ("agent", "agent.automation.commit_after_execute"),
    ("agent", "agent.automation.push_after_review"),
    ("agent", "agent.automation.branch_strategy"),
    ("agent", "agent.automation.auto_remediate"),
    // keybinds
    ("keybinds", "keybinds.quit"),
    ("keybinds", "keybinds.up"),
    ("keybinds", "keybinds.down"),
    ("keybinds", "keybinds.page_up"),
    ("keybinds", "keybinds.page_down"),
    ("keybinds", "keybinds.enter"),
    ("keybinds", "keybinds.escape"),
    ("keybinds", "keybinds.help"),
    ("keybinds", "keybinds.filter"),
    ("keybinds", "keybinds.hide_done"),
    ("keybinds", "keybinds.create_annotation"),
    ("keybinds", "keybinds.resolve"),
    ("keybinds", "keybinds.reopen"),
    ("keybinds", "keybinds.approve"),
    ("keybinds", "keybinds.review_menu"),
    ("keybinds", "keybinds.open_settings"),
    ("keybinds", "keybinds.previous_lane"),
    ("keybinds", "keybinds.next_lane"),
    ("keybinds", "keybinds.refresh"),
    ("keybinds", "keybinds.next_section"),
    ("keybinds", "keybinds.prev_section"),
    ("keybinds", "keybinds.next_item"),
    ("keybinds", "keybinds.prev_item"),
    ("keybinds", "keybinds.lifecycle_filter"),
    ("keybinds", "keybinds.grooming_preset"),
    ("keybinds", "keybinds.search"),
    ("keybinds", "keybinds.cycle_sort"),
];

/// `(section, key)` for the row at `idx`, or `None` if out of range.
pub fn flat_key(idx: usize) -> Option<(&'static str, &'static str)> {
    SETTINGS_KEYS.get(idx).copied()
}

pub fn handle_key(key: KeyEvent, app: &App) -> Vec<Action> {
    if app.active_lane != Lane::Settings {
        return Vec::new();
    }
    let Some(state) = app.settings.as_ref() else {
        return Vec::new();
    };
    let kb = &app.keybinds;

    // `h` (default hide_done) is a no-op on Settings so list-pane state
    // behind the lane is not toggled accidentally.
    if let KeyCode::Char('h') = key.code {
        if key.modifiers == KeyModifiers::empty() || key.modifiers == KeyModifiers::SHIFT {
            return Vec::new();
        }
    }

    // Save (`s`) on the Settings lane — not open_settings (Ctrl-O elsewhere).
    if let KeyCode::Char('s') = key.code {
        if key.modifiers == KeyModifiers::empty() || key.modifiers == KeyModifiers::SHIFT {
            return vec![Action::SettingsSave];
        }
    }

    // M201 S7: while NOT editing, Left / Right on a `choice` key cycles
    // through `allowed` in place. The schema lookup is best-effort: if
    // the schema isn't loaded, fall through to the editor path so the
    // user can still type into a free-form edit popup.
    if !matches!(state.focus, SettingsFocus::Editing) {
        if matches!(key.code, KeyCode::Right | KeyCode::Left) && key.modifiers.is_empty() {
            let key_name = crate::tui::modes::settings::flat_key(state.selected_idx)
                .map(|(_, k)| k.to_string());
            let ty = key_name
                .as_deref()
                .and_then(|k| state.schema.as_ref().and_then(|s| s.get(k)))
                .map(|e| e.ty.as_str());
            if matches!(ty, Some("choice")) {
                return vec![Action::SettingsCycleChoice {
                    forward: matches!(key.code, KeyCode::Right),
                }];
            }
        }

        // M201 S6: while NOT editing, Space on a `bool` key toggles
        // in place. No editor opens.
        if let KeyCode::Char(' ') = key.code {
            if key.modifiers.is_empty() {
                let key_name = crate::tui::modes::settings::flat_key(state.selected_idx)
                    .map(|(_, k)| k.to_string());
                let ty = key_name
                    .as_deref()
                    .and_then(|k| state.schema.as_ref().and_then(|s| s.get(k)))
                    .map(|e| e.ty.as_str());
                if matches!(ty, Some("bool")) {
                    return vec![Action::SettingsToggleBool];
                }
            }
        }
    }

    if matches!(state.focus, SettingsFocus::Editing) {
        if any_matches(&kb.escape, &key) {
            return vec![Action::Esc];
        }
        // Q-02: Tab / Shift+Tab commit the active edit instead of cycling lanes.
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) && key.modifiers.is_empty() {
            return vec![Action::Enter];
        }
        if any_matches(&kb.enter, &key) {
            return vec![Action::Enter];
        }
        if key.code == KeyCode::Backspace && key.modifiers.is_empty() {
            return vec![Action::PopInputChar];
        }
        if let KeyCode::Char(c) = key.code {
            if key.modifiers == KeyModifiers::empty() || key.modifiers == KeyModifiers::SHIFT {
                return vec![Action::PushInputChar(c)];
            }
        }
        return Vec::new();
    }

    // Flat list: Esc is a no-op (AC-02).
    if any_matches(&kb.escape, &key) {
        return Vec::new();
    }

    if any_matches(&kb.up, &key) {
        return vec![Action::Up];
    }
    if any_matches(&kb.down, &key) {
        return vec![Action::Down];
    }
    if any_matches(&kb.page_up, &key) {
        return vec![Action::PageUp];
    }
    if any_matches(&kb.page_down, &key) {
        return vec![Action::PageDown];
    }
    if any_matches(&kb.enter, &key) {
        return vec![Action::Enter];
    }
    Vec::new()
}

/// Read a dotted config key from the `config` object of `mp config show`.
pub fn value_for_key(config: &serde_json::Value, key: &str) -> String {
    let mut cur = config;
    for part in key.split('.') {
        cur = match cur.get(part) {
            Some(v) => v,
            None => return String::new(),
        };
    }
    match cur {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Display label for unset keybinds.* entries.
pub fn keybind_default_label(action: &str) -> Option<String> {
    use crate::tui::key_combo::format_key_combo;
    use crate::tui::keybinds::Keybinds;
    let defaults = Keybinds::default();
    let combos = match action {
        "quit" => &defaults.quit,
        "up" => &defaults.up,
        "down" => &defaults.down,
        "page_up" => &defaults.page_up,
        "page_down" => &defaults.page_down,
        "enter" => &defaults.enter,
        "escape" => &defaults.escape,
        "help" => &defaults.help,
        "filter" => &defaults.filter,
        "hide_done" => &defaults.hide_done,
        "create_annotation" => &defaults.create_annotation,
        "resolve" => &defaults.resolve,
        "reopen" => &defaults.reopen,
        "approve" => &defaults.approve,
        "review_menu" => &defaults.review_menu,
        "open_settings" => &defaults.open_settings,
        "previous_lane" => &defaults.previous_lane,
        "next_lane" => &defaults.next_lane,
        "refresh" => &defaults.refresh,
        "next_section" => &defaults.next_section,
        "prev_section" => &defaults.prev_section,
        "next_item" => &defaults.next_item,
        "prev_item" => &defaults.prev_item,
        "lifecycle_filter" => &defaults.lifecycle_filter,
        "grooming_preset" => &defaults.grooming_preset,
        "search" => &defaults.search,
        "cycle_sort" => &defaults.cycle_sort,
        _ => return None,
    };
    if combos.is_empty() {
        Some("(default: unbound)".to_string())
    } else {
        let s = combos
            .iter()
            .map(|c| format_key_combo(*c))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("(default: {s})"))
    }
}
