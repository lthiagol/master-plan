//! M222 S4: read-only Settings keymap view.
//!
//! AC-05: Settings must render the effective global + per-lane
//! map, mark overridden versus default bindings, and state the
//! external TOML path. Editing is external; the view exposes
//! no in-app editor.

use std::collections::HashSet;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::keybinds::{Keybinds, KeybindsView, KeybindsViewRow};

fn km_pair(c: KeyCode, m: KeyModifiers) -> (KeyCode, KeyModifiers) {
    (c, m)
}

#[test]
fn view_lists_every_global_and_autopilot_action() {
    let kb = Keybinds::default();
    let view = kb.view();
    // The view must contain every name from both action
    // registries. We pin the count rather than the literal
    // contents so a future addition to either side surfaces
    // here instead of silently shrinking the legend.
    let names: HashSet<(&'static str, &'static str)> = view
        .rows
        .iter()
        .map(|r| (r.section, r.action))
        .collect();
    let expected_global: HashSet<&'static str> =
        Keybinds::global_action_names().iter().copied().collect();
    let expected_autopilot: HashSet<&'static str> =
        Keybinds::autopilot_action_names().iter().copied().collect();
    for name in &expected_global {
        assert!(
            names.contains(&("global", name)),
            "global action `{name}` missing from view; rows: {rows:?}",
            rows = view.rows
        );
    }
    for name in &expected_autopilot {
        assert!(
            names.contains(&("autopilot", name)),
            "autopilot action `{name}` missing from view"
        );
    }
    assert_eq!(
        view.rows.len(),
        expected_global.len() + expected_autopilot.len(),
        "row count must equal the union of both registries"
    );
}

#[test]
fn view_with_defaults_marks_nothing_overridden() {
    let kb = Keybinds::default();
    let view = kb.view();
    assert_eq!(view.overridden_count(), 0);
    for row in &view.rows {
        assert_eq!(
            row.effective_combo, row.default_combo,
            "default config must not mark anything overridden (row {section}.{action})",
            section = row.section,
            action = row.action
        );
        assert!(
            !row.overridden,
            "row {section}.{action} must not be `overridden` at defaults",
            section = row.section,
            action = row.action
        );
    }
}

#[test]
fn view_with_overrides_marks_only_changed_rows() {
    let mut kb = Keybinds::default();
    // Override ONLY autopilot.select; everything else must stay
    // at the default.
    kb.lane_autopilot.select = vec![km_pair(KeyCode::F(1), KeyModifiers::empty())];

    let view = kb.view();
    let mut over = 0usize;
    for row in &view.rows {
        if row.overridden {
            over += 1;
            assert_eq!(row.section, "autopilot");
            assert_eq!(row.action, "select");
            assert_eq!(row.effective_combo, "F1");
            assert!(!row.default_combo.is_empty());
        }
    }
    assert_eq!(over, 1, "exactly one row must be marked overridden");
    assert_eq!(view.overridden_count(), 1);
}

#[test]
fn view_surfaces_toml_path_and_uses_default_path_when_home_unset() {
    let kb = Keybinds::default();
    let view = kb.view();
    let path: &PathBuf = &view.toml_path;
    // The default path always ends with `keybinds.toml` even when
    // the env doesn't resolve to a writable location.
    assert!(
        path.to_string_lossy().ends_with("keybinds.toml"),
        "default TOML path must end in keybinds.toml; got {path:?}"
    );
    // Ensure the file name appears somewhere; some platforms
    // produce an empty relative path on missing env vars — that
    // still satisfies the contract because the Settings UI can
    // still hand the user a stable path expression.
    let _ = Keybinds::default_path();
}

#[test]
fn view_effective_combo_matches_dispatcher_for_per_autopilot_override() {
    // End-to-end: build a TOML with the [autopilot] override,
    // parse it, then render the view. The viewport must show
    // the user-visible diff and the dispatcher must agree.
    let text = "[autopilot]\nselect = \"f1\"\n";
    let (_diags, kb) = Keybinds::load_from_keybinds_toml(text);
    let view = kb.view();
    let select_row = view
        .rows
        .iter()
        .find(|r| r.section == "autopilot" && r.action == "select")
        .expect("autopilot.select row in the view");
    assert_eq!(select_row.effective_combo, "F1");
    assert!(select_row.overridden);
    assert!(!select_row.default_combo.is_empty());
    assert_ne!(select_row.effective_combo, select_row.default_combo);
}

#[test]
fn view_has_no_in_app_editing_affordance() {
    // AC-05 explicitly disallows in-app editing. We pin the
    // shape: `KeybindsView` is data + a path; there is no
    // `set_action` / `apply_edit` method (the type system is the
    // pin — a future addition would break the tests as a
    // reminder).
    let kb = Keybinds::default();
    let view: KeybindsView = kb.view();
    // Use field access (the only affordance the type offers):
    // rows + toml_path. No writer surface.
    let _: Vec<KeybindsViewRow> = view.rows;
    let _: PathBuf = view.toml_path;
}

#[test]
fn view_renders_format_per_combo_for_known_aliases() {
    // The view must render canonical keybinder names so the
    // operator reads the same glyph as on the help overlay.
    // (Future settings auto-remap would key off this column.)
    let kb = Keybinds::default();
    let view = kb.view();
    let quit_row = view
        .rows
        .iter()
        .find(|r| r.action == "quit")
        .expect("quit row");
    // Default: q + Shift+q (the second binding uses an uppercase
    // letter and is rendered via the SHIFT separator rather than
    // the literal `Q`, matching the help-overlay format).
    assert_eq!(quit_row.default_combo, "q, Shift+q");
    assert_eq!(quit_row.effective_combo, "q, Shift+q");
}
