//! M184 AC-02 / F-01: Lane::Backlog visible_backlog() is B-/TW-/BF-/BL-*.

use raul::tui::app::{is_actionable_backlog_id, App, BacklogLine, Lane};

fn sample() -> Vec<BacklogLine> {
    vec![
        bl("B-85", "formal backlog"),
        bl("TW-01", "tweak"),
        bl("BF-02", "bugfix"),
        bl("BL-03", "bl-alias"),
        bl("ID-04", "idea"),
        bl("XX-05", "other"),
    ]
}

fn bl(id: &str, title: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: title.into(),
        priority: "normal".into(),
        status: "open".into(),
        resolution: String::new(),
        preview: String::new(),
    }
}

#[test]
fn backlog_lane_keeps_b_tw_bf_bl_excludes_id() {
    let mut app = App::new();
    app.load_backlog(sample());
    app.select_lane(Lane::Backlog);
    // M187: backlog now applies the active sort key (default Id),
    // so the rows come back in numeric-id order — TW-01 (1), BF-02 (2),
    // BL-03 (3), B-85 (85) — not the insertion order the pre-fix test
    // pinned.
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, vec!["TW-01", "BF-02", "BL-03", "B-85"]);
}

#[test]
fn m184_f01_canonical_b_prefix_is_actionable() {
    // mp::store::next_backlog_id assigns B-; BL- alone would hide them.
    assert!(is_actionable_backlog_id("B-01"));
    assert!(is_actionable_backlog_id("B-85"));
    assert!(is_actionable_backlog_id("BL-01"));
    assert!(is_actionable_backlog_id("TW-01"));
    assert!(is_actionable_backlog_id("BF-01"));
    assert!(!is_actionable_backlog_id("ID-01"));
    assert!(!is_actionable_backlog_id("XX-01"));
    // BF- must not be swallowed by a mistaken bare "B" prefix check.
    assert!(is_actionable_backlog_id("BF-99"));
    assert!(!is_actionable_backlog_id("B"));
}
