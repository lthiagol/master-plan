//! M186 AC-04: Esc cancels — prior term restored.

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
fn esc_cancels_and_restores_prior_term() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a"), ms("02", "b"), ms("03", "c")]);
    app.select_lane(Lane::Milestones);
    // Prior term: "a"
    app.lane_search.insert(Lane::Milestones, "a".to_string());

    let r = MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenSearch).unwrap();
    apply_action(&mut app, &r, Action::SearchInputChar('x')).unwrap();
    apply_action(&mut app, &r, Action::SearchInputCancel).unwrap();

    assert_eq!(app.active_mode, Mode::Normal);
    assert_eq!(app.lane_search_term(), "a");
    let ids: Vec<_> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(ids, vec!["01"]);
}

#[test]
fn esc_with_no_prior_term_clears() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a")]);
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenSearch).unwrap();
    apply_action(&mut app, &r, Action::SearchInputChar('x')).unwrap();
    apply_action(&mut app, &r, Action::SearchInputCancel).unwrap();
    assert_eq!(app.lane_search_term(), "");
    assert_eq!(app.visible_milestones().len(), 1);
}
