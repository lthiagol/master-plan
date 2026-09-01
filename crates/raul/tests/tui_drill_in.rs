use raul::tui::app::{App, ContentState, Lane};
use std::collections::BTreeMap;

#[test]
fn drill_into_milestone_from_list() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "Test".into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    assert_eq!(app.content, ContentState::MilestoneDetail);
}

// AC-01 (M87): drilling in by id must resolve to the correct milestone even
// when hide_done hides the target from the visible list.
#[test]
fn drill_into_done_milestone_by_id_with_hide_done() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![
        raul::tui::app::MilestoneSummary {
            id: "01".into(),
            title: "Planned".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "42".into(),
            title: "Done one".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "43".into(),
            title: "Planned two".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        flow_stages: BTreeMap::new(),
        },
    ]);
    app.hide_done = true;
    // The done milestone (42) is NOT in visible_milestones().
    assert!(
        app.visible_milestones().iter().all(|m| m.id != "42"),
        "hide_done must filter out the done milestone"
    );
    // Drill in by id (Board/Path/Inbox path) — must resolve to 42, not to
    // the visible row at some index.
    app.enter_milestone_detail_by_id("42");
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.selected_milestone_id.as_deref(), Some("42"));
}

#[test]
fn drill_into_backlog_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-01".into(),
        title: "Item".into(),
        priority: "high".into(),
        status: "open".into(),
        resolution: String::new(),
    }]);
    if let Some(b) = app.backlog.first() {
        app.selected_backlog_id = Some(b.id.clone());
        app.content = ContentState::BacklogDetail;
    }
    assert_eq!(app.content, ContentState::BacklogDetail);
}

#[test]
fn back_from_detail_returns_to_list() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.content = ContentState::MilestoneDetail;
    app.go_back();
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn detail_scroll_works() {
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    assert_eq!(app.detail_scroll, 0);

    // render reports the detail content is scrollable (lines past the viewport)
    app.detail_max_scroll.set(5);

    app.move_down();
    assert_eq!(app.detail_scroll, 1);
    app.move_down();
    assert_eq!(app.detail_scroll, 2);
    app.move_up();
    assert_eq!(app.detail_scroll, 1);
}

#[test]
fn detail_scroll_clamps_at_max() {
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    app.detail_max_scroll.set(2);
    app.move_down();
    assert_eq!(app.detail_scroll, 1);
    app.move_down();
    assert_eq!(app.detail_scroll, 2);
    app.move_down(); // at max — cannot scroll past content
    assert_eq!(app.detail_scroll, 2);
}

#[test]
fn detail_scroll_blocked_when_content_fits() {
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    // default max is 0 (render reports content fits the viewport)
    app.move_down();
    assert_eq!(app.detail_scroll, 0);
}
