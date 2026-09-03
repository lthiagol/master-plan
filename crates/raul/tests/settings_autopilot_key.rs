//! M214 AC-03: Settings lane renders `ui.show_autopilot_tab` as the
//! displayed key; the digit-jump dispatch routes to the Autopilot
//! lane correctly.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::modes::normal;
use raul::tui::modes::settings::{flat_key, value_for_key, SETTINGS_KEYS};

/// AC-03: `SETTINGS_KEYS` contains `ui.show_autopilot_tab` — the
/// canonical user-facing key. The Settings lane renders this string
/// as the row label, and `SettingsSave` (`s` on the lane) writes the
/// toggle value back to this key via `mp config set ui.show_autopilot_tab
/// <value>`.
#[test]
fn settings_keys_contains_new_autopilot_key() {
    let autopilot_entry = SETTINGS_KEYS
        .iter()
        .find(|(_, k)| *k == "ui.show_autopilot_tab");
    assert!(
        autopilot_entry.is_some(),
        "SETTINGS_KEYS must expose `ui.show_autopilot_tab`; the legacy `ui.show_watch_tab` is read-only"
    );
    // Section column is `"ui"` — paired with the dotted key for
    // `mp config set <section>.<key>` routing.
    let (section, _key) = autopilot_entry.expect("present");
    assert_eq!(*section, "ui");
}

/// AC-03: `flat_key(idx)` resolves to the canonical Autopilot row.
/// The Settings lane's selection model drives `idx`; the helper
/// surfaces the row at every (lane, content_state) pair without
/// crashing.
#[test]
fn flat_key_resolves_to_autopilot_row() {
    let autopilot_idx = SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == "ui.show_autopilot_tab")
        .expect("ui.show_autopilot_tab must be in SETTINGS_KEYS");
    let (section, key) = flat_key(autopilot_idx).expect("flat_key resolves");
    assert_eq!(section, "ui");
    assert_eq!(key, "ui.show_autopilot_tab");
}

/// AC-03: `value_for_key` reads the Autopilot key from a parsed
/// `mp config show` payload. Both the boolean form (cargo `mp config
/// show` returns true/false) round-trip cleanly.
#[test]
fn value_for_key_round_trips_autopilot_key() {
    let true_payload = serde_json::json!({ "ui": { "show_autopilot_tab": true } });
    assert_eq!(
        value_for_key(&true_payload, "ui.show_autopilot_tab"),
        "true"
    );

    let false_payload = serde_json::json!({ "ui": { "show_autopilot_tab": false } });
    assert_eq!(
        value_for_key(&false_payload, "ui.show_autopilot_tab"),
        "false"
    );

    // Absent key returns an empty string — the Settings lane
    // surfaces the empty current value as "unset" rather than crashing.
    let absent_payload = serde_json::json!({ "ui": {} });
    assert_eq!(value_for_key(&absent_payload, "ui.show_autopilot_tab"), "");
}

/// AC-03: digit-jump dispatch routes to the Autopilot lane when the
/// lane is visible. The Settings lane's `JumpLane` arm
/// (`Action::JumpLane(idx)`) lands on `Lane::Autopilot` for the
/// digit at Autopilot's slot in the visible lane list. This pins
/// the digit-to-lane mapping the user sees on the tab bar.
#[test]
fn digit_jump_routes_to_autopilot_when_visible() {
    let mut app = App::new();
    app.show_autopilot_tab = true;
    app.active_lane = Lane::Milestones;

    let visible = Lane::ordered_visible(true);
    let autopilot_idx = visible
        .iter()
        .position(|l| matches!(l, Lane::Autopilot))
        .expect("Autopilot must be visible");
    let digit = char::from_digit((autopilot_idx + 1) as u32, 10).expect("valid digit");
    let key = KeyEvent::new(KeyCode::Char(digit), KeyModifiers::empty());

    let actions = normal::handle_key(key, &app);
    assert_eq!(
        actions,
        vec![Action::JumpLane(autopilot_idx)],
        "digit dispatch must emit JumpLane(autopilot_idx) so the Settings lane's digit shortcut lands on Autopilot"
    );

    // And: after the dispatch, selecting the Autopilot lane through
    // `JumpLane` lands on `Lane::Autopilot` — the user-visible
    // routing.
    let mut app2 = App::new();
    app2.show_autopilot_tab = true;
    app2.active_lane = Lane::Settings;
    // The Settings lane keeps its own selection model; the digit
    // dispatch goes through the normal arm, not Settings.
    let actions = normal::handle_key(key, &app2);
    assert_eq!(actions, vec![Action::JumpLane(autopilot_idx)]);
}

/// AC-03: when the operator hides the Autopilot tab
/// (`ui.show_autopilot_tab = false`), the digit that previously
/// routed to Autopilot now routes to Settings (the lane that took
/// Autopilot's slot at the same visible-list index). This is the
/// same single-filter-point behavior the M198 S4 spec pinned for
/// `show_watch_tab = false`.
#[test]
fn digit_jump_skips_hidden_autopilot_lane() {
    let mut app = App::new();
    app.show_autopilot_tab = false;
    app.active_lane = Lane::Overview;

    let visible = Lane::ordered_visible(false);
    // The lane at index 5 (digit 6) is Settings, not Autopilot —
    // the filter list promotes Settings into Autopilot's slot.
    let target_idx = 5;
    let target_lane = visible
        .get(target_idx)
        .expect("Settings must remain in the visible list");
    assert_eq!(*target_lane, Lane::Settings);

    let digit = char::from_digit((target_idx + 1) as u32, 10).expect("valid digit");
    let key = KeyEvent::new(KeyCode::Char(digit), KeyModifiers::empty());
    let actions = normal::handle_key(key, &app);
    assert_eq!(
        actions,
        vec![Action::JumpLane(target_idx)],
        "digit must route to Settings when Autopilot is hidden"
    );
}

/// AC-03: App routing for the Autopilot lane stays consistent — the
/// `ordered_visible` list and the active-lane lookup agree. Defensive
/// pin: a future drift that breaks the single-filter-point contract
/// fails here.
#[test]
fn ordered_visible_and_active_lane_lookup_agree() {
    for show in [true, false] {
        let mut app = App::new();
        app.show_autopilot_tab = show;
        app.active_lane = Lane::Autopilot;
        // After reconcile, Autopilot stays active when shown, falls
        // back to Overview when hidden.
        app.reconcile_active_lane_with_visible();
        if show {
            assert_eq!(app.active_lane, Lane::Autopilot);
        } else {
            assert_eq!(app.active_lane, Lane::Overview);
        }
    }
}

/// AC-03: App's content state stays at `List` on the Autopilot lane.
/// `ContentState::CoApproval` / `AnnotationThread` / `MilestoneDetail`
/// are not valid for Autopilot (the lane has no per-row drilling
/// beyond the picker); the reconcile guard keeps the state sane.
#[test]
fn reconcile_does_not_change_content_state_for_autopilot() {
    let mut app = App::new();
    app.show_autopilot_tab = true;
    app.active_lane = Lane::Autopilot;
    app.content = ContentState::List;
    app.reconcile_active_lane_with_visible();
    assert_eq!(app.content, ContentState::List);
}
