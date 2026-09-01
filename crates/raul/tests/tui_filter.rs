use raul::tui::app::{App, BacklogLine, ContentState, Lane, MilestoneSummary};
use std::collections::BTreeMap;

fn milestones() -> Vec<MilestoneSummary> {
    vec![
        MilestoneSummary {
            id: "01".into(),
            title: "Done one".into(),
            lifecycle: "complete".into(),
            lifecycle_at: Some("2026-07-04T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "02".into(),
            title: "Active".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: Some("2026-07-08T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "03".into(),
            title: "Done two".into(),
            lifecycle: "complete".into(),
            lifecycle_at: Some("2026-07-05T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "04".into(),
            title: "Planned".into(),
            lifecycle: "approved".into(),
            lifecycle_at: Some("2026-07-09T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]
}

#[test]
fn hide_done_filters_done_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(milestones());
    assert_eq!(app.visible_milestones().len(), 4);

    app.toggle_hide_done();
    assert!(app.hide_done);

    let visible = app.visible_milestones();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0].id, "02");
    assert_eq!(visible[1].id, "04");
}

#[test]
fn toggle_resets_selection_to_first() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(milestones());
    app.move_down();
    app.move_down();
    assert_eq!(app.selected_index, 2);

    app.toggle_hide_done();
    assert_eq!(app.selected_index, 0);
}

#[test]
fn enter_detail_targets_visible_item() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(milestones());
    app.toggle_hide_done();

    app.enter_milestone_detail(None);
    assert_eq!(app.selected_milestone_id.as_deref(), Some("02"));
    assert_eq!(app.content, ContentState::MilestoneDetail);
}

#[test]
fn hide_done_filters_resolved_backlog() {
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    app.load_backlog(vec![
        BacklogLine {
            id: "BL-01".into(),
            title: "open item".into(),
            priority: "high".into(),
            status: "open".into(),
            resolution: String::new(),
        },
        BacklogLine {
            id: "BL-02".into(),
            title: "resolved item".into(),
            priority: "low".into(),
            status: "resolved".into(),
            resolution: "shipped".into(),
        },
        BacklogLine {
            id: "BL-03".into(),
            title: "pending item".into(),
            priority: "regular".into(),
            status: "pending".into(),
            resolution: String::new(),
        },
        BacklogLine {
            id: "BL-04".into(),
            title: "archived item".into(),
            priority: "low".into(),
            status: "archived".into(),
            resolution: String::new(),
        },
    ]);
    assert_eq!(app.visible_backlog().len(), 4);

    app.toggle_hide_done();
    let visible = app.visible_backlog();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0].id, "BL-01");
    assert_eq!(visible[1].id, "BL-03");
    assert!(!visible.iter().any(|b| b.id == "BL-02" || b.id == "BL-04"));
}
