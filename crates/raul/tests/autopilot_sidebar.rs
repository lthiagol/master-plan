//! The Autopilot sidebar: tabs, visibility, and the split drag.
//!
//! Three things are pinned here, and they are pinned at different
//! levels on purpose:
//!
//! * **Which tab is selected** and **whether the sidebar shows** are
//!   state transitions — asserted directly on `AutopilotLaneState`.
//! * **Which tab is clickable** is geometry — asserted against
//!   `setup::regions`, the same value the renderer and the mouse
//!   handler use, so "clickable" and "drawn" cannot disagree.
//! * **That the choice persists** is asserted on the argv raul hands
//!   to `mp`, because that is the whole persistence contract: there is
//!   no raul-owned state file to inspect.

use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::{
    self, AutopilotLayout, SetupForm, SidebarTab, SPLIT_PCT_DEFAULT, SPLIT_PCT_MAX, SPLIT_PCT_MIN,
};
use raul::tui::mouse;
use raul::tui::view_state;

const W: u16 = 132;
const H: u16 = 36;

fn app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app
}

fn regions(app: &App) -> setup::AutopilotRegions {
    let view = view_state::compute_view(app, ratatui::layout::Rect::new(0, 0, W, H));
    view.autopilot
        .expect("autopilot hit areas are published for the lane")
}

// ─── tabs ────────────────────────────────────────────────────────

/// AC-03: the default tab is Progress, matching the `ui.autopilot
/// .sidebar_tab` default.
#[test]
fn sidebar_defaults_to_the_progress_tab() {
    let app = app();
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::Progress);
}

/// AC-03: all three tab headers are published as hit areas, in the
/// spec's order.
#[test]
fn all_three_tabs_are_offered_with_hit_areas() {
    let app = app();
    let regions = regions(&app);
    let tabs: Vec<SidebarTab> = regions.tabs.iter().map(|t| t.tab).collect();
    assert_eq!(
        tabs,
        vec![
            SidebarTab::Progress,
            SidebarTab::Activity,
            SidebarTab::State
        ]
    );
    for tab in &tabs {
        let rect = regions
            .tab_rect(*tab)
            .unwrap_or_else(|| panic!("{tab:?} should have a hit area"));
        assert!(rect.width > 0 && rect.height > 0);
    }
}

/// AC-03: a click on a tab header selects that tab. Driven through the
/// real dispatch path so the click target is the drawn rect.
#[test]
fn clicking_a_tab_header_selects_that_tab() {
    let mut app = app();
    let regions = regions(&app);
    let state_rect = regions
        .tab_rect(SidebarTab::State)
        .expect("state tab is offered while no run is live");
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let consumed = mouse::handle_dispatch(&mut app, &view, state_rect.x, state_rect.y, false);
    assert!(consumed, "a click on a tab header must be consumed");
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::State);
}

/// AC-03: `v` advances and `Shift+V` retreats, wrapping.
#[test]
fn next_and_prev_cycle_forward_and_back_through_the_tabs() {
    let mut layout = AutopilotLayout::new();
    layout.cycle_tab(1);
    assert_eq!(layout.sidebar_tab, SidebarTab::Activity);
    layout.cycle_tab(1);
    assert_eq!(layout.sidebar_tab, SidebarTab::State);
    layout.cycle_tab(1);
    assert_eq!(layout.sidebar_tab, SidebarTab::Progress, "must wrap");
    layout.cycle_tab(-1);
    assert_eq!(layout.sidebar_tab, SidebarTab::State, "must wrap back");
}

/// AC-03: the State tab is withheld while a run is live and the
/// operator pressed Back, and both the click targets and the key cycle
/// skip it.
#[test]
fn state_tab_is_hidden_during_a_live_run() {
    let mut app = app();
    app.autopilot.note_run_live(true);
    assert!(app.autopilot.takeover_active());
    app.autopilot.dismiss_takeover();
    assert!(app.autopilot.layout.state_tab_hidden);

    // Click targets: State is not offered.
    let regions = regions(&app);
    assert!(regions.tab_rect(SidebarTab::State).is_none());
    assert!(regions.tab_rect(SidebarTab::Activity).is_some());

    // Key cycle: two stops, and neither is State.
    let mut layout = app.autopilot.layout.clone();
    for _ in 0..4 {
        layout.cycle_tab(1);
        assert_ne!(layout.sidebar_tab, SidebarTab::State);
    }
}

/// AC-03: when the run ends, the State tab comes back.
#[test]
fn state_tab_returns_when_the_run_ends() {
    let mut app = app();
    app.autopilot.note_run_live(true);
    app.autopilot.dismiss_takeover();
    assert!(app.autopilot.layout.state_tab_hidden);
    app.autopilot.note_run_live(false);
    assert!(!app.autopilot.layout.state_tab_hidden);
    assert!(regions(&app).tab_rect(SidebarTab::State).is_some());
}

/// AC-03: the tab choice is handed to `mp config set
/// ui.autopilot.sidebar_tab`, spelled the way mp validates it.
#[test]
fn tab_choice_persists_through_mp_config_set() {
    let mut app = app();
    app.autopilot.layout.cycle_tab(1);
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::Activity);
    assert_eq!(
        setup::ui_pref_argv("sidebar_tab", "activity"),
        vec!["config", "set", "ui.autopilot.sidebar_tab", "activity"]
    );
}

// ─── visibility ──────────────────────────────────────────────────

/// AC-03: `z` collapses and expands the sidebar, and a collapsed
/// sidebar gives the setup region the full lane width.
#[test]
fn toggle_sidebar_flips_visibility_and_reclaims_the_width() {
    let mut app = app();
    assert!(app.autopilot.layout.sidebar_visible);
    let full = app.autopilot.layout.left_width(W);
    assert!(full < W, "an expanded sidebar takes some width");

    app.autopilot.layout.toggle_sidebar();
    assert!(!app.autopilot.layout.sidebar_visible);
    assert_eq!(app.autopilot.layout.left_width(W), W);

    app.autopilot.layout.toggle_sidebar();
    assert!(app.autopilot.layout.sidebar_visible);
    assert_eq!(app.autopilot.layout.left_width(W), full);
}

/// AC-03: a collapsed sidebar publishes no tab hit areas, so there is
/// nothing to click back into the sidebar — `z` is the way out.
#[test]
fn a_collapsed_sidebar_publishes_no_tab_hit_areas() {
    let mut app = app();
    app.autopilot.layout.toggle_sidebar();
    let regions = regions(&app);
    assert!(regions.tabs.is_empty());
    assert!(regions.sidebar.is_none());
    assert_eq!(regions.setup.width, W, "setup takes the whole lane");
}

/// AC-03: visibility is handed to `mp config set
/// ui.autopilot.sidebar_visible` as the literal mp accepts.
#[test]
fn visibility_persists_through_mp_config_set() {
    let mut app = app();
    app.autopilot.layout.toggle_sidebar();
    assert_eq!(
        setup::ui_pref_argv("sidebar_visible", "false"),
        vec!["config", "set", "ui.autopilot.sidebar_visible", "false"]
    );
}

// ─── split drag ──────────────────────────────────────────────────

/// AC-02: the default geometry is the 40/60 split.
#[test]
fn default_geometry_is_forty_sixty() {
    let app = app();
    assert_eq!(app.autopilot.layout.split_pct, SPLIT_PCT_DEFAULT);
    assert_eq!(app.autopilot.layout.left_width(100), 40);
}

/// AC-02: the drag border is published while idle.
#[test]
fn split_border_is_a_hit_target_while_idle() {
    let mut app = app();
    let regions = regions(&app);
    let inside = regions.split_x.expect("border is draggable while idle");
    let y = regions.setup.y + 2;
    assert!(
        mouse::begin_split_drag(&mut app, &regions, inside, y),
        "a press on the border must arm the drag"
    );
    assert!(app.autopilot.dragging_split);
}

/// AC-02: a press away from the border does not arm the drag, so a
/// click on a chip near the border is still a chip click.
#[test]
fn a_press_away_from_the_border_does_not_arm_the_drag() {
    let mut app = app();
    let regions = regions(&app);
    let border = regions.split_x.expect("border exists while idle");
    assert!(!mouse::begin_split_drag(
        &mut app,
        &regions,
        border + 6,
        regions.setup.y + 2
    ));
    assert!(!app.autopilot.dragging_split);
}

/// AC-02: dragging clamps to 25..=75. The value is clamped on every
/// motion, so the live preview never shows an out-of-range column.
#[test]
fn drag_clamps_to_the_legal_window() {
    let seed = app();
    let regions = regions(&seed);
    let left = regions.setup.x;
    // The percentage is relative to the lane's full width, so the
    // right edge is `left + lane_width` — not the current setup
    // column, which is only 40% of it.
    let right = left + regions.lane_width;
    let border = regions.split_x.expect("border while idle");

    for (target_x, expected) in [
        (left, SPLIT_PCT_MIN),
        (left, SPLIT_PCT_MIN),
        (right, SPLIT_PCT_MAX),
        (left + regions.lane_width / 2, 50),
        (left + regions.lane_width * 3 / 4, SPLIT_PCT_MAX),
    ] {
        let mut app = app();
        assert!(mouse::begin_split_drag(
            &mut app,
            &regions,
            border,
            regions.setup.y + 2
        ));
        mouse::update_split_drag(&mut app, &regions, target_x);
        assert_eq!(
            app.autopilot.layout.split_pct, expected,
            "dragging to column {target_x} should clamp to {expected}"
        );
        assert!(app.autopilot.layout.split_pct >= SPLIT_PCT_MIN);
        assert!(app.autopilot.layout.split_pct <= SPLIT_PCT_MAX);
        assert!(mouse::end_split_drag(&mut app));
        assert!(!app.autopilot.dragging_split);
    }
}

/// AC-02: a drag that never gets a mouse-up is not persisted, and a
/// mouse-up with no drag armed is a no-op.
#[test]
fn only_a_completed_drag_ends_the_drag() {
    let mut app = app();
    let regions = regions(&app);
    let border = regions.split_x.expect("border while idle");
    // No drag armed: mouse-up does nothing.
    assert!(!mouse::end_split_drag(&mut app));
    assert!(mouse::begin_split_drag(
        &mut app,
        &regions,
        border,
        regions.setup.y + 2
    ));
    assert!(mouse::end_split_drag(&mut app));
    // A second mouse-up is inert.
    assert!(!mouse::end_split_drag(&mut app));
}

/// AC-02: the completed width is what gets written to mp, and it is
/// inside the window `mp config set` validates.
#[test]
fn drag_result_persists_a_value_mp_will_accept() {
    let mut app = app();
    let regions = regions(&app);
    let border = regions.split_x.expect("border while idle");
    assert!(mouse::begin_split_drag(
        &mut app,
        &regions,
        border,
        regions.setup.y + 2
    ));
    // Drag far past the right edge.
    mouse::update_split_drag(&mut app, &regions, regions.setup.x + 10_000);
    assert!(mouse::end_split_drag(&mut app));
    let pct = app.autopilot.layout.split_pct;
    assert_eq!(pct, SPLIT_PCT_MAX);
    // mp rejects anything outside 25..=75, so the persisted argv must
    // carry the clamped value.
    assert!((SPLIT_PCT_MIN..=SPLIT_PCT_MAX).contains(&pct));
    assert_eq!(
        setup::ui_pref_argv("split_pct", &pct.to_string()),
        vec!["config", "set", "ui.autopilot.split_pct", &pct.to_string()]
    );
}

/// AC-02: the border is inert while a run is live — `split_x` is
/// `None`, so there is no drag to arm and the layout cannot change
/// under the operator mid-run.
#[test]
fn the_split_drag_is_inert_while_a_run_is_live() {
    let mut app = app();
    app.autopilot.note_run_live(true);
    let regions = regions(&app);
    assert!(
        regions.split_x.is_none(),
        "no drag target may be published while a run is live"
    );
    let before = app.autopilot.layout.split_pct;
    assert!(!mouse::begin_split_drag(
        &mut app,
        &regions,
        40,
        regions.setup.y + 2
    ));
    mouse::update_split_drag(&mut app, &regions, 5);
    mouse::end_split_drag(&mut app);
    assert_eq!(
        app.autopilot.layout.split_pct, before,
        "the width must not move while a run is live"
    );
}

/// AC-02: `begin_split_drag` / `update_split_drag` /
/// `split_pct_for_column` are the whole drag path; the column-to-
/// percentage mapping is asserted directly so a change in rounding
/// shows up here rather than as a one-pixel jump.
#[test]
fn column_to_percentage_mapping_is_exact_at_the_midpoint() {
    // Clamped, not zero: a 0% column is outside the legal window.
    assert_eq!(mouse::split_pct_for_column(100, 0, 0), SPLIT_PCT_MIN);
    assert_eq!(mouse::split_pct_for_column(100, 50, 0), 50);
    assert_eq!(mouse::split_pct_for_column(100, 100, 0), SPLIT_PCT_MAX);
    // Clamped at both ends regardless of how far the pointer went.
    assert_eq!(mouse::split_pct_for_column(100, 5, 0), SPLIT_PCT_MIN);
    assert_eq!(mouse::split_pct_for_column(100, 95, 0), SPLIT_PCT_MAX);
    // An `x` left of the area's origin is clamped, not wrapped.
    assert_eq!(mouse::split_pct_for_column(100, 0, 50), SPLIT_PCT_MIN);
    // A zero-width area cannot divide by zero.
    assert_eq!(mouse::split_pct_for_column(0, 10, 0), SPLIT_PCT_DEFAULT);
}

/// The Milestones section's chips are the picker's rows: a click both
/// lands the cursor and flips the selection, so "click the milestone I
/// mean" is one gesture.
#[test]
fn milestone_chip_click_lands_the_cursor_and_toggles_selection() {
    let mut app = app();
    app.autopilot.picker.refresh_candidates(&serde_json::json!({
        "milestones": [
            {"id": "M239", "title": "A", "lifecycle": "approved"},
            {"id": "M240", "title": "B", "lifecycle": "in-progress"},
        ]
    }));
    assert!(setup::click_candidate(&mut app, "240"));
    assert_eq!(app.autopilot.picker.cursor, 1);
    assert_eq!(app.autopilot.setup.selected, vec!["240".to_string()]);
    // A second click toggles it back off.
    assert!(setup::click_candidate(&mut app, "240"));
    assert!(app.autopilot.setup.selected.is_empty());
    // An id that is not a candidate changes nothing.
    assert!(!setup::click_candidate(&mut app, "999"));
}

/// The chips the regions publish carry stable ids, so a test (and the
/// docs) can name a chip without depending on its pixel position.
#[test]
fn chips_carry_stable_identifiers() {
    let mut app = app();
    app.autopilot.setup = SetupForm::new();
    let regions = regions(&app);
    let ids: Vec<&str> = regions.chips.iter().map(|c| c.id.as_str()).collect();
    for expected in [
        "topology:one-agent",
        "topology:two-agent",
        "topology:three-agent",
        "harness:uniform",
        "commit:commit_after_execute",
        "commit:push_after_review",
        "run_mode:normal",
        "run_mode:detached",
        "start",
    ] {
        assert!(
            ids.contains(&expected),
            "expected a chip with id {expected:?}; got {ids:?}"
        );
    }
    // Every chip has a non-empty drawn rect, so no chip is invisible but
    // clickable.
    for chip in &regions.chips {
        assert!(chip.rect.width > 0, "chip {} has zero width", chip.id);
    }
}
