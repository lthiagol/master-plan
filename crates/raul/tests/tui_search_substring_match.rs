//! M186 AC-02: substring match (case-insensitive) on id + title.

use raul::tui::app::{App, Lane, MilestoneSummary};

fn ms(id: &str, title: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: title.into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
    }
}

#[test]
fn substring_match_narrows_by_id_or_title_case_insensitive() {
    let mut app = App::new();
    app.load_milestones(vec![
        ms("01", "Setup logging"),
        ms("02", "Database migration"),
        ms("03", "LOG viewer"),
        ms("04", "unrelated"),
    ]);
    app.select_lane(Lane::Milestones);

    app.lane_search.insert(Lane::Milestones, "log".to_string());
    let ids: Vec<_> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(ids, vec!["01", "03"]);
}

#[test]
fn empty_term_shows_full_list() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a"), ms("02", "b")]);
    app.select_lane(Lane::Milestones);
    assert_eq!(app.visible_milestones().len(), 2);
}
