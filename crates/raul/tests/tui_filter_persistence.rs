//! M185 AC-07: filter survives lane switches.
//! M204: filter is now a per-lane field on `App::lane_filters`.

use raul::tui::app::{App, Lane};

#[test]
fn filter_persists_across_lane_round_trip() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.set_lifecycle_filter(
        ["approved".to_string(), "in-progress".to_string()]
            .into_iter()
            .collect(),
    );
    app.select_lane(Lane::Backlog);
    app.select_lane(Lane::Milestones);
    let lf = app.lifecycle_filter_set();
    assert!(lf.contains("approved"));
    assert!(lf.contains("in-progress"));
    assert_eq!(lf.len(), 2);
}
