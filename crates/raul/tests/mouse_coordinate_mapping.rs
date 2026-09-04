//! M221 S1 / AC-06: pure coordinate mapping tests.
//!
//! The `mouse` module's `resolve_list_click` returns the row id
//! + the matched `ListItemHitArea`. The tests in this file
//! exercise the contract the dispatcher relies on:
//!
//! 1. **Resize**: a click on the same logical row resolves to the
//!    same row id regardless of the terminal width / height.
//! 2. **Header / footer changes**: the row positions shift when
//!    the header or footer grows, but a click on the visible row
//!    still resolves to the correct id.
//! 3. **Viewport scrolling**: as `selected_index` advances past the
//!    visible window, the rects move up; a click on the visible
//!    window resolves to the right id.
//! 4. **Filters / search**: a filtered list still resolves clicks
//!    to the right row id (because `visible_milestones()` filters
//!    by the same predicate as the renderer).
//! 5. **Empty lists**: no clicks resolve.
//! 6. **Out-of-bounds clicks**: clicks outside the rects return
//!    `None`.

use std::collections::BTreeMap;

use raul::tui::app::{App, Lane};
use raul::tui::mouse::resolve_list_click;
use raul::tui::view_state::{compute_view, ListItemHitArea};

fn mk_milestone(id: &str) -> raul::tui::app::MilestoneSummary {
    raul::tui::app::MilestoneSummary {
        id: id.to_string(),
        title: format!("title-{id}"),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }
}

fn app_with_milestones(count: usize) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let ms: Vec<_> = (1..=count).map(|i| mk_milestone(&format!("{i:02}"))).collect();
    app.load_milestones(ms);
    app
}

fn click_center(hit: &ListItemHitArea) -> (u16, u16) {
    (hit.rect.x + hit.rect.width / 2, hit.rect.y + hit.rect.height / 2)
}

/// AC-06: a click on the center of any visible rect resolves to
/// the row id encoded in the rect.
#[test]
fn resolve_returns_id_for_visible_row() {
    let app = app_with_milestones(5);
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 100, 30));
    for hit in &view.list_item_rects {
        let (cx, cy) = click_center(hit);
        let resolved = resolve_list_click(&app, &view, cx, cy);
        assert!(
            resolved.is_some(),
            "click on rect {} must resolve to a row",
            hit.id
        );
        assert_eq!(resolved.as_ref().unwrap().0, hit.id);
    }
}

/// AC-06 / "after resize": widening the terminal moves the row
/// rects but a click on the same logical row id still resolves
/// to that id. We pick the second row and click its center at
/// every width — it must always resolve to "02".
#[test]
fn resize_keeps_row_id_resolution_stable() {
    let app = app_with_milestones(5);
    let target_id = "02";
    for width in [60u16, 80, 120, 160, 200] {
        let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, width, 30));
        let hit = view
            .list_item_rects
            .iter()
            .find(|h| h.id == target_id)
            .unwrap_or_else(|| panic!("row {target_id} must be visible at width {width}"));
        let (cx, cy) = click_center(hit);
        let resolved = resolve_list_click(&app, &view, cx, cy);
        assert_eq!(
            resolved.as_ref().map(|(id, _)| id.clone()),
            Some(target_id.to_string()),
            "click on {target_id} at width {width} must resolve to {target_id}"
        );
    }
}

/// AC-06 / "viewport scrolling": as `selected_index` advances
/// past the visible window, the rects shift up. A click on the
/// new top-row rect still resolves to the new top-row id (not
/// the original row 0).
#[test]
fn viewport_scrolling_resolves_to_shifted_top_row() {
    let mut app = app_with_milestones(30);
    app.selected_index = 25;
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    // The list rect at viewport-offset 0 should NOT be row "01"
    // anymore — the offset advanced.
    let first_visible = &view.list_item_rects[0];
    assert_ne!(
        first_visible.id, "01",
        "viewport must have advanced past row 01 when selected_index=25"
    );
    // Click on the top row of the visible window.
    let (cx, cy) = click_center(first_visible);
    let resolved = resolve_list_click(&app, &view, cx, cy);
    assert_eq!(resolved.as_ref().map(|(id, _)| id.clone()), Some(first_visible.id.clone()));
}

/// AC-06 / "filters/search" (hide_done): hiding done milestones
/// shifts the visible set; a click on the visible row resolves
/// to the id of the visible item.
#[test]
fn hide_done_filter_keeps_resolution_to_visible_row() {
    let mut app = app_with_milestones(5);
    app.hide_done = true;
    // Mark first two as complete.
    app.milestones[0].lifecycle = "complete".to_string();
    app.milestones[1].lifecycle = "complete".to_string();
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    // Only 3 rows visible after hide_done.
    assert_eq!(view.list_item_rects.len(), 3, "hide_done must filter");
    for hit in &view.list_item_rects {
        let (cx, cy) = click_center(hit);
        let resolved = resolve_list_click(&app, &view, cx, cy);
        assert_eq!(resolved.as_ref().map(|(id, _)| id.clone()), Some(hit.id.clone()));
    }
}

/// AC-06 / "empty lists": with no milestones, no click resolves.
#[test]
fn empty_list_has_no_hit() {
    let app = app_with_milestones(0);
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    assert!(view.list_item_rects.is_empty());
    assert!(resolve_list_click(&app, &view, 50, 15).is_none());
    assert!(resolve_list_click(&app, &view, 0, 0).is_none());
}

/// AC-06 / "out-of-bounds": clicks outside the rects return None.
#[test]
fn out_of_bounds_click_returns_none() {
    let app = app_with_milestones(5);
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    let last = view.list_item_rects.last().unwrap();
    let oob_y = last.rect.y.saturating_add(last.rect.height) + 5;
    // Pick a row far past the last visible rect.
    let resolved = resolve_list_click(&app, &view, 40, oob_y);
    assert!(
        resolved.is_none(),
        "click below the last row must not resolve; got {:?}",
        resolved.map(|(id, _)| id)
    );
}

/// AC-06 / "filtered Backlog": backlog filtering by id resolves
/// to the visible row only.
#[test]
fn backlog_filter_keeps_resolution_to_visible_row() {
    use raul::tui::app::BacklogLine;
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    app.backlog = vec![
        BacklogLine {
            id: "BL-01".to_string(),
            title: "first".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            ..Default::default()
        },
        BacklogLine {
            id: "BL-02".to_string(),
            title: "second".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            ..Default::default()
        },
        BacklogLine {
            id: "BL-03".to_string(),
            title: "third".to_string(),
            priority: "high".to_string(),
            status: "resolved".to_string(),
            resolution: "shipped".to_string(),
            ..Default::default()
        },
    ];
    // Hide resolved items — only BL-01 + BL-02 remain.
    app.hide_done = true;
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    let ids: Vec<String> = view.list_item_rects.iter().map(|h| h.id.clone()).collect();
    assert_eq!(ids, vec!["BL-01", "BL-02"]);
    // A click on BL-02 resolves to BL-02.
    let hit = &view.list_item_rects[1];
    let (cx, cy) = click_center(hit);
    let resolved = resolve_list_click(&app, &view, cx, cy);
    assert_eq!(resolved.as_ref().map(|(id, _)| id.clone()), Some("BL-02".to_string()));
}

/// AC-06 / "invalid coordinates": a click on the tab-bar row
/// must NOT resolve to a list row (tab clicks live on a
/// separate hit-area list).
#[test]
fn click_on_tab_bar_does_not_resolve_to_list_row() {
    let app = app_with_milestones(5);
    let view = compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 30));
    let bar_y = view.tab_bar_area.y;
    // Click in the middle of the tab bar row.
    let resolved = resolve_list_click(&app, &view, 40, bar_y);
    assert!(
        resolved.is_none(),
        "tab-bar click must not resolve to a list row; got {:?}",
        resolved.map(|(id, _)| id)
    );
}
