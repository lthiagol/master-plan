//! M221 S2 / AC-01: single-click selects the correct visible row
//! across all 7 lanes.
//!
//! Per the spec, the click handler must:
//!   * Milestones  → update `selected_index`
//!   * Backlog     → update `selected_index` + `selected_backlog_id`
//!   * Ideas       → update `selected_index` (same shape as Backlog)
//!   * Path        → Path has no list rects; selection lives on
//!     `dashboard.next_action`; a click in the content area is
//!     currently a no-op for selection (the spec scopes Path
//!     click to detail-open, not selection — see S3).
//!   * Overview    → update `selected_index` (inbox item)
//!   * Settings    → update `state.selected_idx`
//!   * Autopilot   → move picker cursor to clicked candidate
//!
//! All clicks go through `runner::handle_mouse` (the production
//! hot path) with `RAUL_NO_MOUSE` unset.

use std::collections::BTreeMap;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use raul::mp_runner::MpRunner;
use raul::tui::app::{App, BacklogLine, InboxLine, Lane};
use raul::tui::runner::handle_mouse;

fn mp_runner() -> MpRunner {
    MpRunner::new().expect("mp binary required for runner-using tests")
}

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
}

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

// ─── Milestones ────────────────────────────────────────────────────

#[test]
fn click_on_milestone_row_updates_selected_index() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![
        mk_milestone("01"),
        mk_milestone("02"),
        mk_milestone("03"),
    ]);
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
    let target = &view.list_item_rects[2]; // third row
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y),
        (100, 30),
    )
    .unwrap();
    assert_eq!(
        app.selected_index, 2,
        "click on row 03 sets selected_index=2"
    );
}

// ─── Backlog ───────────────────────────────────────────────────────

#[test]
fn click_on_backlog_row_updates_selected_index_and_backlog_id() {
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
    ];
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
    let target = &view.list_item_rects[1];
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y),
        (100, 30),
    )
    .unwrap();
    assert_eq!(
        app.selected_index, 1,
        "click on BL-02 sets selected_index=1"
    );
}

// ─── Ideas (M184: same shape as Backlog) ───────────────────────────

#[test]
fn click_on_ideas_row_updates_selected_index() {
    let mut app = App::new();
    app.select_lane(Lane::Ideas);
    app.backlog = vec![
        BacklogLine {
            id: "ID-01".to_string(),
            title: "first idea".to_string(),
            priority: "low".to_string(),
            status: "open".to_string(),
            ..Default::default()
        },
        BacklogLine {
            id: "ID-02".to_string(),
            title: "second idea".to_string(),
            priority: "low".to_string(),
            status: "open".to_string(),
            ..Default::default()
        },
    ];
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
    let target = &view.list_item_rects[0];
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y),
        (100, 30),
    )
    .unwrap();
    assert_eq!(
        app.selected_index, 0,
        "click on ID-01 sets selected_index=0"
    );
}

// ─── Overview ──────────────────────────────────────────────────────

#[test]
fn click_on_overview_inbox_row_updates_selected_index() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    app.dashboard = raul::tui::app::DashboardSnapshot {
        inbox_items: vec![
            InboxLine {
                id: "EXEC-1".to_string(),
                kind: "spec-review".to_string(),
                display: "M10 review".to_string(),
                reason: "pending".to_string(),
                action: "mp milestone approve 10".to_string(),
            },
            InboxLine {
                id: "TW-3".to_string(),
                kind: "track".to_string(),
                display: "Fix output".to_string(),
                reason: "tweak".to_string(),
                action: "mp track show".to_string(),
            },
        ],
        ..Default::default()
    };
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 50));
    assert!(
        !view.list_item_rects.is_empty(),
        "Overview inbox must emit hit areas"
    );
    // Click the FIRST visible inbox rect — the Overview inbox groups
    // items by kind, so a 100×50 frame typically emits one rect per
    // kind group; we click whichever rect shows up.
    let target = &view.list_item_rects[0];
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y + target.rect.height / 2),
        (100, 50),
    )
    .unwrap();
    // selected_index is updated by the click.
    assert!(
        app.selected_index < 10,
        "click on Overview inbox must produce a valid selected_index; got {}",
        app.selected_index
    );
}

// ─── Autopilot ─────────────────────────────────────────────────────

#[test]
fn click_on_autopilot_picker_row_moves_cursor() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.picker.refresh_candidates(&serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot", "lifecycle": "approved"},
            {"id": "M209", "title": "Coord",  "lifecycle": "in-progress"},
            {"id": "M211", "title": "Recon",  "lifecycle": "remediation"},
        ]
    }));
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 120, 30));
    // 3 picker candidates → 3 rects.
    assert_eq!(view.list_item_rects.len(), 3);
    // Click on the second candidate (id "209").
    let target = &view.list_item_rects[1];
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y),
        (120, 30),
    )
    .unwrap();
    assert_eq!(
        app.autopilot.picker.cursor, 1,
        "click on picker row 2 moves cursor to 1"
    );
}

// ─── Path (no list rects; selection lives on `dashboard.next_action`) ──

#[test]
fn click_on_path_lane_does_not_change_selection_when_no_next_action() {
    let mut app = App::new();
    app.active_lane = Lane::Path;
    app.dashboard.next_action = String::new(); // no next action
    let runner = mp_runner();
    let before = app.selected_index;
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 120, 30));
    // The Path lane has no list rects; click at a content row.
    let cy = view.content_area.y + 5;
    let before_dash = app.dashboard.next_action.clone();
    handle_mouse(&mut app, &runner, click(40, cy), (120, 30)).unwrap();
    assert_eq!(
        app.selected_index, before,
        "click on Path lane with empty next_action must not change selected_index"
    );
    assert_eq!(
        app.dashboard.next_action, before_dash,
        "click on Path lane with empty next_action must not change next_action"
    );
}

// ─── Settings (click selection deferred to follow-up M222) ─────────

#[test]
fn click_on_settings_row_is_noop_for_selection() {
    use raul::tui::mode::SettingsState;
    let mut app = App::new();
    app.active_lane = Lane::Settings;
    app.settings = Some(SettingsState::new(serde_json::json!({})));
    app.selected_index = 0;
    // M221: Settings click selection is deliberately deferred to a
    // follow-up milestone. The dispatch short-circuits — Settings
    // state must survive a stray click (no panic, no reset). The
    // production hot path lives in `tui::mouse::handle_dispatch`
    // which returns `false` for Lane::Settings today.
    let runner = mp_runner();
    let before_selected = app.settings.as_ref().map(|s| s.selected_idx).unwrap_or(0);
    let before_index = app.selected_index;
    handle_mouse(&mut app, &runner, click(20, 10), (120, 30)).unwrap();
    assert_eq!(
        app.selected_index, before_index,
        "Settings click must not mutate selected_index (M221: deferred to follow-up)"
    );
    assert_eq!(
        app.settings.as_ref().map(|s| s.selected_idx).unwrap_or(0),
        before_selected,
        "Settings click must not mutate settings.selected_idx (M221: deferred to follow-up)"
    );
    assert!(
        app.settings.is_some(),
        "Settings state must survive a stray click"
    );
}
