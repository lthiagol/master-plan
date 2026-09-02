//! M172 / M184: Path lane + backlog-prefix side surfaces.
//!
//! M184 folded Tweaks into Backlog (B-/TW-/BF-/BL-*) and dropped the
//! Grooming tab. Ideas remains ID-* only. Watch stays before Settings.

use raul::tui::app::{App, Lane};
use raul::tui::view_state;

#[test]
fn tui_path_lane_split_lane_ordered_is_seven() {
    let lanes = Lane::ordered();
    // M184: Overview, Milestones, Path, Backlog, Ideas, Watch, Settings.
    assert_eq!(lanes.len(), 7);
    assert_eq!(
        lanes,
        vec![
            Lane::Overview,
            Lane::Milestones,
            Lane::Path,
            Lane::Backlog,
            Lane::Ideas,
            Lane::Watch,
            Lane::Settings,
        ]
    );
}

#[test]
fn tui_path_lane_split_side_lanes_have_distinct_labels() {
    assert_eq!(Lane::Backlog.label(), "Backlog");
    assert_eq!(Lane::Ideas.label(), "Ideas");
    assert_eq!(Lane::Backlog.compact_label(), "Bl");
    assert_eq!(Lane::Ideas.compact_label(), "Id");
}

#[test]
fn tui_side_lanes_visible_backlog_filters_by_prefix() {
    use raul::tui::app::BacklogLine;

    let mut app = App::new();
    let sample = vec![
        BacklogLine {
            id: "B-85".into(),
            title: "canonical backlog".into(),
            priority: "high".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
        BacklogLine {
            id: "TW-01".into(),
            title: "first tweak".into(),
            priority: "high".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
        BacklogLine {
            id: "BF-01".into(),
            title: "bugfix".into(),
            priority: "regular".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
        BacklogLine {
            id: "BL-01".into(),
            title: "bl-alias".into(),
            priority: "low".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
        BacklogLine {
            id: "ID-01".into(),
            title: "idea".into(),
            priority: "normal".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
        BacklogLine {
            id: "XX-01".into(),
            title: "other".into(),
            priority: "low".into(),
            status: "open".into(),
            resolution: String::new(),
            preview: String::new(),
            ..Default::default()
        },
    ];
    app.load_backlog(sample);

    // Backlog = B-* + TW-* + BF-* + BL-* (excludes ID-* and unknown).
    app.select_lane(Lane::Backlog);
    let bl = app.visible_backlog();
    assert_eq!(bl.len(), 4, "Backlog must show B-/TW-/BF-/BL-* only");
    assert!(bl
        .iter()
        .all(|b| raul::tui::app::is_actionable_backlog_id(&b.id)));

    // Ideas → only ID-* rows.
    app.select_lane(Lane::Ideas);
    let id = app.visible_backlog();
    assert_eq!(id.len(), 1, "Ideas lane must show only ID-* items");
    assert!(id.iter().all(|b| b.id.starts_with("ID-")));
}

#[test]
fn tui_side_lanes_view_state_computes_scrollbar_rects() {
    let mut app = App::new();
    for lane in [Lane::Backlog, Lane::Ideas] {
        app.select_lane(lane);
        let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 100, 30));
        let _ = view.scrollbar_rects.len();
    }
}

/// M179 S2: the Watch lane is pinned immediately before Settings.
#[test]
fn m179_watch_lane_is_immediately_before_settings() {
    let lanes = Lane::ordered();
    let watch_idx = lanes
        .iter()
        .position(|l| matches!(l, Lane::Watch))
        .expect("Watch lane must be in Lane::ordered()");
    let settings_idx = lanes
        .iter()
        .position(|l| matches!(l, Lane::Settings))
        .expect("Settings lane must remain in Lane::ordered()");
    assert_eq!(
        watch_idx + 1,
        settings_idx,
        "Watch must be immediately before Settings (M179 tab placement DD); \
         got Watch at {watch_idx} and Settings at {settings_idx}"
    );
    assert_eq!(Lane::Watch.label(), "Watch");
    assert_eq!(Lane::Watch.compact_label(), "Wt");
    assert_eq!(
        Lane::Watch.label(),
        raul::lanes::LANE_WATCH,
        "Watch label must be the LANE_WATCH constant from lanes.rs"
    );
}
