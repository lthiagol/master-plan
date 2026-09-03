//! M186 AC-01: `/` opens search on Milestones/Backlog/Ideas; no-op elsewhere.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::mode::Mode;

#[test]
fn slash_opens_search_on_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenSearch).unwrap();
    assert!(matches!(app.active_mode, Mode::SearchInput(_)));
}

#[test]
fn slash_is_noop_on_path_overview_watch_settings() {
    let mut app = App::new();
    let r = MpRunner::new().expect("mp");
    for lane in [Lane::Path, Lane::Overview, Lane::Autopilot, Lane::Settings] {
        app.select_lane(lane);
        app.active_mode = Mode::Normal;
        apply_action(&mut app, &r, Action::OpenSearch).unwrap();
        assert_eq!(app.active_mode, Mode::Normal, "lane {lane:?} must no-op");
    }
}

#[test]
fn slash_opens_on_backlog_and_ideas() {
    let mut app = App::new();
    let r = MpRunner::new().expect("mp");
    for lane in [Lane::Backlog, Lane::Ideas] {
        app.select_lane(lane);
        apply_action(&mut app, &r, Action::OpenSearch).unwrap();
        assert!(
            matches!(app.active_mode, Mode::SearchInput(_)),
            "lane {lane:?} should open search"
        );
    }
}
