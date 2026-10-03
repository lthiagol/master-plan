//! The `?` help overlay's Autopilot section.
//!
//! The section is *generated* from `AutopilotLaneKeybinds` rather than
//! hand-listed, which is the property worth pinning: a rebind in
//! `keybinds.toml` must show up in the overlay with no second edit. If
//! someone replaces the generated rows with a hardcoded list, the
//! rebind test below fails.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::key_combo::parse_key_combo;
use raul::tui::keybinds::{autopilot_help_entries, Keybinds};
use raul::tui::view_state;

const W: u16 = 132;

// The overlay lists the per-lane group, the global group, and the
// Autopilot section. It has to be tall enough to hold all three, or
// the assertion "the last entry is rendered" is really asserting "the
// terminal is big enough".
const H: u16 = 60;

fn autopilot_app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app
}

fn help_text(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).expect("terminal");
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            raul::tui::render::render(frame, app, &view);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..H)
        .map(|y| {
            (0..W)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── the section exists ──────────────────────────────────────────

/// AC-10: the overlay has an Autopilot section while the Autopilot
/// lane is active.
#[test]
fn help_overlay_autopilot_section_is_present() {
    let mut app = autopilot_app();
    app.active_mode = raul::tui::mode::Mode::Help;
    let text = help_text(&app);
    assert!(
        text.contains("Autopilot"),
        "the overlay should name the Autopilot section; got:\n{text}"
    );
}

/// AC-10: the section lists the three new split-view bindings, with
/// their default keys.
#[test]
fn help_overlay_autopilot_lists_the_sidebar_bindings() {
    let mut app = autopilot_app();
    app.active_mode = raul::tui::mode::Mode::Help;
    let text = help_text(&app);
    for label in ["next sidebar tab", "previous sidebar tab", "toggle sidebar"] {
        assert!(
            text.contains(label),
            "the overlay should list {label:?}; got:\n{text}"
        );
    }
    // The default keys are on the same line as their labels.
    assert!(text.contains("v next sidebar tab"), "got:\n{text}");
    assert!(text.contains("V previous sidebar tab"), "got:\n{text}");
    assert!(text.contains("z toggle sidebar"), "got:\n{text}");
}

/// The section is generated, so a rebind shows up.
#[test]
fn help_overlay_autopilot_reflects_a_rebind() {
    let mut app = autopilot_app();
    app.active_mode = raul::tui::mode::Mode::Help;
    // Rebind next_sidebar_tab from `v` to `F3` through the config file
    // surface, exactly as an operator would.
    let (diags, kb) = Keybinds::load_from_keybinds_toml("[autopilot]\nnext_sidebar_tab = \"F3\"\n");
    assert!(diags.is_empty(), "rebind should parse cleanly: {diags:?}");
    app.keybinds = kb;

    let text = help_text(&app);
    // The key grammar normalizes to lower case, so F3 round-trips as
    // `f3` — what matters is that it is no longer the `v` default.
    assert!(
        text.contains("f3 next sidebar tab"),
        "the rebind should appear in the overlay; got:\n{text}"
    );
    assert!(
        !text.contains("v next sidebar tab"),
        "the old default should be gone; got:\n{text}"
    );
}

/// The same rebind changes the generated entries, which is the
/// property the render test above depends on.
#[test]
fn a_rebind_changes_the_generated_entries() {
    let (diags, kb) = Keybinds::load_from_keybinds_toml("[autopilot]\nnext_sidebar_tab = \"F3\"\n");
    assert!(diags.is_empty(), "{diags:?}");
    let entries = autopilot_help_entries(&kb);
    let next = entries
        .iter()
        .find(|(label, _)| label == "next sidebar tab")
        .expect("the entry exists");
    assert_eq!(next.1, "f3");
}

/// Every lane action has an entry, so the section cannot silently lose
/// one. The count is pinned to the struct's field count.
#[test]
fn every_autopilot_lane_action_has_a_help_entry() {
    let kb = Keybinds::default();
    let entries = autopilot_help_entries(&kb);
    // 16 pre-existing lane actions + the 3 split-view bindings.
    assert_eq!(entries.len(), 19);
    // Every entry has a non-empty key rendering — an empty default
    // would render as a bare label with no way to press it.
    for (label, keys) in &entries {
        assert!(!keys.is_empty(), "{label:?} has no key rendering");
    }
    // The labels are unique, so a copy-paste cannot shadow an entry.
    let mut labels: Vec<&str> = entries.iter().map(|(l, _)| l.as_str()).collect();
    labels.sort_unstable();
    let before = labels.len();
    labels.dedup();
    assert_eq!(labels.len(), before, "help labels must be unique");
}

/// Every rendered key string is a combo the canonical grammar can
/// parse, so the overlay never shows a key that could not be typed.
#[test]
fn every_help_overlay_autopilot_key_parses() {
    for (label, keys) in autopilot_help_entries(&Keybinds::default()) {
        for chord in keys.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            assert!(
                parse_key_combo(chord).is_some(),
                "{label:?} shows un-parseable key {chord:?}"
            );
        }
    }
}

/// The section is contextual: it appears on the Autopilot lane and not
/// on others, so the overlay does not bury the global list.
#[test]
fn the_autopilot_section_is_contextual() {
    let entries = autopilot_help_entries(&Keybinds::default());
    assert!(!entries.is_empty(), "the generator always has rows");
    // On a non-Autopilot lane the section is not rendered.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.active_mode = raul::tui::mode::Mode::Help;
    let text = help_text(&app);
    assert!(
        !text.contains("next sidebar tab"),
        "the Autopilot section should not appear on the Milestones lane; got:\n{text}"
    );
}
