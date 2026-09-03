//! M214 AC-01: `Lane::Autopilot` exists; `Lane::Watch` is gone.
//!
//! The lane list under both visible (toggle on) and hidden (toggle off)
//! states matches the documented behavior, including the `JumpLane`
//! digit dispatch and the footer hint.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::modes::normal;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// AC-01: `Lane::Autopilot` exists; `Lane::Watch` is gone (the
/// public enum surface is the new name).
#[test]
fn autopilot_variant_exists_and_watch_is_gone() {
    // The new variant compiles and has the canonical position in the
    // ordered list — immediately before Settings, pinned by the M179
    // design decision the M214 milestone preserves.
    let lanes = Lane::ordered();
    assert!(
        lanes.contains(&Lane::Autopilot),
        "Lane::Autopilot must exist in Lane::ordered()"
    );
    assert_eq!(lanes.len(), 7, "M184 still pins exactly 7 lanes");
    let autopilot_idx = lanes
        .iter()
        .position(|l| matches!(l, Lane::Autopilot))
        .expect("Autopilot must be present");
    let settings_idx = lanes
        .iter()
        .position(|l| matches!(l, Lane::Settings))
        .expect("Settings must be present");
    assert_eq!(
        autopilot_idx + 1,
        settings_idx,
        "Autopilot must be pinned immediately before Settings"
    );
}

/// AC-01: `Lane::Autopilot.label()` and `compact_label()` expose the
/// new user-visible name (`Autopilot` / `Ap`).
#[test]
fn autopilot_label_and_compact_label_use_new_names() {
    assert_eq!(Lane::Autopilot.label(), "Autopilot");
    assert_eq!(Lane::Autopilot.compact_label(), "Ap");
    assert_eq!(Lane::Autopilot.label(), raul::lanes::LANE_AUTOPILOT);
}

/// AC-01: when the operator's `ui.show_autopilot_tab = true`, the
/// Autopilot lane is in the visible list — same as the pre-M214
/// contract under `ui.show_watch_tab`.
#[test]
fn autopilot_visible_when_show_autopilot_tab_is_true() {
    let visible = Lane::ordered_visible(true);
    assert!(
        visible.contains(&Lane::Autopilot),
        "Autopilot must be visible when show_autopilot_tab=true"
    );
    assert_eq!(visible, Lane::ordered());
}

/// AC-01: when `ui.show_autopilot_tab = false` (the default), the
/// Autopilot lane is omitted from the visible list.
#[test]
fn autopilot_hidden_when_show_autopilot_tab_is_false() {
    let visible = Lane::ordered_visible(false);
    assert!(
        !visible.contains(&Lane::Autopilot),
        "Autopilot must be filtered out when show_autopilot_tab=false"
    );
    assert_eq!(visible.len(), Lane::ordered().len() - 1);
    let full_without_autopilot: Vec<Lane> = Lane::ordered()
        .into_iter()
        .filter(|l| !matches!(l, Lane::Autopilot))
        .collect();
    assert_eq!(visible, full_without_autopilot);
}

/// AC-01: `JumpLane` digit dispatch routes to the Autopilot lane when
/// the lane is visible (digit = Autopilot's index in the visible list
/// + 1). When Autopilot is hidden, the digit that previously routed
/// to Autopilot now routes to the lane that took its slot (Settings).
#[test]
fn jump_lane_digit_dispatch_routes_to_autopilot_when_visible() {
    // First: with Autopilot visible, the digit at Autopilot's slot
    // routes to Autopilot.
    let mut app = App::new();
    app.show_autopilot_tab = true;
    let visible = Lane::ordered_visible(true);
    let autopilot_idx = visible
        .iter()
        .position(|l| matches!(l, Lane::Autopilot))
        .expect("Autopilot must be visible");
    app.active_lane = Lane::Overview;
    let digit = AutopilotDigit::new(autopilot_idx + 1);
    let actions = normal::handle_key(digit.key(), &app);
    assert_eq!(
        actions,
        vec![Action::JumpLane(autopilot_idx)],
        "digit dispatch must emit JumpLane(autopilot_idx) when Autopilot is visible"
    );

    // With Autopilot hidden, the digit that previously routed to
    // Autopilot now routes to Settings (the lane that took Autopilot's
    // slot at index 5).
    app.show_autopilot_tab = false;
    let actions = normal::handle_key(digit.key(), &app);
    assert_eq!(
        actions,
        vec![Action::JumpLane(autopilot_idx)],
        "with Autopilot hidden, the same digit now routes to the lane \
         that took Autopilot's slot (Settings at index 5)"
    );

    // Digits past the visible range are no-ops in both modes.
    for mode in [true, false] {
        app.show_autopilot_tab = mode;
        let max_digit = Lane::ordered_visible(mode).len();
        if max_digit < 9 {
            let past = AutopilotDigit::new(max_digit + 1);
            let actions = normal::handle_key(past.key(), &app);
            assert!(
                actions.is_empty(),
                "digit {} must be a no-op when visible lane count is {max_digit}; got {actions:?}",
                max_digit + 1
            );
        }
    }
}

/// AC-01: footer hint for the Autopilot lane — the per-tab footer
/// row is intentionally empty for Autopilot (D-07 / M214 — the action
/// bar lives inside the row, not the footer).
#[test]
fn footer_per_tab_for_autopilot_is_empty_by_design() {
    let kb = Keybinds::default();
    let s = kb.footer_per_tab(
        Lane::Autopilot,
        raul::tui::app::ContentState::List,
        false,
        false,
    );
    assert!(
        s.is_empty(),
        "footer_per_tab(Autopilot, List) must be empty per D-07; got {s:?}"
    );
}

/// AC-01: there is no `Lane::Watch` variant anywhere in the production
/// source. Walks `crates/raul/src/` and asserts the symbol is gone
/// (parity with the M184 `Lane::Tweaks` source-walk pin).
#[test]
fn source_has_no_lane_watch_variant() {
    let root = std::path::Path::new(CARGO_MANIFEST_DIR).join("src");
    let mut hits = Vec::new();
    walk_for_lane_watch(&root, &mut hits);
    assert!(
        hits.is_empty(),
        "Lane::Watch must not appear under crates/raul/src/; found in {hits:?}"
    );
}

fn walk_for_lane_watch(dir: &std::path::Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            walk_for_lane_watch(&path, hits);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let body = std::fs::read_to_string(&path).unwrap();
            if body.contains("Lane::Watch") {
                hits.push(path.display().to_string());
            }
        }
    }
}

/// Helper: produce a `KeyEvent` whose code is the ASCII digit at
/// `position` (1-based). Used by the digit-dispatch test so the
/// caller doesn't have to construct `KeyEvent` literals.
struct AutopilotDigit {
    ch: char,
}

impl AutopilotDigit {
    fn new(position: usize) -> Self {
        assert!(
            position >= 1 && position <= 9,
            "digit position out of range"
        );
        let ch = char::from_digit(position as u32, 10).expect("valid digit");
        Self { ch }
    }

    fn key(&self) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(self.ch), KeyModifiers::empty())
    }
}
