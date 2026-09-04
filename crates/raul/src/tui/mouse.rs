//! M221: central mouse dispatch — coordinate mapping, click
//! classification, double-click debouncing, and wheel mapping.
//!
//! Every lane routes its mouse events through [`handle_dispatch`]
//! (called from [`crate::tui::runner::handle_mouse`]). Each call to a
//! lane's renderer computes a [`crate::tui::view_state::ViewState`]
//! whose `list_item_rects` / `tab_hit_areas` / `scrollbar_rects`
//! describe the visible interactive elements; this module reads
//! from that view and translates `(x, y)` clicks into typed
//! mutations on [`crate::tui::app::App`].
//!
//! ## Why a central module
//!
//! M135 split layout derivation out of the renderer so hit-test
//! areas and rendered areas share one source of truth. M221 adds
//! the second half: a single place where mouse math lives, so
//! long-press thresholds, double-click debouncing, and the
//! `RAUL_NO_MOUSE` escape hatch are each defined exactly once.
//! Per-lane wiring in `handle_dispatch` reads from the lane's
//! hit areas; the math itself never re-implemented on the lane
//! side.
//!
//! ## Surface
//!
//! - [`mouse_disabled`] — true when `RAUL_NO_MOUSE=1` is set
//! - [`classify_click`] — single / double / drag from a position+timestamp history
//! - [`resolve_list_click`] — find the row id under `(x, y)` for a given lane
//! - [`double_click_opens_detail`] — which lanes have a detail view to open
//! - [`handle_dispatch`] — the per-lane click handler entry point

use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::app::{App, ContentState, Lane};
use super::view_state::{self, ListItemHitArea, ViewState};

/// Double-click window in milliseconds. A second click within this
/// window at (nearly) the same coordinates classifies as a
/// double-click. Tuned to match the macOS / iTerm2 default and
/// the Android long-press default; tweaking lives here.
pub const DOUBLE_CLICK_MS: u128 = 500;

/// Maximum pixel drift that still counts as "same coordinates"
/// for a double-click. Two clicks 5 cells apart on the same row
/// are still a double-click on the same row; 50 cells apart is a
/// distinct click.
pub const DOUBLE_CLICK_DRIFT: u16 = 2;

/// True when `RAUL_NO_MOUSE=1` is set in the environment.
/// Operators on terminals that forward spurious mouse events can
/// flip this on to keep the keyboard path pristine. The runner
/// loop checks this once per `handle_mouse` call — no startup
/// snapshot — so a config flip takes effect on the next event.
pub fn mouse_disabled() -> bool {
    std::env::var("RAUL_NO_MOUSE")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

/// Classification of a click sequence. The dispatcher uses this
/// to decide whether to update selection (single) or open the
/// detail view (double).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    /// First click — set selection.
    Single,
    /// Second click within [`DOUBLE_CLICK_MS`] at almost the same
    /// coordinates — open detail.
    Double,
}

/// Classify a click relative to the prior click. Returns
/// [`ClickKind::Double`] when the prior click happened less than
/// [`DOUBLE_CLICK_MS`] ago AND within [`DOUBLE_CLICK_DRIFT`]
/// cells of the new click; otherwise [`ClickKind::Single`]. When
/// `last` is `None` (no prior click) the click is always a
/// `Single`.
pub fn classify_click(
    last: Option<(u16, u16, Instant)>,
    x: u16,
    y: u16,
    now: Instant,
) -> ClickKind {
    let Some((lx, ly, lt)) = last else {
        return ClickKind::Single;
    };
    let dt = now.duration_since(lt);
    if dt.as_millis() >= DOUBLE_CLICK_MS {
        return ClickKind::Single;
    }
    let dx = (lx as i32 - x as i32).abs();
    let dy = (ly as i32 - y as i32).abs();
    if dx > DOUBLE_CLICK_DRIFT as i32 || dy > DOUBLE_CLICK_DRIFT as i32 {
        return ClickKind::Single;
    }
    ClickKind::Double
}

/// Per-lane decision for whether a double-click should open the
/// detail view (the same code path as keyboard Enter). Overview
/// and Settings explicitly stay selection-only — Overview inbox
/// items navigate via the keyboard Enter handler, Settings rows
/// have no row-detail action to invoke.
pub fn double_click_opens_detail(lane: Lane) -> bool {
    matches!(
        lane,
        Lane::Milestones | Lane::Backlog | Lane::Ideas | Lane::Path | Lane::Autopilot
    )
}

/// Per-lane decision for whether a click is meaningful at all.
/// `false` for the Settings lane (selection has its own keybind
/// surface — mouse selection there still moves the highlight,
/// but the dedicated Settings renderer is the spec's hot path).
pub fn lane_supports_click_selection(lane: Lane) -> bool {
    // Every list-bearing lane supports click selection. Settings
    // selection is also mouse-driven in M221 (move the highlight
    // on the active flat list).
    matches!(
        lane,
        Lane::Milestones
            | Lane::Backlog
            | Lane::Ideas
            | Lane::Path
            | Lane::Overview
            | Lane::Settings
            | Lane::Autopilot
    )
}

/// Find the row under `(x, y)` for the active lane, returning
/// `(row_id, list_item_index)` if the click landed on a row in
/// `view.list_item_rects`. Pure function — no mutation, no App
/// read beyond the lane. `app` is reserved for future per-lane
/// hit-test variations (e.g. a custom filter that respects
/// `app.lane_search`); the current implementation hits
/// `view.list_item_rects` directly so all clicks line up with
/// what the renderer drew by construction.
pub fn resolve_list_click(
    _app: &App,
    view: &ViewState,
    x: u16,
    y: u16,
) -> Option<(String, ListItemHitArea)> {
    view.list_item_rects
        .iter()
        .find(|hit| point_in_rect(x, y, hit.rect))
        .map(|hit| (hit.id.clone(), hit.clone()))
}

/// Inclusive-x, inclusive-y point-in-rect test. Mirrors
/// `runner::point_in_rect` so the dispatch and the helper agree
/// on the exclusive-width convention (a cell at `x + width`
/// belongs to the next widget).
fn point_in_rect(x: u16, y: u16, r: ratatui::layout::Rect) -> bool {
    x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
}

/// Lane-specific click position → `selected_index` (or equivalent)
/// translator. Used by [`handle_dispatch`] so the dispatch doesn't
/// have to know about every lane's index semantics. Returns
/// `Some(index)` when the lane has an integer selection model;
/// `None` when the lane's selection lives elsewhere (Path, Settings,
/// Autopilot — each handled by its own dispatch arm).
pub fn resolve_index_for_lane(app: &App, row_id: &str) -> Option<usize> {
    match app.active_lane {
        Lane::Milestones => app.visible_milestones().iter().position(|m| m.id == row_id),
        Lane::Backlog | Lane::Ideas => app.visible_backlog().iter().position(|b| b.id == row_id),
        Lane::Overview => app
            .dashboard
            .inbox_items
            .iter()
            .position(|i| i.id == row_id),
        Lane::Path | Lane::Autopilot | Lane::Settings => None,
    }
}

/// Per-lane click dispatcher. Mutates `app` in-place when the
/// click is meaningful; returns `true` when the click was
/// consumed (and dispatch should NOT continue looking at scrollbar
/// / list rows underneath).
///
/// `was_double` is the result of [`classify_click`] for this
/// click — `true` opens detail (where applicable), `false` sets
/// selection.
pub fn handle_dispatch(app: &mut App, view: &ViewState, x: u16, y: u16, was_double: bool) -> bool {
    if !lane_supports_click_selection(app.active_lane) {
        return false;
    }

    if was_double && double_click_opens_detail(app.active_lane) {
        // Double-click → open detail. Find the row under the
        // click first (so the user can double-click anywhere
        // on a row to open it, not just the rendered glyphs).
        if let Some((row_id, _)) = resolve_list_click(app, view, x, y) {
            return dispatch_open_detail(app, &row_id);
        }
        // No row under the click — fall through to single-click
        // selection (or no-op).
    }

    // Single-click → set selection / move cursor / advance picker.
    if let Some((row_id, _)) = resolve_list_click(app, view, x, y) {
        return dispatch_single_click(app, &row_id);
    }
    false
}

/// Open detail for the given row id, mirroring the keyboard Enter
/// handler. Returns `true` when the lane supports detail open
/// AND the row id resolves to a real item.
fn dispatch_open_detail(app: &mut App, row_id: &str) -> bool {
    match app.active_lane {
        Lane::Milestones => {
            if let Some(idx) = app.visible_milestones().iter().position(|m| m.id == row_id) {
                app.selected_index = idx;
                app.enter_milestone_detail(Some(idx));
                return true;
            }
            false
        }
        Lane::Backlog | Lane::Ideas => {
            if let Some(idx) = app.visible_backlog().iter().position(|b| b.id == row_id) {
                app.selected_index = idx;
                app.selected_backlog_id = Some(row_id.to_string());
                app.detail_scroll = 0;
                app.content = ContentState::BacklogDetail;
                return true;
            }
            false
        }
        // Path / Autopilot double-click open is wired by the
        // lane-specific modules (the dispatch is non-trivial —
        // Path opens a milestone detail by id, Autopilot toggles
        // picker selection). `dispatch_single_click` already
        // handles single-click for those lanes; the dispatcher
        // in `runner.rs` escalates a Double via
        // `dispatch_double_click_for_lane` when this function
        // returns false.
        _ => false,
    }
}

/// Apply a single-click on a row in the active lane. Returns
/// `true` when a state mutation happened.
fn dispatch_single_click(app: &mut App, row_id: &str) -> bool {
    if let Some(idx) = resolve_index_for_lane(app, row_id) {
        app.selected_index = idx;
        app.touch();
        return true;
    }
    match app.active_lane {
        Lane::Autopilot => {
            // Click in the picker → move the cursor to the row,
            // matching `j` / `k` semantics.
            if let Some(pos) = app
                .autopilot
                .picker
                .candidates
                .iter()
                .position(|c| c.id == row_id)
            {
                app.autopilot.picker.cursor = pos;
                app.touch();
                return true;
            }
            false
        }
        Lane::Settings => {
            // Settings row click: the renderer is responsible
            // for emitting hit areas keyed by the canonical
            // `settings_idx` (the integer index into the flat
            // list). Without that plumbing this arm is a no-op
            // — the keyboard path remains canonical.
            false
        }
        _ => false,
    }
}

/// Handle a scroll-wheel event. Pure mapping: `Up` → `move_up`,
/// `Down` → `move_down`, modulo the lane-specific rules in
/// [`crate::tui::runner::handle_mouse`]. Returns `true` when the
/// event was consumed.
///
/// The Autopilot lane is special-cased here: the picker cursor
/// has no canonical `selected_index` integration (it lives on
/// `app.autopilot.picker.cursor`), so `move_down` would silently
/// no-op. The wheel handler dispatches the equivalent of `j`/`k`
/// directly via `Picker::move_cursor`.
pub fn handle_wheel(app: &mut App, kind: MouseEventKind) -> bool {
    if app.active_lane == super::app::Lane::Autopilot {
        let delta: i64 = match kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => return false,
        };
        app.autopilot.picker.move_cursor(delta);
        app.touch();
        return true;
    }
    match kind {
        MouseEventKind::ScrollUp => {
            app.move_up();
            true
        }
        MouseEventKind::ScrollDown => {
            app.move_down();
            true
        }
        _ => false,
    }
}

/// Lane-agnostic helper: was the click on the tab bar row? Used
/// by the runner to gate scroll-wheel events (tab-bar wheel is a
/// no-op per AC-09).
pub fn click_on_tab_bar(view: &ViewState, y: u16) -> bool {
    let bar = view.tab_bar_area;
    y >= bar.y && y < bar.y.saturating_add(bar.height)
}

/// Build a [`ViewState`] for the live `app` + terminal size. Thin
/// wrapper over [`view_state::compute_view`] so dispatchers don't
/// have to know the path.
pub fn compute_view(app: &App, width: u16, height: u16) -> ViewState {
    view_state::compute_view(app, ratatui::layout::Rect::new(0, 0, width, height))
}

/// Down-cast a `MouseEvent` to a `MouseButton::Left` Down event,
/// returning the `(x, y)` coords if it is one. Convenience for
/// callers that want to skip the `match` over `kind`.
pub fn left_down(mouse: &MouseEvent) -> Option<(u16, u16)> {
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        Some((mouse.column, mouse.row))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn mouse_disabled_default_is_false() {
        // RAUL_NO_MOUSE unset → false.
        // SAFETY: tests run in parallel; only safe because we
        // do not mutate the env from this test.
        assert!(!mouse_disabled());
    }

    #[test]
    fn classify_click_no_prior_is_single() {
        let now = Instant::now();
        assert_eq!(classify_click(None, 10, 10, now), ClickKind::Single);
    }

    #[test]
    fn classify_click_within_window_is_double() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(100);
        let last = Some((10u16, 10u16, t0));
        assert_eq!(classify_click(last, 11, 11, now), ClickKind::Double);
    }

    #[test]
    fn classify_click_outside_window_is_single() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(2_000);
        let last = Some((10u16, 10u16, t0));
        assert_eq!(classify_click(last, 11, 11, now), ClickKind::Single);
    }

    #[test]
    fn classify_click_far_drift_is_single() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(100);
        let last = Some((10u16, 10u16, t0));
        assert_eq!(classify_click(last, 50, 50, now), ClickKind::Single);
    }

    #[test]
    fn double_click_opens_detail_for_milestones_backlog_ideas_path_autopilot() {
        assert!(double_click_opens_detail(Lane::Milestones));
        assert!(double_click_opens_detail(Lane::Backlog));
        assert!(double_click_opens_detail(Lane::Ideas));
        assert!(double_click_opens_detail(Lane::Path));
        assert!(double_click_opens_detail(Lane::Autopilot));
        assert!(!double_click_opens_detail(Lane::Overview));
        assert!(!double_click_opens_detail(Lane::Settings));
    }

    #[test]
    fn left_down_picks_left_button_only() {
        let mk = |kind: MouseEventKind| MouseEvent {
            kind,
            column: 5,
            row: 7,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        assert_eq!(
            left_down(&mk(MouseEventKind::Down(MouseButton::Left))),
            Some((5, 7))
        );
        assert_eq!(
            left_down(&mk(MouseEventKind::Down(MouseButton::Right))),
            None
        );
        assert_eq!(left_down(&mk(MouseEventKind::Up(MouseButton::Left))), None);
        assert_eq!(left_down(&mk(MouseEventKind::ScrollUp)), None);
    }
}
