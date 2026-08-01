//! M186 AC-05: per-lane search state persists across lane switches.

use raul::tui::app::{App, Lane};

#[test]
fn per_lane_search_state_round_trip() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.lane_search
        .insert(Lane::Milestones, "login".to_string());
    app.lane_search.insert(Lane::Backlog, "tweak".to_string());

    app.select_lane(Lane::Backlog);
    assert_eq!(app.lane_search_term(), "tweak");

    app.select_lane(Lane::Milestones);
    assert_eq!(app.lane_search_term(), "login");

    app.select_lane(Lane::Ideas);
    assert_eq!(app.lane_search_term(), "");
}
