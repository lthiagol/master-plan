//! M186 AC-03: Enter commits search term.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::mode::Mode;

fn ms(id: &str, title: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: title.into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    }
}

#[test]
fn enter_commits_search_term() {
    let mut app = App::new();
    app.load_milestones(vec![
        ms("01", "login"),
        ms("02", "logout"),
        ms("03", "other"),
    ]);
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenSearch).unwrap();
    apply_action(&mut app, &r, Action::SearchInputChar('l')).unwrap();
    apply_action(&mut app, &r, Action::SearchInputChar('o')).unwrap();
    apply_action(&mut app, &r, Action::SearchInputCommit).unwrap();

    assert_eq!(app.active_mode, Mode::Normal);
    assert_eq!(app.lane_search_term(), "lo");
    let ids: Vec<_> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(ids, vec!["01", "02"]);
}
