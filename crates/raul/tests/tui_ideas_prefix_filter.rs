//! M184 AC-03: Lane::Ideas visible_backlog() is ID-* only.

use raul::tui::app::{App, BacklogLine, Lane};

#[test]
fn ideas_lane_keeps_id_only() {
    let mut app = App::new();
    app.load_backlog(vec![
        BacklogLine {
            id: "TW-01".into(),
            title: "tweak".into(),
            priority: "normal".into(),
            status: "open".into(),
            resolution: String::new(),
        },
        BacklogLine {
            id: "ID-01".into(),
            title: "idea one".into(),
            priority: "normal".into(),
            status: "open".into(),
            resolution: String::new(),
        },
        BacklogLine {
            id: "ID-02".into(),
            title: "idea two".into(),
            priority: "normal".into(),
            status: "open".into(),
            resolution: String::new(),
        },
        BacklogLine {
            id: "BL-01".into(),
            title: "backlog".into(),
            priority: "normal".into(),
            status: "open".into(),
            resolution: String::new(),
        },
    ]);
    app.select_lane(Lane::Ideas);
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, vec!["ID-01", "ID-02"]);
}
