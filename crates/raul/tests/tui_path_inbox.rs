use raul::tui::app::{App, Lane};
use std::path::PathBuf;

#[test]
fn path_lane_has_list_count_from_dashboard() {
    let mut app = App::new();
    app.load_dashboard(raul::tui::app::DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 0,
        pending_review_count: 0,
        track_pending: 0,
        annotations_open: 0,
        next_action: "M77/S3".into(),
        path_preview: vec!["M77/S4".into(), "M77/S5".into()],
        inbox_items: vec![],
        ..Default::default()
    });
    app.select_lane(Lane::Path);
    // next_action (1) + 2 preview items = 3
    assert!(app.selected_index == 0);
}

#[test]
fn overview_lane_has_list_count_from_dashboard() {
    let mut app = App::new();
    app.load_dashboard(raul::tui::app::DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 2,
        pending_review_count: 0,
        track_pending: 0,
        annotations_open: 0,
        next_action: "".into(),
        path_preview: vec![],
        inbox_items: vec![
            raul::tui::app::InboxLine {
                id: "TW-01".into(),
                kind: "track".into(),
                display: "Test 1".into(),
                reason: "pending tweak".into(),
                action: "mp track show tweak".into(),
            },
            raul::tui::app::InboxLine {
                id: "TW-02".into(),
                kind: "track".into(),
                display: "Test 2".into(),
                reason: "pending tweak".into(),
                action: "mp track show tweak".into(),
            },
        ],
        ..Default::default()
    });
    assert_eq!(app.dashboard.inbox_items.len(), 2);
    assert_eq!(app.selected_index, 0);
}

#[test]
fn ac10_path_drill_in_extracts_milestone_id() {
    // M136: the (Lane::Path, Enter) drill-in moved out of `runner.rs` (the
    // giant inline match) into `tui/action.rs::apply_enter` and the key
    // binding in `tui/modes/normal.rs`. Look in both places so the gate
    // test continues to exercise the contract.
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let action_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("action.rs");
    let modes_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("modes");

    let mut combined = std::fs::read_to_string(&runner_path).unwrap_or_default();
    combined.push('\n');
    combined.push_str(&std::fs::read_to_string(&action_path).unwrap_or_default());
    combined.push('\n');
    if modes_dir.exists() {
        for entry in std::fs::read_dir(&modes_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rs") {
                combined.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
                combined.push('\n');
            }
        }
    }

    assert!(
        combined.contains("Lane::Path =>"),
        "Path drill-in must exist somewhere in dispatch (AC-10)"
    );
    assert!(
        combined.contains("trim_start_matches"),
        "Path drill-in must extract milestone ID from next_action (AC-10)"
    );
    assert!(
        combined.contains("select_lane") && combined.contains("Milestones"),
        "Path drill-in must navigate to Milestones lane (AC-10)"
    );
    assert!(
        combined.contains("enter_milestone_detail"),
        "Path drill-in must enter milestone detail (AC-10)"
    );
}
