//! M135: ViewState — pre-computed hit areas + frame layout for the
//! current frame.
//!
//! [`compute_view`] walks `App` once and populates a [`ViewState`] with
//! the screen rect of every interactive element AND the frame layout
//! rects (header, tab bar, content, footer, list hit areas,
//! dashboard chunks, co-approval chunks, overlay rects). Both
//! `render()` and the mouse handler read from the same struct, so
//! layout math happens exactly once per frame.
//!
//! Adding a new clickable region, scrollable list, or modal is a
//! one-place change in `compute_view` — the renderer and the input
//! dispatch pick it up automatically. Before M135, every interactive
//! element needed its hit area recomputed in two places (`render()` in
//! `render.rs` and the mouse handler in `runner.rs`); the off-by-one
//! drift between the two was the bug class M135 closes.
//!
//! ## Field conventions
//!
//! - Hit-area `id`s are the **stable source-data identifier** of the
//!   element: `Lane` for tabs, the milestone / backlog / inbox id for
//!   list items. The id is what a click looks up in the
//!   same data source the renderer walked, so render and hit-test
//!   agree by construction.
//! - All `rect`s are absolute (relative to the full frame, not the
//!   content area).
//! - All vectors are in **render order** (top-to-bottom,
//!   left-to-right) so test assertions can compare them against
//!   `terminal.backend().buffer()` snapshots row by row.

use ratatui::layout::{Constraint, Layout, Rect};

use super::app::{App, ContentState, Lane};
use super::dashboard;
// M137: SCROLLBAR_GUTTER is the constant other modules use to reserve a
// 1-column-wide gutter on every scrollable region. Re-exported here so
// `view_state::SCROLLBAR_GUTTER` is the canonical path tests / future
// call sites use.
use super::mode::Mode;
pub use super::render::scrollbar::{scrollbar_rect, SCROLLBAR_GUTTER};

// =============================================================================
// M135 (S4) — tab-bar layout machinery moved here from `render.rs`
// =============================================================================
//
// Pre-M135, the tab-bar layout was `pub` from `render.rs` so the
// mouse handler in `runner.rs` could recompute hit areas on the same
// `TabBarLayout` the renderer used. That was the cross-module leak
// the spec calls out — layout internals exposed so two files could
// agree by construction. M135 closes the leak: the layout machinery
// now lives in `view_state.rs`, and the mouse handler reads
// `view.tab_hit_areas` (a pre-computed list of `(Lane, Rect)` pairs)
// instead of recomputing the layout on its own.
//
// `compute_view` is the only consumer; the visibility stays `pub`
// (not `pub(super)`) so the existing `tui_mouse.rs` / `tui_tab_bar.rs`
// tests that import these symbols through `raul::tui::view_state`
// keep working. The cross-module leak to `runner.rs` is gone — the
// mouse handler no longer references any of these names.

/// Width in cells of the `idx`-th tab as rendered — `" {label} "` for
/// the first tab (idx 0), `" │ {label} "` for subsequent tabs.
const INDICATOR_WIDTH: usize = 3;

/// Width in cells of the `idx`-th tab as rendered. First tab is
/// `" {label} "`, subsequent tabs are `" │ {label} "`. Used by
/// `compute_tab_bar_layout` to size the visible set; the renderer
/// formats labels with the same prefix in `render_tab_bar`.
///
/// `pub(super)` (not `pub`) so the cross-module leak to `runner.rs`
/// is closed (M135 S4 done_when: "no longer pub"). Tests that
/// exercise the layout machinery live in `#[cfg(test)] mod tests`
/// inside this module, where `pub(super)` is sufficient.
pub(super) fn tab_text_width(lane: &Lane, idx: usize, compact: bool) -> usize {
    let label = if compact {
        lane.compact_label()
    } else {
        lane.label()
    };
    // M137+: the first lane renders ` {label} ` (3-char surround);
    // subsequent lanes drop the leading space so the pipe overlaps
    // the predecessor's trailing space, producing a bar that reads
    // `name │ name │ name` with consistent spacing. This must match
    // `render_tab_bar` exactly — a width mismatch lets the active
    // tab's highlight "drift" past the pipe.
    let s = if idx == 0 {
        format!(" {label} ")
    } else {
        format!("│ {label} ")
    };
    s.chars().count()
}

/// Layout result for the tab bar at a given width with a given active
/// lane. Pure data; both rendering and hit-testing consume it.
/// Constructed by `compute_tab_bar_layout`.
///
/// `pub(super)` per M135 S4 — the layout machinery is no longer
/// `pub` from any cross-module surface. Tests live in `#[cfg(test)]
/// mod tests` inside this module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabBarLayout {
    /// Indices into `Lane::ordered()` that fit on screen. Always
    /// contains the active lane's index. In wide mode this is
    /// `0..total`; in overflow mode it is the active lane plus as many
    /// neighbors as fit.
    pub visible: Vec<usize>,
    /// True when at least one lane to the left of `visible[0]` is
    /// hidden. In wide mode always false.
    pub has_left_indicator: bool,
    /// True when at least one lane to the right of `*visible.last()` is
    /// hidden.
    pub has_right_indicator: bool,
    /// True when `visible[0] > 1` — i.e., a " … " ellipsis between
    /// the left indicator and the first visible tab is emitted.
    pub has_left_ellipsis: bool,
    /// True when `*visible.last() < total - 2`.
    pub has_right_ellipsis: bool,
    /// True when the bar overflowed `area.width`.
    pub overflowed: bool,
    /// True when the bar is in compact-label mode (width < 60).
    pub compact: bool,
}

/// Pick which lanes are visible for the given terminal width and
/// active lane. Mirror of M91 S6's overflow logic — active lane is
/// always visible; outward fan adds neighbors until the budget is
/// spent. See `render::render_tab_bar` (pre-M135 home) for the
/// matching emission order.
///
/// `pub(super)` per M135 S4 — the layout machinery is no longer
/// `pub` from any cross-module surface. Tests live in `#[cfg(test)]
/// mod tests` inside this module.
pub(super) fn compute_tab_bar_layout(
    width: u16,
    compact: bool,
    active: &Lane,
    lanes: &[Lane],
) -> TabBarLayout {
    // M198 WP2 / AC-04 / M214: the caller passes the filtered lane
    // list (`Lane::ordered_visible(app.show_autopilot_tab)`) so the
    // layout and the hit-test areas see the same set. The function
    // itself is pure; tests pass `&Lane::ordered()` for the
    // full-list case.
    let total = lanes.len();

    let total_width: usize = lanes
        .iter()
        .enumerate()
        .map(|(i, lane)| tab_text_width(lane, i, compact))
        .sum::<usize>()
        + 1;
    let overflowed = (total_width as u32) > u32::from(width);

    if !overflowed {
        return TabBarLayout {
            visible: (0..total).collect(),
            has_left_indicator: false,
            has_right_indicator: false,
            has_left_ellipsis: false,
            has_right_ellipsis: false,
            overflowed: false,
            compact,
        };
    }

    // Ultra-narrow terminals: indicator reservation leaves no tab
    // budget. M115 AC-01 + M124 widen the guard so it fires when
    // either the indicator reservation OR the active tab text would
    // overflow the area. See the M115 / M124 follow-ups for the full
    // history.
    let active_idx = lanes.iter().position(|l| l == active).unwrap_or(0);
    let active_label_w = tab_text_width(&lanes[active_idx], active_idx, compact);
    if (width as usize) < 2 * INDICATOR_WIDTH || (active_label_w + 1) > width as usize {
        if (active_label_w + 1) as u32 > u32::from(width) {
            return TabBarLayout {
                visible: Vec::new(),
                has_left_indicator: false,
                has_right_indicator: false,
                has_left_ellipsis: false,
                has_right_ellipsis: false,
                overflowed: false,
                compact,
            };
        }
        return TabBarLayout {
            visible: vec![active_idx],
            has_left_indicator: false,
            has_right_indicator: false,
            has_left_ellipsis: false,
            has_right_ellipsis: false,
            overflowed: true,
            compact,
        };
    }

    // Overflow path. Reserve:
    //   * 3 cols on left and right for "◂" / "▸" indicators.
    //   * 4 cols per ellipsis segment when needed.
    let mut budget = width as usize;
    let left_ind = INDICATOR_WIDTH;
    let right_ind = INDICATOR_WIDTH;
    let ellipsis = " \u{2026} "; // 4 cols
    budget = budget.saturating_sub(left_ind + right_ind);

    let active_idx = lanes.iter().position(|l| l == active).unwrap_or(0);
    let mut visible: Vec<usize> = Vec::new();
    visible.push(active_idx);
    let mut used_w = tab_text_width(&lanes[active_idx], active_idx, compact);
    let mut left = active_idx.checked_sub(1);
    let mut right = active_idx + 1;
    loop {
        let mut picked = false;
        if let Some(l) = left {
            let w = tab_text_width(&lanes[l], l, compact);
            let ellipsis_cost = if !visible.is_empty() && l + 1 != visible[0] {
                ellipsis.chars().count()
            } else {
                0
            };
            if used_w + w + ellipsis_cost <= budget {
                visible.insert(0, l);
                used_w += w + ellipsis_cost;
                left = l.checked_sub(1);
                picked = true;
            }
        }
        if right < total {
            let w = tab_text_width(&lanes[right], right, compact);
            let ellipsis_cost =
                if !visible.is_empty() && right != visible.last().copied().unwrap() + 1 {
                    ellipsis.chars().count()
                } else {
                    0
                };
            if used_w + w + ellipsis_cost <= budget {
                visible.push(right);
                used_w += w + ellipsis_cost;
                right += 1;
                picked = true;
            }
        }
        if !picked {
            break;
        }
    }

    let has_left_indicator = visible[0] > 0;
    let has_right_indicator = *visible.last().unwrap() < total - 1;
    let has_left_ellipsis = has_left_indicator && visible[0] > 1;
    let has_right_ellipsis = has_right_indicator && *visible.last().unwrap() < total - 2;

    // M124: if the total emitted width (indicators + ellipses + visible
    // tabs) exceeds the area, drop the indicators/ellipses so the bar
    // doesn't fragment past the area boundary.
    let total_emitted_w: usize = {
        let indicator_w =
            if has_left_indicator { 3 } else { 0 } + if has_right_indicator { 3 } else { 0 };
        let ellipsis_w =
            if has_left_ellipsis { 4 } else { 0 } + if has_right_ellipsis { 4 } else { 0 };
        let tabs_w: usize = visible
            .iter()
            .map(|&i| tab_text_width(&lanes[i], i, compact))
            .sum();
        indicator_w + ellipsis_w + tabs_w
    };
    let (has_left_indicator, has_right_indicator, has_left_ellipsis, has_right_ellipsis) =
        if total_emitted_w > width as usize {
            (false, false, false, false)
        } else {
            (
                has_left_indicator,
                has_right_indicator,
                has_left_ellipsis,
                has_right_ellipsis,
            )
        };

    TabBarLayout {
        visible,
        has_left_indicator,
        has_right_indicator,
        has_left_ellipsis,
        has_right_ellipsis,
        overflowed: true,
        compact,
    }
}

/// Inclusive-exclusive x range each visible tab occupies on the bar
/// row. In wide mode: column 0 is leading space, then tabs in
/// `Lane::ordered()` order. In overflow mode: column 0..3 is the left
/// indicator, optional 4-col ellipsis at 3..7, then visible tabs.
///
/// `pub(super)` per M135 S4 — the layout machinery is no longer
/// `pub` from any cross-module surface. Tests live in `#[cfg(test)]
/// mod tests` inside this module.
pub(super) fn visible_tab_x_ranges(
    layout: &TabBarLayout,
    lanes: &[Lane],
) -> Vec<(usize, u16, u16)> {
    // M198 WP2 / AC-04: same shape as `compute_tab_bar_layout` —
    // caller passes the filtered lane list so the hit areas
    // agree with the renderer's visible set.
    let ellipsis_w = " \u{2026} ".chars().count();
    let indicator_w = INDICATOR_WIDTH;

    let mut out = Vec::with_capacity(layout.visible.len());
    let mut cursor: usize =
        if layout.overflowed && (layout.has_left_indicator || layout.has_right_indicator) {
            let mut c = indicator_w;
            if layout.has_left_ellipsis {
                c += ellipsis_w;
            }
            c
        } else {
            1
        };

    let push_tab = |lane_idx: usize, cursor: &mut usize, out: &mut Vec<(usize, u16, u16)>| {
        let lane = &lanes[lane_idx];
        let w = tab_text_width(lane, lane_idx, layout.compact);
        let start_x = *cursor as u16;
        let end_x = (*cursor + w) as u16;
        out.push((lane_idx, start_x, end_x));
        *cursor += w;
    };

    if layout.overflowed {
        let n = layout.visible.len();
        if n >= 1 {
            push_tab(layout.visible[0], &mut cursor, &mut out);
        }
        if n >= 3 {
            for &lane_idx in &layout.visible[1..n - 1] {
                push_tab(lane_idx, &mut cursor, &mut out);
            }
        }
        if layout.has_right_ellipsis {
            cursor += ellipsis_w;
        }
        if n >= 2 {
            push_tab(layout.visible[n - 1], &mut cursor, &mut out);
        }
    } else {
        for &lane_idx in &layout.visible {
            push_tab(lane_idx, &mut cursor, &mut out);
        }
    }
    out
}

// =============================================================================
// M135 — hit-area types
// =============================================================================

/// Hit area for a single tab in the tab bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabHitArea {
    pub id: Lane,
    pub rect: Rect,
}

/// Hit area for a single item in the active list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItemHitArea {
    pub id: String,
    pub rect: Rect,
}

// =============================================================================
// M137 — scrollbar hit areas
// =============================================================================

/// Stable identifier for a scrollable region on screen. Used both as a
/// key for [`ScrollbarHitArea`] and as the lookup for clicks on the
/// scrollbar track (so the dispatcher can map a track-click to the
/// right `(selected_index, region)` to scroll).
///
/// The enum is exhaustive over the scrollable regions: lists
/// (milestones/backlog/overview inbox), detail views, annotation
/// thread, and path view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollableId {
    /// The Milestones list (Table view).
    MilestonesList,
    /// The Backlog list (List view).
    BacklogList,
    /// The Overview lane's inbox block (multi-row per item).
    OverviewInbox,
    /// Milestone detail scrollback.
    MilestoneDetail,
    /// Annotation thread scrollback.
    AnnotationThread,
    /// Backlog detail scrollback.
    BacklogDetail,
    /// Path tree scrollback (`app.path_scroll` / `app.path_max_scroll`).
    PathLane,
}

/// Hit area + scroll state for a single scrollable region. The
/// `rect` is the gutter rect (1 column wide on the right edge of the
/// region); `scroll` is the current top-row index;
/// `total` is the total number of scrollable units; `visible` is how
/// many units fit in the viewport. `scrollbar_rects` carries one entry
/// per scrollable region on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollbarHitArea {
    pub id: ScrollableId,
    pub rect: Rect,
    pub scroll: usize,
    pub total: usize,
    /// Content units visible in the viewport. When `0`, render falls
    /// back to `rect.height` (row-cell lists).
    pub visible: usize,
}

/// M181: Overview-lane chunk geometry. The active lane's content
/// area splits into six rectangles (Health; Statistics; Work queues;
/// Lifecycle grid; Suggested path; the lower Inbox/activity split).
///
/// The lower split's `lower_inbox` is the only block whose rows
/// receive hit areas — the Activity block, the Statistics/Work
/// queues/Lifecycle/path blocks, and the Health strip are
/// display-only per AC-02/04/06.
///
/// `inbox_side_by_side` records the breakpoint decision so the
/// renderer can pick the right layout (horizontal vs stacked) and
/// tests can pin the breakpoint behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardChunks {
    pub health: Rect,
    pub statistics: Rect,
    pub work_queues: Rect,
    pub lifecycle: Rect,
    pub suggested_path: Rect,
    /// Lower-left / upper inbox block. On wide terminals the
    /// activity block sits to the right; on narrow terminals it
    /// sits below.
    pub lower_inbox: Rect,
    /// Lower-right / lower activity block. `None` when stacked
    /// below the inbox (narrow terminal).
    pub lower_activity: Option<Rect>,
    pub inbox_side_by_side: bool,
    /// M183 F-04: density flag shared with `render_dashboard` so
    /// chunk heights and content lines agree. True when the
    /// content area is below [`DASHBOARD_COMPACT_CONTENT_HEIGHT`].
    pub compact: bool,
}

/// Content-area height below which Overview uses compact top-block
/// heights. Measured on `content_area` (not the full frame) so the
/// 2-line footer (M183) does not desync layout from the renderer.
pub const DASHBOARD_COMPACT_CONTENT_HEIGHT: u16 = 32;

// =============================================================================
// M135 — ViewState
// =============================================================================

/// M135: pre-computed hit areas + frame layout for the current frame.
///
/// `compute_view` populates one of these from `App` and the current
/// `area`. `render()` and the mouse handler both take it as input, so
/// the render pass and the input dispatch read the **same** rect for
/// every interactive element.
///
/// ## Hit areas
///
/// The hit-area vectors are what the mouse handler walks; they are
/// also useful for assertions in tests that compare what was rendered
/// against what is clickable.
///
/// ## Frame layout
///
/// `header_area`, `tab_bar_area`, `content_area`, `footer_area` are
/// the four top-level rects `render()` always needs. The
/// content-area-specific fields (`dashboard_chunks`,
/// `co_approval_chunks`) are populated only when the active content
/// is a list / dashboard / co-approval view. `tab_layout` is the
/// pre-computed `TabBarLayout` that `render_tab_bar` reads instead of
/// recomputing.
///
/// ## Adding a new hit area
///
/// 1. Add a new field here (e.g. `pub breadcrumb_hit_areas: Vec<…>`).
/// 2. Populate it in `compute_view` — single source of truth.
/// 3. Render reads it; the mouse handler reads it. Both paths agree
///    because both read the same field.
///
/// ## Scrollbar
///
/// `scrollbar_rects` is reserved for the scrollbar widget landing in
/// M137 — always empty until then. Computing scrollbar hit areas
/// before the widget exists would lock the layout to a pre-M137
/// shape, so the slot is intentionally unused.
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    // === Hit areas (consumed by mouse handler + render coordination) ===
    /// One entry per visible tab in the tab bar, in render order.
    pub tab_hit_areas: Vec<TabHitArea>,
    /// One entry per visible row in the active list, in render order.
    /// Empty when the active content is not a list.
    pub list_item_rects: Vec<ListItemHitArea>,
    /// Bounds of the active overlay (help, input prompt, review
    /// menu). `None` when no overlay is on screen.
    pub overlay_rect: Option<Rect>,
    /// M137: one entry per scrollable region. Each entry carries the
    /// region identifier, the gutter rect (absolute coords), and
    /// `(scroll, total)` so the renderer and the hit-test both pull
    /// from the same struct.
    pub scrollbar_rects: Vec<ScrollbarHitArea>,

    // === Frame layout (consumed by render) ===
    /// Top-level splits: header (row 0), main (row 1+), footer (last
    /// row). Always populated.
    pub header_area: Rect,
    pub main_area: Rect,
    pub footer_area: Rect,
    /// Within main: tab bar (row 0), content (row 1+). Always
    /// populated.
    pub tab_bar_area: Rect,
    pub content_area: Rect,

    /// Pre-computed `TabBarLayout`. `render_tab_bar` reads this
    /// instead of calling `compute_tab_bar_layout` itself, so the
    /// hit areas in `tab_hit_areas` and the rendered spans agree by
    /// construction.
    pub tab_layout: TabBarLayout,

    /// M181: Dashboard chunk geometry for the Overview lane. Holds
    /// the six-block responsive layout — `None` when the active
    /// content is not a list in the Overview lane.
    pub dashboard_chunks: Option<DashboardChunks>,

    /// Co-approval screen chunks: `[header_block, body_block,
    /// actions_block, status_block]`. `None` when not in co-approval.
    pub co_approval_chunks: Option<[Rect; 4]>,
}

// =============================================================================
// M135 — compute_view
// =============================================================================

/// M135: build a [`ViewState`] for the current `app` state and the
/// full-frame `area`.
///
/// Pure read of `App` — never mutates. The mouse handler can call
/// this on demand (or, in a future hot path, the render pass can pass
/// the already-computed `ViewState` down to dispatch). For M135, both
/// call sites call `compute_view` themselves; the redundancy is cheap
/// (one Layout split per frame) and keeps the integration surface
/// small.
///
/// The function mirrors the layout decisions in `render::render` so
/// every rect matches what the renderer actually drew. Diverging here
/// M217: the per-tab footer line's full text — the
/// per-(lane, content_state) keybind glyphs from
/// `Keybinds::footer_per_tab` plus the trailing lane indicators
/// (M205's `sort:`, M204's `<N> filters`, M217's `poll:`).
///
/// Single source of truth for two callers that must agree:
/// [`compute_view`] sizes `footer_area` from whether this string
/// is empty, and `render::chrome::footer_for` paints it. Before
/// M217 the height was derived from `footer_per_tab` alone, so a
/// lane whose only per-tab content was an indicator (exactly the
/// Autopilot lane's situation) reserved no row and the indicator
/// was silently dropped.
pub fn footer_per_tab_text(app: &App) -> String {
    let settings_staged = app.settings.as_ref().is_some_and(|s| s.has_staged_edits());
    let mut text =
        app.keybinds
            .footer_per_tab(app.active_lane, app.content, app.open_only, settings_staged);
    // M205 AC-06: the per-tab footer carries a trailing
    // `sort: <key> ▼` indicator on the three sort-bearing lanes
    // (Milestones / Backlog / Ideas), showing the active sort
    // key. The arrow matches the column-header arrow glyph used
    // in `header_cell`, so the operator sees the same visual
    // affordance on the column header and the footer.
    //
    // M204 / AC-09: the footer also surfaces the active
    // filter count as `<N> filters` when at least one filter
    // chip is active. Both indicators sit on the per-tab line
    // (right of the lane-key affordances) so the operator
    // sees sort + filter state in one glance. Hidden when
    // no filters and the default sort.
    if matches!(
        app.active_lane,
        Lane::Milestones | Lane::Backlog | Lane::Ideas
    ) && app.content == ContentState::List
    {
        let key = app.lane_sort_key(app.active_lane);
        let indicator = format!("sort: {} ▼", key.label());
        if text.is_empty() {
            text = format!(" {indicator} ");
        } else {
            // Append after the existing per-tab text — separator
            // matches the existing `·` between affordances.
            text.push_str(&format!("  ·  {indicator}"));
        }
        // M204 / AC-09: filter count indicator. The count is
        // the total number of (dim, value) chips across the
        // active lane (not the number of dimensions — a
        // multi-select dim with two values counts as 2).
        let filter_count = app
            .lane_filters
            .get(&app.active_lane)
            .map(|d| d.values().map(|s| s.len()).sum::<usize>())
            .unwrap_or(0);
        if filter_count > 0 {
            let f = format!("{filter_count} filters");
            if text.is_empty() {
                text = format!(" {f} ");
            } else {
                text.push_str(&format!("  ·  {f}"));
            }
        }
    }
    // M217 / AC-03: the Autopilot lane carries its auto-refresh
    // state on the per-tab footer line, so a *paused poll* is
    // never misread as a *stalled drive*. The label also names
    // the cadence and which link of the resolution chain supplied
    // it (`poll: 9s (session)`), which is how AC-04's precedence
    // becomes visible to the operator rather than only to a test.
    if app.active_lane == Lane::Autopilot {
        let indicator = app.autopilot_poller.footer_label();
        if text.is_empty() {
            text = format!(" {indicator} ");
        } else {
            text.push_str(&format!("  ·  {indicator}"));
        }
    }
    text
}

/// is the bug M135 closes — keep the two in sync.
pub fn compute_view(app: &App, area: Rect) -> ViewState {
    let mut view = ViewState::default();

    // Help overlay is exclusive: render() returns early when
    // `app.active_mode == Mode::Help`, so the tab bar, content, and
    // footer are all hidden behind the help screen. The only
    // interactive element on screen is the help overlay itself.
    // M186 F-01: bumped to 80% so the new Search/Cycle-sort rows fit
    // alongside Detail actions on a 40-row terminal.
    if matches!(app.active_mode, Mode::Help) {
        view.overlay_rect = Some(centered_rect(60, 80, area));
        return view;
    }

    // Top-level layout: row 0 = header, row 1 = main, last rows = footer.
    // M199: footer height is conditional on the active
    // (lane, content_state) — Path and Watch have empty per-tab
    // strings so the footer is 1 row (globals only); every other
    // lane reserves 2 rows (globals on h-2, per-tab on h-1).
    // `compute_view` is the single source of truth for
    // `footer_area.height`; `render_footer` reads it back without
    // re-deriving the count.
    // M217: size the footer from the *composed* per-tab text
    // (glyphs + indicators), not from `footer_per_tab` alone —
    // otherwise an indicator-only lane reserves no row.
    let per_tab_text = footer_per_tab_text(app);
    let footer_height = if per_tab_text.is_empty() { 1u16 } else { 2u16 };
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(footer_height),
    ])
    .split(area);
    let header_area = outer[0];
    let main_area = outer[1];
    let footer_area = outer[2];

    // Within main: row 0 = tab bar, row 1.. = content. Mirrors
    // render()'s `bar_split`.
    let bar_split = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(main_area);
    let tab_bar_area = bar_split[0];
    let content_area = bar_split[1];

    // Tab layout. Reuses the shared `compute_tab_bar_layout` so the
    // hit areas agree with the renderer's `render_tab_bar` by
    // construction (M105 S1 / B-39 / M124 follow-up).
    let compact = tab_bar_area.width < 60;
    // M198 WP2 / AC-04 / M214: pass the filtered lane list
    // (Autopilot omitted when `ui.show_autopilot_tab` is
    // `false`) to both `compute_tab_bar_layout` and
    // `visible_tab_x_ranges` so the layout, the hit-test areas,
    // and the prev/next navigation all see the same set. Single
    // filter point.
    let lanes = Lane::ordered_visible(app.show_autopilot_tab);
    let tab_layout = compute_tab_bar_layout(tab_bar_area.width, compact, &app.active_lane, &lanes);
    for (lane_idx, start_x, end_x) in visible_tab_x_ranges(&tab_layout, &lanes) {
        if let Some(lane) = lanes.get(lane_idx).cloned() {
            view.tab_hit_areas.push(TabHitArea {
                id: lane,
                rect: Rect {
                    x: tab_bar_area.x.saturating_add(start_x),
                    y: tab_bar_area.y,
                    width: (end_x - start_x),
                    height: tab_bar_area.height,
                },
            });
        }
    }

    // List / Co-approval layout — only when the active
    // content is a list (or co-approval). Detail / thread screens
    // render a scrollable paragraph, not a list of selectable items.
    if app.content == ContentState::List {
        match app.active_lane {
            Lane::Milestones => compute_milestone_list_rects(&mut view, app, content_area),
            Lane::Backlog => compute_backlog_list_rects(&mut view, app, content_area),
            Lane::Overview => compute_overview_list_rects(&mut view, app, content_area),
            Lane::Path => {
                // External-review F-04: Path lane reserves per-column
                // gutters (AC-01) even though path items are not
                // individually selectable like list rows.
                compute_path_scrollbar_rects(&mut view, app, content_area);
            }
            Lane::Ideas => {
                // M184: Ideas is a filtered backlog view (ID-*); same
                // list-with-scrollbar shape as Backlog.
                compute_backlog_list_rects(&mut view, app, content_area);
            }
            Lane::Settings => {
                // Settings content is the modal overlay; no list rects.
            }
            Lane::Autopilot => {
                // M221: hit areas for the Autopilot picker. The
                // renderer (`render::watch::render_picker`) takes
                // the LEFT 40% of the content area; each candidate
                // occupies one row inside the bordered block. We
                // mirror that geometry here so a click on a row
                // resolves to that candidate's id and the picker
                // cursor moves (single-click) or toggles selection
                // (double-click via `Action::AutopilotToggleSelect`
                // dispatched by the mouse handler).
                compute_autopilot_picker_rects(&mut view, app, content_area);
            }
        }
    } else if app.content == ContentState::CoApproval {
        compute_co_approval_chunks(&mut view, area);
    } else if app.content == ContentState::MilestoneDetail {
        // M137: detail scrollbar for milestone detail.
        push_detail_scrollbar(&mut view, app, content_area, ScrollableId::MilestoneDetail);
    } else if app.content == ContentState::AnnotationThread {
        compute_annotation_list_rects(&mut view, app, content_area);
        push_detail_scrollbar(&mut view, app, content_area, ScrollableId::AnnotationThread);
    } else if app.content == ContentState::BacklogDetail {
        // M137: scrollbar for backlog detail.
        push_detail_scrollbar(&mut view, app, content_area, ScrollableId::BacklogDetail);
    }

    // Overlay rect. Help is handled above (early return). Input
    // prompt and review menu render on top of whatever is below;
    // record whichever is active, preferring the input prompt
    // (innermost overlay wins) so a click in the review menu area
    // when the input is also open would be ambiguous — but the app
    // never opens both at once, so this branch is one-or-none in
    // practice.
    if app.is_input_active() {
        view.overlay_rect = Some(centered_rect(50, 30, area));
    } else if matches!(app.active_mode, Mode::ReviewMenu(_)) {
        view.overlay_rect = Some(centered_rect(40, 40, area));
    } else if matches!(app.active_mode, Mode::LifecycleFilter(_)) {
        // Tall enough for 10 lifecycles + chrome; clamps via centered_rect.
        view.overlay_rect = Some(centered_rect(50, 70, area));
    } else if matches!(app.active_mode, Mode::SearchInput(_)) {
        view.overlay_rect = Some(centered_rect(60, 15, area));
    } else if app.sort_rebind_open() {
        // Sort-rebind menu — small modal sized to the key list.
        view.overlay_rect = Some(centered_rect(30, 40, area));
    }

    view.header_area = header_area;
    view.main_area = main_area;
    view.footer_area = footer_area;
    view.tab_bar_area = tab_bar_area;
    view.content_area = content_area;
    view.tab_layout = tab_layout;
    view
}

fn compute_annotation_list_rects(view: &mut ViewState, app: &App, area: Rect) {
    let visible = app.visible_annotations();
    if visible.is_empty() || area.width < 2 || area.height < 2 {
        return;
    }
    let data_height = area.height.saturating_sub(2) as usize;
    let offset = app
        .selected_annotation_index
        .saturating_add(1)
        .saturating_sub(data_height);
    for (row, annotation) in visible.iter().skip(offset).take(data_height).enumerate() {
        view.list_item_rects.push(ListItemHitArea {
            id: annotation.id.clone(),
            rect: Rect {
                x: area.x.saturating_add(1),
                y: area.y.saturating_add(1).saturating_add(row as u16),
                width: area.width.saturating_sub(2 + SCROLLBAR_GUTTER),
                height: 1,
            },
        });
    }
}

/// M135 + M137: hit areas + scrollbar for the Milestones lane (Table).
///
/// M137: reserves `SCROLLBAR_GUTTER` (1 column) on the right edge of
/// the inner panel. The list item widths account for the gutter so a
/// click on a list row never collides with the scrollbar rail.
fn compute_milestone_list_rects(view: &mut ViewState, app: &App, area: Rect) {
    let visible = app.visible_milestones();
    let track = scrollbar_rect(area, SCROLLBAR_GUTTER);

    // External-review F-05: always reserve the gutter even when the
    // list is empty so empty↔non-empty does not reflow by 1 column.
    if visible.is_empty() {
        if track.width > 0 {
            view.scrollbar_rects.push(ScrollbarHitArea {
                id: ScrollableId::MilestonesList,
                rect: track,
                scroll: 0,
                total: 0,
                visible: 0,
            });
        }
        return;
    }

    let inner_height = area.height.saturating_sub(2);
    let data_height = inner_height.saturating_sub(1);
    if data_height == 0 {
        if track.width > 0 {
            view.scrollbar_rects.push(ScrollbarHitArea {
                id: ScrollableId::MilestonesList,
                rect: track,
                scroll: 0,
                total: visible.len(),
                visible: 0,
            });
        }
        return;
    }

    let selected = app.selected_index;
    // Smooth-scroll: start pulling rows up when the cursor is within
    // `SCROLL_LOOKAHEAD` rows of the bottom edge. The previous M137
    // formula jumped exactly at the bottom edge (`selected - data_h + 1`)
    // which the user reported as a stuttery "screen snaps up by N rows
    // when I'm at the very last one". The new formula keeps the cursor
    // 5 rows away from the bottom (or sooner, on tiny lists), giving
    // the smooth-scroll effect.
    let offset = compute_list_scroll(selected, data_height as usize, visible.len());

    let data_y_start = area.y.saturating_add(2);
    let inner_x = area.x.saturating_add(1);
    // M137: subtract `SCROLLBAR_GUTTER` so list items don't paint
    // under the scrollbar rail.
    let inner_width = area
        .width
        .saturating_sub(2)
        .saturating_sub(SCROLLBAR_GUTTER);

    for (i, m) in visible.iter().enumerate() {
        if i < offset {
            continue;
        }
        let visible_idx = i - offset;
        if visible_idx >= data_height as usize {
            break;
        }
        view.list_item_rects.push(ListItemHitArea {
            id: m.id.clone(),
            rect: Rect {
                x: inner_x,
                y: data_y_start.saturating_add(visible_idx as u16),
                width: inner_width,
                height: 1,
            },
        });
    }

    // M137: scrollbar hit area for the milestones list. The gutter
    // hugs the panel's right edge; `total` is the full list length
    // (post `hide_done` filter) and `scroll` is the top-row index of
    // the visible window.
    if track.width > 0 {
        view.scrollbar_rects.push(ScrollbarHitArea {
            id: ScrollableId::MilestonesList,
            rect: track,
            scroll: offset,
            total: visible.len(),
            visible: data_height as usize,
        });
    }
}
/// M135 + M137: hit areas + scrollbar for the Backlog lane (Table).
/// Same header-aware geometry as milestones so clicks land on data rows.
///
/// M203: each logical backlog row now spans 2 visual lines (title on
/// line 1, preview on line 2). Hit areas are 2 cells tall; the
/// scroll offset is computed in *logical* rows so the table-selected
/// index in the renderer stays aligned.
fn compute_backlog_list_rects(view: &mut ViewState, app: &App, area: Rect) {
    let track = scrollbar_rect(area, SCROLLBAR_GUTTER);

    // hide_done filters terminal statuses; hit areas must use the same set.
    let visible = app.visible_backlog();
    if visible.is_empty() {
        if track.width > 0 {
            view.scrollbar_rects.push(ScrollbarHitArea {
                id: ScrollableId::BacklogList,
                rect: track,
                scroll: 0,
                total: 0,
                visible: 0,
            });
        }
        return;
    }

    let inner_height = area.height.saturating_sub(2);
    let data_height_visual = inner_height.saturating_sub(1); // table header row
    if data_height_visual == 0 {
        if track.width > 0 {
            view.scrollbar_rects.push(ScrollbarHitArea {
                id: ScrollableId::BacklogList,
                rect: track,
                scroll: 0,
                total: visible.len(),
                visible: 0,
            });
        }
        return;
    }

    // Logical rows that fit in the data window (each row is 2 visual lines).
    const BACKLOG_ROW_VISUAL_HEIGHT: u16 = 2;
    let data_rows = (data_height_visual / BACKLOG_ROW_VISUAL_HEIGHT) as usize;
    let data_height_rows = data_rows.max(1);

    let offset = compute_list_scroll(app.selected_index, data_height_rows, visible.len());
    let data_y_start = area.y.saturating_add(2); // border + header
    let inner_x = area.x.saturating_add(1);
    let inner_width = area
        .width
        .saturating_sub(2)
        .saturating_sub(SCROLLBAR_GUTTER);

    for (i, b) in visible.iter().enumerate() {
        if i < offset {
            continue;
        }
        let visible_idx = i - offset;
        if visible_idx >= data_rows {
            break;
        }
        view.list_item_rects.push(ListItemHitArea {
            id: b.id.clone(),
            rect: Rect {
                x: inner_x,
                y: data_y_start
                    .saturating_add((visible_idx as u16).saturating_mul(BACKLOG_ROW_VISUAL_HEIGHT)),
                width: inner_width,
                height: BACKLOG_ROW_VISUAL_HEIGHT,
            },
        });
    }

    if track.width > 0 {
        view.scrollbar_rects.push(ScrollbarHitArea {
            id: ScrollableId::BacklogList,
            rect: track,
            scroll: offset,
            total: visible.len(),
            visible: data_rows,
        });
    }
}

/// How far from the bottom the cursor stays before the pane starts
/// scrolling, avoiding a jump when selection reaches the last row.
pub const SCROLL_LOOKAHEAD: usize = 5;

/// Helper: compute the scroll offset (top-row index) for a list so
/// the selected row stays visible.
///
/// Leaves `SCROLL_LOOKAHEAD` rows below the cursor when possible, so
/// scrolling begins before selection reaches the bottom edge. Upward
/// movement is symmetric and returns the offset to zero near the top.
///
/// Edge cases:
///
/// * `inner_height == 0` or `inner_height >= total + lookahead` → 0
///   (the list fits within the pane plus lookahead, no scroll needed).
/// * `selected < inner_height` and offset would be 0 → keep 0.
fn compute_list_scroll(selected: usize, inner_height: usize, total: usize) -> usize {
    if inner_height == 0 || total <= inner_height {
        return 0;
    }
    // The latest row index that should NOT yet be auto-scrolled.
    // When `selected >= trailing_threshold`, scrolling kicks in so
    // the cursor sits at row `inner_height - 1 - SCROLL_LOOKAHEAD`
    // (or later) at all times.
    let trailing_threshold = inner_height.saturating_sub(SCROLL_LOOKAHEAD + 1);
    if selected <= trailing_threshold {
        return 0;
    }
    let max_offset = total.saturating_sub(inner_height);
    (selected - trailing_threshold).min(max_offset)
}

/// Row height for an Overview inbox item (id+label / reason / action).
/// Rendering and scroll calculations must share this value.
const INBOX_ROW_HEIGHT: usize = 3;

/// M181 S4: hit areas + scrollbar for the redesigned Overview lane
/// (dashboard). The chunk geometry now follows the spec hierarchy —
/// Health; Statistics | Work queues; Lifecycle; Suggested path; the
/// lower Inbox/Activity split. The lower Inbox is the only block
/// that receives hit areas; activity, statistics, work queues,
/// lifecycle, and the path are display-only.
///
/// Responsive split (AC-01, AC-09): the lower Inbox/Activity pair is
/// side-by-side when the content area is wide enough to give both
/// blocks at least `LOWER_PANEL_MIN_WIDTH` columns of usable text;
/// otherwise the activity block stacks below the inbox.
///
/// Block heights (total rows including borders):
/// - Health: 4 content lines (Validation, Blockers, Execution +
///   Planning, Watch) + 2 borders = 6
/// - Statistics: 1 milestone row + blank + Steps label + 5 step
///   rows = 8 content lines + 2 borders = 10
/// - Lifecycle: 1 header + 3 chunk rows (3 buckets per row) = 4
///   content lines + 2 borders = 6
/// - Path: 1 Next header + up to 5 items = 6 content lines + 2
///   borders = 7
///
/// Total top-stack minimum: 6 + 10 + 6 + 7 = 29 rows of content +
/// header/tab/footer (4 rows: header + tab + 2-line footer) = 33
/// rows minimum for the full hierarchy. Terminals below that height
/// collapse the lower panel to zero rows — the user sees the top
/// blocks only, with no panic (the dashboard_chunks struct still
/// holds zero-sized rects and `render_dashboard` skips the lower
/// panel via early-returns).
const LOWER_PANEL_MIN_WIDTH: u16 = 32;
const HEALTH_HEIGHT: u16 = 6;
const STATS_HEIGHT: u16 = 10;
const LIFECYCLE_HEIGHT: u16 = 6;
const PATH_HEIGHT: u16 = 7;
const LOWER_PANEL_MIN_HEIGHT: u16 = 6;

fn compute_overview_list_rects(view: &mut ViewState, app: &App, area: Rect) {
    // M137-2: split the dashboard into a 1-column-wide gutter on the
    // right + a "content area minus gutter" to the left. The chunks
    // fill the left content area only — their right border stops one
    // column short of the gutter, so the block doesn't paint over
    // the scrollbar rail. The scrollbar lives next to the inbox
    // (the only interactive lower panel) per the S4 design.
    let content_no_gutter = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(SCROLLBAR_GUTTER),
        height: area.height,
    };

    // Top-of-screen chunked blocks (Health / Stats / Queues /
    // Lifecycle / Path). Heights are content-driven — full mode
    // (wide terminals) uses the rich layout; compact mode (30-row
    // terminals) compresses the per-section height so the lower
    // Inbox + Activity split still fits. The renderer reads
    // `DashboardChunks.compact` (same predicate) so density matches
    // the allocated rects (M183 F-04).
    let compact = area.height < DASHBOARD_COMPACT_CONTENT_HEIGHT;
    let (health_h, stats_h, lifecycle_h, path_h) = if compact {
        // Compact heights: 2/2/3/4 content lines = 4/4/5/6 total.
        (4u16, 4u16, 5u16, 6u16)
    } else {
        (HEALTH_HEIGHT, STATS_HEIGHT, LIFECYCLE_HEIGHT, PATH_HEIGHT)
    };
    let top_chunks = Layout::vertical([
        Constraint::Length(health_h),
        Constraint::Length(stats_h),
        Constraint::Length(lifecycle_h),
        Constraint::Length(path_h),
    ])
    .split(content_no_gutter);

    // The Statistics | Work queues row sits inside the second slot:
    // an inner horizontal split shares that block into two panels.
    let stats_row = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top_chunks[1]);

    // M183 F-02: anchor lower panels at the *actual* path chunk
    // bottom (`path_h`), not the full-mode `PATH_HEIGHT` constant.
    // Using PATH_HEIGHT in compact mode pushed inbox/activity onto
    // `footer_area` and left box-drawing on the globals/flash row.
    let path_rect = top_chunks[3];
    let lower_y = path_rect.y.saturating_add(path_rect.height);
    let content_bottom = content_no_gutter.y.saturating_add(content_no_gutter.height);
    let lower_min_height = content_bottom.saturating_sub(lower_y);

    // Decide between side-by-side and stacked for the lower split.
    // The inbox is the only block that emits hit areas / scrollbars
    // — when stacked, we move the gutter + scrollbar to the inbox
    // (top half); the activity block sits below without one.
    let half_width = content_no_gutter.width / 2;
    let side_by_side = half_width >= LOWER_PANEL_MIN_WIDTH;

    let (inbox_block, activity_block) = if side_by_side {
        let split = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(Rect {
                x: content_no_gutter.x,
                y: lower_y,
                width: content_no_gutter.width,
                height: lower_min_height,
            });
        (split[0], Some(split[1]))
    } else {
        // Stacked: inbox on top, activity below. Reserve at least
        // LOWER_PANEL_MIN_HEIGHT rows for the inbox so it stays
        // readable; activity takes the rest.
        let inbox_h = LOWER_PANEL_MIN_HEIGHT.max(lower_min_height / 2);
        let act_h = lower_min_height.saturating_sub(inbox_h);
        let inbox = Rect {
            x: content_no_gutter.x,
            y: lower_y,
            width: content_no_gutter.width,
            height: inbox_h.min(lower_min_height),
        };
        let activity = if act_h >= LOWER_PANEL_MIN_HEIGHT {
            Some(Rect {
                x: content_no_gutter.x,
                y: inbox.y.saturating_add(inbox.height),
                width: content_no_gutter.width,
                height: act_h,
            })
        } else {
            None
        };
        (inbox, activity)
    };

    view.dashboard_chunks = Some(DashboardChunks {
        health: top_chunks[0],
        statistics: stats_row[0],
        work_queues: stats_row[1],
        lifecycle: top_chunks[2],
        suggested_path: top_chunks[3],
        lower_inbox: inbox_block,
        lower_activity: activity_block,
        inbox_side_by_side: side_by_side,
        compact,
    });

    // Scrollbar gutter sits next to the inbox block (the only
    // interactive lower panel). When side-by-side the rail stays on
    // the right of the inbox; when stacked the rail sits inside the
    // inbox's own gutter.
    let inbox_inner_width = inbox_block.width.saturating_sub(SCROLLBAR_GUTTER);
    let inbox_inner = Rect {
        x: inbox_block.x,
        y: inbox_block.y,
        width: inbox_inner_width,
        height: inbox_block.height,
    };
    let rail_y = inbox_block.y.saturating_add(1);
    let rail_height = inbox_block.height.saturating_sub(2);
    let gutter_area = Rect {
        x: inbox_inner.x.saturating_add(inbox_inner.width),
        y: rail_y,
        width: SCROLLBAR_GUTTER,
        height: rail_height,
    };

    if inbox_block.height < 3 {
        return;
    }
    let inner_x = inbox_inner.x.saturating_add(1);
    let inner_y = inbox_inner.y.saturating_add(1);
    let inner_width = inbox_inner.width.saturating_sub(2);
    let inner_height = inbox_inner.height.saturating_sub(2);
    if inner_height < 2 {
        return;
    }

    let inbox = app.visible_inbox();
    let kind_order = dashboard::inbox_kind_order();

    // Sort the inbox into a flat (kind, item) order so that
    // `flat_idx` mirrors what the renderer increments. We compute the
    // scroll offset against this flat-order count so the renderer,
    // hit-test, and scrollbar agree.
    let mut flat_items: Vec<&super::app::InboxLine> = Vec::new();
    for kind in kind_order {
        for item in inbox.iter().filter(|i| i.kind == *kind) {
            flat_items.push(item);
        }
    }
    for item in inbox
        .iter()
        .filter(|i| !kind_order.contains(&i.kind.as_str()))
    {
        flat_items.push(item);
    }
    let total = flat_items.len();

    let visible_items = (inner_height as usize / INBOX_ROW_HEIGHT).max(1);
    let scroll = compute_list_scroll(app.selected_index, visible_items, total);

    if gutter_area.width > 0 {
        view.scrollbar_rects.push(ScrollbarHitArea {
            id: ScrollableId::OverviewInbox,
            rect: gutter_area,
            scroll,
            total,
            visible: visible_items,
        });
    }

    let mut y = inner_y.saturating_add(1);
    let block_bottom = inner_y.saturating_add(inner_height);
    let mut flat_idx = 0usize;

    for kind in kind_order {
        let items: Vec<_> = inbox.iter().filter(|i| i.kind == *kind).collect();
        if items.is_empty() {
            continue;
        }
        let group_last = flat_idx + items.len() - 1;
        if group_last < scroll {
            flat_idx += items.len();
            continue;
        }
        if y >= block_bottom {
            return;
        }
        y = y.saturating_add(1);
        if y >= block_bottom {
            return;
        }
        for item in items {
            if flat_idx < scroll {
                flat_idx += 1;
                continue;
            }
            let visible = block_bottom.saturating_sub(y);
            if visible == 0 {
                return;
            }
            let height = (INBOX_ROW_HEIGHT as u16).min(visible);
            view.list_item_rects.push(ListItemHitArea {
                id: item.id.clone(),
                rect: Rect {
                    x: inner_x,
                    y,
                    width: inner_width,
                    height,
                },
            });
            y = y.saturating_add(INBOX_ROW_HEIGHT as u16);
            flat_idx += 1;
        }
    }

    for item in inbox
        .iter()
        .filter(|i| !kind_order.contains(&i.kind.as_str()))
    {
        if flat_idx < scroll {
            flat_idx += 1;
            continue;
        }
        if y >= block_bottom {
            return;
        }
        view.list_item_rects.push(ListItemHitArea {
            id: item.id.clone(),
            rect: Rect {
                x: inner_x,
                y,
                width: inner_width,
                height: 1,
            },
        });
        y = y.saturating_add(1);
        flat_idx += 1;
    }
}

#[allow(dead_code)]
fn _junk_marker_removed() {}

/// M221: hit areas for the Autopilot lane picker. The renderer
/// (`render::watch::render_picker`) splits the content area 40/60
/// and lays out each candidate as a single row inside a bordered
/// block on the left; we mirror that geometry here so click
/// resolution agrees with the rendered glyphs by construction.
///
/// Each candidate gets one `ListItemHitArea` keyed by the
/// candidate's `id`. The picker surface does NOT use a scrollbar
/// (the picker is intentionally short — the queue panel above
/// is the multi-row surface), so no scrollbar hit area is added
/// here.
fn compute_autopilot_picker_rects(view: &mut ViewState, app: &App, area: Rect) {
    if area.width < 4 || area.height < 4 {
        return;
    }

    // The picker takes the left 40% of the content area — same
    // split as `render_watch_lane`.
    let picker_area = Rect {
        x: area.x,
        y: area.y,
        width: (area.width as u32 * 40 / 100) as u16,
        height: area.height,
    };

    // Prefer the typed picker candidates; fall back to the legacy
    // `app.watch.candidates` so the backcompat surface stays
    // clickable (the same backcompat path `render_picker` walks).
    let candidates: Vec<String> = if !app.autopilot.picker.candidates.is_empty() {
        app.autopilot
            .picker
            .candidates
            .iter()
            .map(|c| c.id.clone())
            .collect()
    } else {
        app.watch.candidates.iter().map(|c| c.id.clone()).collect()
    };
    if candidates.is_empty() {
        return;
    }

    // Picker block: bordered box at `picker_area`. Border eats
    // the top + bottom rows; data starts at `picker_area.y + 1`
    // and is `area.height - 2` rows tall.
    let inner_y_start = picker_area.y.saturating_add(1);
    let inner_height = picker_area.height.saturating_sub(2);
    if inner_height == 0 {
        return;
    }
    let inner_x = picker_area.x.saturating_add(1);
    let inner_width = picker_area.width.saturating_sub(2);

    for (i, id) in candidates.iter().enumerate() {
        if i as u16 >= inner_height {
            break;
        }
        view.list_item_rects.push(ListItemHitArea {
            id: id.clone(),
            rect: Rect {
                x: inner_x,
                y: inner_y_start.saturating_add(i as u16),
                width: inner_width,
                height: 1,
            },
        });
    }
}

/// M157: single vertical Path tree scrollbar. Mirrors detail-scroll
/// semantics (`path_scroll` / `path_max_scroll`) rather than the
/// pre-M157 multi-column swimlane layout.
fn compute_path_scrollbar_rects(view: &mut ViewState, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let track = scrollbar_rect(area, SCROLLBAR_GUTTER);
    if track.width == 0 {
        return;
    }
    // Borders eat 2 rows; inner viewport is what path_view scrolls against.
    let visible = area.height.saturating_sub(2) as usize;
    let line_count = app
        .path_data
        .as_ref()
        .map(|data| super::path_view::build_tree_lines(app, data).len())
        .unwrap_or(0);
    let max = line_count.saturating_sub(visible);
    // Keep path_max_scroll in sync so keyboard/page scroll clamps match
    // the scrollbar even before the paint pass.
    app.path_max_scroll.set(max as u16);
    let scroll = (app.path_scroll as usize).min(max);
    let total = if max == 0 {
        visible
    } else {
        max.saturating_add(visible)
    };
    view.scrollbar_rects.push(ScrollbarHitArea {
        id: ScrollableId::PathLane,
        rect: track,
        scroll,
        total,
        visible,
    });
}

/// M137: scrollbar for the detail screens. Detail views are
/// single-paragraph scrollback (`app.detail_scroll: u16`,
/// `app.detail_max_scroll: Cell<u16>`); total and scroll are rough
/// approximations of the rendered paragraph because we don't track
/// the per-paragraph line count, but the gutter is reserved on every
/// detail screen so AC-01 holds uniformly.
fn push_detail_scrollbar(view: &mut ViewState, app: &App, area: Rect, id: ScrollableId) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let track = scrollbar_rect(area, SCROLLBAR_GUTTER);
    if track.width == 0 {
        return;
    }
    let max = app.detail_max_scroll.get() as usize;
    let visible = area.height.saturating_sub(2) as usize;
    let scroll = app.detail_scroll as usize;
    // `total` includes the visible window so `total ≤ visible` means
    // "content fits". `max_scroll` is the last scrollable row, so the
    // total rendered rows is `max_scroll + visible` (when max_scroll >
    // 0) or `visible` (when nothing overflows).
    let total = if max == 0 {
        visible
    } else {
        max.saturating_add(visible)
    };
    view.scrollbar_rects.push(ScrollbarHitArea {
        id,
        rect: track,
        scroll,
        total,
        visible,
    });
}

/// M135: co-approval vertical split — `[header_block, body_block,
/// actions_block, status_block]`. Mirrors `render_co_approval`.
fn compute_co_approval_chunks(view: &mut ViewState, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);
    view.co_approval_chunks = Some([chunks[0], chunks[1], chunks[2], chunks[3]]);
}

/// M135: localized copy of `render::centered_rect`. The original is
/// `fn` (private); the new module re-implements the same math so
/// overlay hit areas match the rendered overlay exactly. `pub(super)`
/// so `render.rs`'s overlay helpers (help / input / review menu) can
/// share the same implementation.
pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

// =============================================================================
// M135 (S4) — unit tests for the tab-bar layout machinery
// =============================================================================
//
// The three layout functions (`compute_tab_bar_layout`,
// `visible_tab_x_ranges`, `tab_text_width`) and the `TabBarLayout`
// struct are `pub(super)` — accessible from sibling modules in
// `tui/` but NOT from integration tests in `crates/raul/tests/`.
// Tests that exercise the layout internals live here, where
// `pub(super)` is sufficient; the integration tests in
// `tui_mouse.rs` / `tab_bar_narrow_width.rs` that previously
// called these directly have been refactored to drive the layout
// through `compute_view` (the public entry point) and inspect the
// resulting `view.tab_layout` / `view.tab_hit_areas`.
//
// These tests were originally written under M105 S1 (B-39) and
// M124 (M91 ER-2) and lived in `crates/raul/tests/` against the
// then-pub layout functions. Moving them here is the only way to
// satisfy the S4 done_when "no longer pub" while keeping the
// coverage intact.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Lane;
    use crate::tui::runner::tab_hit_test_for_layout;

    fn lane_at(idx: usize) -> Lane {
        Lane::ordered()[idx]
    }

    fn narrow_overflow_layout(active_idx: usize) -> TabBarLayout {
        let layout = compute_tab_bar_layout(16, true, &lane_at(active_idx), &Lane::ordered());
        assert!(
            layout.overflowed,
            "test premise: width=16 must overflow the compact-mode bar; \
             got visible={:?}",
            layout.visible
        );
        layout
    }

    fn wide_compact_layout(active_idx: usize) -> TabBarLayout {
        let layout = compute_tab_bar_layout(50, true, &lane_at(active_idx), &Lane::ordered());
        assert!(
            !layout.overflowed,
            "test premise: width=50 must not overflow the compact-mode bar"
        );
        layout
    }

    fn wide_full_layout(active_idx: usize) -> TabBarLayout {
        // M184: 7 lanes still need headroom past 80 cols for full
        // labels. 110 is a safe upper bound for the test premise.
        let layout = compute_tab_bar_layout(110, false, &lane_at(active_idx), &Lane::ordered());
        assert!(
            !layout.overflowed,
            "test premise: width=110 must not overflow the full-mode bar"
        );
        layout
    }

    // ---- M105 / B-39: narrow-width overflow hit-test (originally
    //      `tui_mouse.rs::m105_overflow_hit_test`) ---------------------

    #[test]
    fn overflow_hit_test_selects_visible_only() {
        let layout = narrow_overflow_layout(1);
        let ranges = visible_tab_x_ranges(&layout, &Lane::ordered());

        // Sanity: the active lane is always visible, even under overflow.
        assert!(
            ranges.iter().any(|r| r.0 == 1),
            "active lane idx=1 must be visible; got ranges={:?}",
            ranges
        );

        // Lane widths are no longer pinned (the M137+ format dropped
        // the leading space on non-first lanes), so the left/right
        // indicator positions are derived from the layout rather than
        // hardcoded. The contracts we still want to pin:
        //   * indicator cells never resolve to a hidden lane.
        //   * ellipsis cells (when present) never resolve to a
        //     hidden lane.
        // The "must be None" assertions below assert both.
        let layout_total = {
            let last = ranges.last().expect("at least one visible lane");
            // Right indicator sits just past the last visible tab —
            // the test premise is width=20 narrow overflow.
            let indicator_w = INDICATOR_WIDTH as u16;
            last.2 + indicator_w
        };

        if layout.has_left_indicator {
            for x in 0u16..INDICATOR_WIDTH as u16 {
                assert_eq!(
                    tab_hit_test_for_layout(x, &layout, &Lane::ordered()),
                    None,
                    "click x={x} landed on left indicator; must be None (got hit on hidden tab)"
                );
            }
        }

        if layout.has_right_indicator {
            // Walk back from the end of the bar — the right indicator
            // lives in the trailing `INDICATOR_WIDTH` cells.
            for x in (layout_total.saturating_sub(INDICATOR_WIDTH as u16)..layout_total).rev() {
                assert_eq!(
                    tab_hit_test_for_layout(x, &layout, &Lane::ordered()),
                    None,
                    "click x={x} landed on right indicator; must be None"
                );
            }
        }

        if layout.has_left_ellipsis {
            let ellipsis_w = " \u{2026} ".chars().count() as u16;
            let ellipsis_start = INDICATOR_WIDTH as u16;
            for x in ellipsis_start..(ellipsis_start + ellipsis_w) {
                assert_eq!(
                    tab_hit_test_for_layout(x, &layout, &Lane::ordered()),
                    None,
                    "click x={x} landed on left ellipsis; must be None"
                );
            }
        }
    }

    #[test]
    fn overflow_hit_test_matches_render_column_walk() {
        fn render_tab_ranges(layout: &TabBarLayout) -> Vec<(usize, u16, u16)> {
            let lanes = Lane::ordered();
            let ellipsis_w = " \u{2026} ".chars().count();
            let indicator_w = 3usize;
            let mut out = Vec::new();
            let mut cursor =
                if layout.overflowed && (layout.has_left_indicator || layout.has_right_indicator) {
                    indicator_w
                        + if layout.has_left_ellipsis {
                            ellipsis_w
                        } else {
                            0
                        }
                } else {
                    1
                };

            let mut push = |idx: usize, cursor: &mut usize| {
                let w = tab_text_width(&lanes[idx], idx, layout.compact);
                out.push((idx, *cursor as u16, (*cursor + w) as u16));
                *cursor += w;
            };

            let n = layout.visible.len();
            if n >= 1 {
                push(layout.visible[0], &mut cursor);
            }
            if n >= 3 {
                for &idx in &layout.visible[1..n - 1] {
                    push(idx, &mut cursor);
                }
            }
            if layout.has_right_ellipsis {
                cursor += ellipsis_w;
            }
            if n >= 2 {
                push(layout.visible[n - 1], &mut cursor);
            }
            out
        }

        for active_idx in 0..Lane::ordered().len() {
            for &width in &[10u16, 14, 18, 20, 25, 30] {
                let layout =
                    compute_tab_bar_layout(width, true, &lane_at(active_idx), &Lane::ordered());
                if !layout.overflowed {
                    continue;
                }
                let hit = visible_tab_x_ranges(&layout, &Lane::ordered());
                let render = render_tab_ranges(&layout);
                assert_eq!(
                    hit, render,
                    "width={width} active={active_idx}: hit-test ranges must match render walk; \
                     visible={:?} left_ell={} right_ell={}",
                    layout.visible, layout.has_left_ellipsis, layout.has_right_ellipsis
                );
            }
        }
    }

    #[test]
    fn overflow_clicks_off_the_bar_return_none() {
        let layout = narrow_overflow_layout(2);
        for x in [20u16, 21, 30, 100, u16::MAX] {
            assert_eq!(
                tab_hit_test_for_layout(x, &layout, &Lane::ordered()),
                None,
                "click x={x} is outside the rendered bar; must be None"
            );
        }
    }

    #[test]
    fn overflow_hit_test_skips_hidden_lanes() {
        // Active lane idx=3 (Backlog). Width=20 triggers overflow. The
        // active lane is always visible; a hidden tab at idx=0 (Overview)
        // would have been resolved by the pre-M91 hit-test (which assumed
        // a contiguous layout of all lanes).
        let layout = narrow_overflow_layout(3);

        assert!(
            !layout.visible.contains(&0),
            "test premise: lane idx=0 (Overview) must be hidden at width=20 with active idx=3; got visible={:?}",
            layout.visible
        );

        for x in 0u16..20 {
            if let Some(idx) = tab_hit_test_for_layout(x, &layout, &Lane::ordered()) {
                assert!(
                    layout.visible.contains(&idx),
                    "click x={x} resolved to hidden lane idx={idx}; \
                     visible lanes are {:?}",
                    layout.visible
                );
            }
        }
    }

    #[test]
    fn overflow_active_is_always_visible_regardless_of_width() {
        // Try three extreme widths, each spanning the active lane from
        // idx=0 to the last lane. The visible set must always include
        // the active lane — otherwise the user wouldn't be able to see
        // which tab they're on.
        for active_idx in 0..Lane::ordered().len() {
            for &width in &[10u16, 14, 18, 25, 30, 40] {
                let layout =
                    compute_tab_bar_layout(width, true, &lane_at(active_idx), &Lane::ordered());
                if layout.overflowed {
                    assert!(
                        layout.visible.contains(&active_idx),
                        "active lane idx={active_idx} must always be visible at width={width}; got visible={:?}",
                        layout.visible
                    );
                }
            }
        }
    }

    #[test]
    fn wide_compact_keeps_old_tab_hit_test_answers() {
        // Non-overflow compact mode: the shared layout returns visible =
        // 0..total, no indicators. `tab_hit_test_for_layout` should
        // produce the same column→lane mapping that the pre-existing
        // `tab_hit_test(x, compact=true)` helper would, modulo the
        // leading-space semantics.
        let layout = wide_compact_layout(0);
        assert!(!layout.overflowed);
        assert_eq!(
            layout.visible,
            (0..Lane::ordered().len()).collect::<Vec<_>>()
        );

        assert_eq!(tab_hit_test_for_layout(0, &layout, &Lane::ordered()), None);
        assert_eq!(
            tab_hit_test_for_layout(1, &layout, &Lane::ordered()),
            Some(0)
        );
        assert_eq!(
            tab_hit_test_for_layout(3, &layout, &Lane::ordered()),
            Some(0)
        );
        assert_eq!(
            tab_hit_test_for_layout(200, &layout, &Lane::ordered()),
            None
        );
    }

    #[test]
    fn wide_full_keeps_old_tab_hit_test_answers() {
        // Non-overflow full mode: visible = 0..5, no indicators. Column
        // ranges are read off `visible_tab_x_ranges` rather than pinned
        // — the lane widths changed in M137+ (each non-first lane
        // dropped its leading space so the bar renders consistent
        // `name | name | name` spacing), so the absolute pixel
        // positions are an implementation detail.
        let layout = wide_full_layout(0);
        assert!(!layout.overflowed);

        let ranges = visible_tab_x_ranges(&layout, &Lane::ordered());
        // Lane widths we care about: ranges[i] = (idx, start_x, end_x).
        let by_idx: std::collections::HashMap<usize, (u16, u16)> = ranges
            .iter()
            .map(|(idx, sx, ex)| (*idx, (*sx, *ex)))
            .collect();

        // Click immediately before the bar's left margin returns None.
        assert_eq!(tab_hit_test_for_layout(0, &layout, &Lane::ordered()), None);
        // First-lane interior returns idx=0 (Overview).
        assert_eq!(
            tab_hit_test_for_layout(1, &layout, &Lane::ordered()),
            Some(0)
        );
        // First-lane trailing space still maps to Overview.
        let (_, e0) = by_idx[&0];
        assert_eq!(
            tab_hit_test_for_layout(e0.saturating_sub(1), &layout, &Lane::ordered()),
            Some(0)
        );
        // Click in the third lane (idx=2 = Path) picks Path.
        let (s2, e2) = by_idx[&2];
        assert_eq!(
            tab_hit_test_for_layout(s2, &layout, &Lane::ordered()),
            Some(2)
        );
        assert_eq!(
            tab_hit_test_for_layout(e2.saturating_sub(1), &layout, &Lane::ordered()),
            Some(2)
        );
        // After the bar ends, every click is None.
        assert_eq!(
            tab_hit_test_for_layout(200, &layout, &Lane::ordered()),
            None
        );
    }

    // ---- M115 / M124: narrow-width budget (originally
    //      `tab_bar_narrow_width.rs`) -----------------------------------

    #[test]
    fn layout_narrow_width_does_not_exceed_width() {
        for w in 1u16..=5 {
            for lane in [Lane::Overview, Lane::Milestones] {
                for compact in [false, true] {
                    let layout = compute_tab_bar_layout(w, compact, &lane, &Lane::ordered());
                    let ranges = visible_tab_x_ranges(&layout, &Lane::ordered());
                    let last_end = ranges
                        .iter()
                        .map(|(_, _, end)| *end as usize)
                        .max()
                        .unwrap_or(0);
                    assert!(
                        last_end <= w as usize,
                        "width={w} compact={compact} active={lane:?}: render ranges {:?} overflow past width={w} (last_end={last_end})",
                        ranges
                    );
                    assert!(
                        !layout.has_left_indicator
                            && !layout.has_right_indicator
                            && !layout.has_left_ellipsis
                            && !layout.has_right_ellipsis,
                        "width={w} compact={compact} active={lane:?}: indicators must be suppressed at narrow width, got layout={:?}",
                        layout
                    );
                }
            }
        }
    }

    #[test]
    fn layout_at_width_5_still_renders_active_lane() {
        let layout = compute_tab_bar_layout(5, true, &Lane::Overview, &Lane::ordered());
        assert!(
            layout.visible.contains(&0),
            "active lane Overview must be visible at width=5; got visible={:?}",
            layout.visible
        );
    }

    #[test]
    fn layout_at_width_1_renders_nothing() {
        let layout = compute_tab_bar_layout(1, true, &Lane::Overview, &Lane::ordered());
        assert!(
            layout.visible.is_empty(),
            "width=1 must yield no visible lanes, got {:?}",
            layout.visible
        );
    }

    #[test]
    fn wide_width_with_fitting_active_keeps_indicator_logic() {
        let layout = compute_tab_bar_layout(16, false, &Lane::Milestones, &Lane::ordered());
        let lanes = Lane::ordered();
        let milestones_idx = lanes.iter().position(|l| *l == Lane::Milestones).unwrap();
        assert!(
            layout.visible.contains(&milestones_idx),
            "active lane Milestones must be visible at width=16; got visible={:?}",
            layout.visible
        );
    }

    #[test]
    fn active_tab_text_does_not_fit_yields_empty_visible() {
        // LANE_MILESTONES label is 10 cells wide; tab text adds the
        // ' │ ' / ' │ ' separators for 13 cells total. active_label_w + 1
        // = 14; widths < 14 fire the new branch.
        for w in 6u16..=13 {
            let layout = compute_tab_bar_layout(w, false, &Lane::Milestones, &Lane::ordered());
            assert!(
                layout.visible.is_empty(),
                "width={w} + Milestones must yield empty visible (tab text=13 > w); got {:?}",
                layout.visible
            );
        }
    }

    #[test]
    fn active_lane_visible_and_no_overflow_at_fitting_widths() {
        let lanes = Lane::ordered();
        for w in 6u16..=16 {
            for lane in [Lane::Overview, Lane::Milestones] {
                let layout = compute_tab_bar_layout(w, false, &lane, &Lane::ordered());
                let active_idx = lanes.iter().position(|l| l == &lane).unwrap_or(0);
                if layout.visible.is_empty() {
                    continue;
                }
                assert!(
                    layout.visible.contains(&active_idx),
                    "width={w} active={lane:?}: active lane must be visible; got visible={:?}",
                    layout.visible
                );
                let indicator_w = if layout.has_left_indicator { 3 } else { 0 }
                    + if layout.has_right_indicator { 3 } else { 0 };
                let ellipsis_w = if layout.has_left_ellipsis { 4 } else { 0 }
                    + if layout.has_right_ellipsis { 4 } else { 0 };
                let tabs_w: usize = layout
                    .visible
                    .iter()
                    .map(|&i| {
                        let label = if layout.compact {
                            lanes[i].compact_label()
                        } else {
                            lanes[i].label()
                        };
                        let s = if i == 0 {
                            format!(" {label} ")
                        } else {
                            format!("│ {label} ")
                        };
                        s.chars().count()
                    })
                    .sum();
                let total = indicator_w + ellipsis_w + tabs_w;
                assert!(
                    total <= w as usize,
                    "width={w} active={lane:?}: total emitted width {total} overflows past width={w} \
                     (indicator={indicator_w}, ellipsis={ellipsis_w}, tabs={tabs_w}, layout={layout:?})"
                );
            }
        }
    }
}
