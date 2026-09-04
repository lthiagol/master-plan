//! M221 S3 / AC-02: double-click on a list row invokes the same
//! detail action as keyboard Enter — for Milestones, Backlog,
//! Ideas, Path, and Autopilot. Overview and Settings explicitly
//! stay selection-only.
//!
//! The dispatch path is the production `handle_mouse` — the test
//! drives two Left-Down events within the double-click window
//! at the same coordinates and asserts the detail state flips.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use raul::mp_runner::MpRunner;
use raul::tui::app::{App, BacklogLine, ContentState, Lane};
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

/// Drive a single Left-Down with a synthetic `last_click` history
/// so the next Down classifies as a double-click. Returns
/// immediately if `app.last_click` was set by the first Down.
fn send_double_click(app: &mut App, runner: &MpRunner, x: u16, y: u16, term_size: (u16, u16)) {
    let now = Instant::now();
    // Seed last_click 100 ms before now so the second Down
    // classifies as a Double (< DOUBLE_CLICK_MS=500).
    app.last_click = Some((x, y, now - Duration::from_millis(100)));
    handle_mouse(app, runner, click(x, y), term_size).unwrap();
}

// ─── Milestones: double-click opens detail ─────────────────────────

#[test]
fn double_click_on_milestone_opens_milestone_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![mk_milestone("01"), mk_milestone("02"), mk_milestone("03")]);
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
    let target = &view.list_item_rects[1];

    send_double_click(
        &mut app,
        &runner,
        target.rect.x + 5,
        target.rect.y,
        (100, 30),
    );

    assert_eq!(
        app.content,
        ContentState::MilestoneDetail,
        "double-click on Milestones row must open MilestoneDetail"
    );
}

// ─── Backlog: double-click opens detail ────────────────────────────

#[test]
fn double_click_on_backlog_opens_backlog_detail() {
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
    let target = &view.list_item_rects[0];

    send_double_click(
        &mut app,
        &runner,
        target.rect.x + 5,
        target.rect.y,
        (100, 30),
    );

    assert_eq!(
        app.content,
        ContentState::BacklogDetail,
        "double-click on Backlog row must open BacklogDetail"
    );
    assert_eq!(app.selected_backlog_id.as_deref(), Some("BL-01"));
}

// ─── Ideas: double-click opens detail (same shape as Backlog) ─────

#[test]
fn double_click_on_ideas_opens_backlog_detail() {
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

    send_double_click(
        &mut app,
        &runner,
        target.rect.x + 5,
        target.rect.y,
        (100, 30),
    );

    assert_eq!(
        app.content,
        ContentState::BacklogDetail,
        "double-click on Ideas row must open BacklogDetail"
    );
}

// ─── Overview: double-click is a no-op (selection-only per AC-02) ─

#[test]
fn double_click_on_overview_does_not_open_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    app.dashboard = raul::tui::app::DashboardSnapshot {
        inbox_items: vec![raul::tui::app::InboxLine {
            id: "EXEC-1".to_string(),
            kind: "spec-review".to_string(),
            display: "M10 review".to_string(),
            reason: "pending".to_string(),
            action: "mp milestone approve 10".to_string(),
        }],
        ..Default::default()
    };
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 50));
    assert!(!view.list_item_rects.is_empty());
    let target = &view.list_item_rects[0];

    send_double_click(
        &mut app,
        &runner,
        target.rect.x + 5,
        target.rect.y + target.rect.height / 2,
        (100, 50),
    );

    assert_eq!(
        app.content,
        ContentState::List,
        "double-click on Overview must NOT open detail (selection-only)"
    );
}

// ─── Autopilot: double-click toggles picker selection ─────────────

#[test]
fn double_click_on_autopilot_picker_toggles_selection() {
    let mut app = App::new();
    app.active_lane = Lane::Autopilot;
    app.autopilot.picker.refresh_candidates(&serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot", "lifecycle": "approved"},
            {"id": "M209", "title": "Coord",  "lifecycle": "in-progress"},
        ]
    }));
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 120, 30));
    let target = &view.list_item_rects[0];

    send_double_click(
        &mut app,
        &runner,
        target.rect.x + 5,
        target.rect.y,
        (120, 30),
    );

    let selected = app.autopilot.picker.queue_ids();
    assert!(
        selected.contains(&"207".to_string()),
        "double-click on Autopilot picker row 1 must toggle selection; got {:?}",
        selected
    );
}

// ─── Settings: double-click is a no-op (selection-only per AC-02) ─

#[test]
fn double_click_on_settings_does_not_open_detail() {
    use raul::tui::mode::SettingsState;
    let mut app = App::new();
    app.active_lane = Lane::Settings;
    app.settings = Some(SettingsState::new(serde_json::json!({})));
    let runner = mp_runner();
    let before_content = app.content.clone();

    send_double_click(&mut app, &runner, 20, 10, (120, 30));

    assert_eq!(
        app.content, before_content,
        "double-click on Settings must NOT change content state (selection-only)"
    );
}

// ─── Single click does NOT open detail (regression guard) ──────────

#[test]
fn single_click_on_milestone_does_not_open_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![mk_milestone("01"), mk_milestone("02")]);
    let runner = mp_runner();
    let view = raul::tui::view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
    let target = &view.list_item_rects[0];

    // Single click — no prior click history.
    handle_mouse(
        &mut app,
        &runner,
        click(target.rect.x + 5, target.rect.y),
        (100, 30),
    )
    .unwrap();

    assert_eq!(
        app.content,
        ContentState::List,
        "single click on Milestone must NOT open detail; only double-click does"
    );
    assert_eq!(
        app.selected_index, 0,
        "single click must still update selection"
    );
}
