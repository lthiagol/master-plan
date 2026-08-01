//! M185 AC-07: filter survives lane switches.

use raul::tui::app::{App, Lane};

#[test]
fn filter_persists_across_lane_round_trip() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.milestone_filter.insert("approved".into());
    app.milestone_filter.insert("in-progress".into());
    app.select_lane(Lane::Backlog);
    app.select_lane(Lane::Milestones);
    assert!(app.milestone_filter.contains("approved"));
    assert!(app.milestone_filter.contains("in-progress"));
    assert_eq!(app.milestone_filter.len(), 2);
}
