//! M186 AC-08: `o` cycles sort key on Milestones/Backlog/Ideas.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane, SortKey};

#[test]
fn o_cycles_id_title_priority_lifecycle_updated_id_on_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().expect("mp");

    // M187: cycle now follows column order — Id → Title → Priority →
    // Lifecycle → Updated → Id.
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Id);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Title);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Priority);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Lifecycle);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Updated);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Id);
}

#[test]
fn o_is_noop_on_overview_path_watch_settings() {
    let mut app = App::new();
    let r = MpRunner::new().expect("mp");
    for lane in [Lane::Overview, Lane::Path, Lane::Watch, Lane::Settings] {
        app.select_lane(lane);
        apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
        // sort key on these lanes is always Id (no sort menu).
        assert_eq!(app.lane_sort_key(lane), SortKey::Id);
    }
}
