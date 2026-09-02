//! M187 regressions: backlog sort + column-header indicator.

use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use ratatui::Terminal;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, BacklogLine, Lane, SortKey};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn bl(id: &str, title: &str, priority: &str, status: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: title.into(),
        priority: priority.into(),
        status: status.into(),
        resolution: String::new(),
        preview: String::new(),
        ..Default::default()
    }
}

/// M187: backlog cycle-sort lands within the per-lane key set in
/// column order. M205: extended to 6 stops (Id → Title → Priority →
/// Status → Created → ResolvedAt → Id) per the new M205 cycle.
#[test]
fn backlog_cycle_sort_uses_per_lane_key_set() {
    let mut app = App::new();
    app.load_backlog(vec![
        bl("TW-01", "a", "normal", "open"),
        bl("TW-02", "b", "high", "open"),
    ]);
    app.select_lane(Lane::Backlog);
    let r = raul::mp_runner::MpRunner::new().unwrap();

    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Id);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Title);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Priority);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Status);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Created);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::ResolvedAt);
    apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Id);
}

/// M187: visible_backlog now applies the active sort key. Sort by
/// Priority should reorder the rows (high before normal before low).
#[test]
fn backlog_visible_applies_priority_sort() {
    let mut app = App::new();
    app.load_backlog(vec![
        bl("TW-01", "low", "low", "open"),
        bl("TW-02", "high", "high", "open"),
        bl("TW-03", "normal", "normal", "open"),
    ]);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Priority);

    let titles: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.title.as_str())
        .collect();
    assert_eq!(titles, vec!["high", "normal", "low"]);
}

/// M187: visible_backlog sort by Status — open/pending above resolved.
#[test]
fn backlog_visible_applies_status_sort() {
    let mut app = App::new();
    app.load_backlog(vec![
        bl("TW-01", "resolved", "normal", "resolved"),
        bl("TW-02", "open", "normal", "open"),
        bl("TW-03", "pending", "normal", "pending"),
    ]);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Status);

    let titles: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.title.as_str())
        .collect();
    assert_eq!(titles, vec!["open", "pending", "resolved"]);
}

/// M187: visible_backlog default sort (Id) reorders by numeric id,
/// not insertion order.
#[test]
fn backlog_default_sort_is_numeric_id() {
    let mut app = App::new();
    app.load_backlog(vec![
        bl("TW-10", "ten", "normal", "open"),
        bl("TW-02", "two", "normal", "open"),
        bl("TW-01", "one", "normal", "open"),
    ]);
    app.select_lane(Lane::Backlog);
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, vec!["TW-01", "TW-02", "TW-10"]);
}

/// M187: column header carries a `▼` marker on the active sort column.
/// M205: the Milestones table's Stage column IS now a sortable
/// column (it carries position via flow_stages, and the Stage sort
/// reuses that signal). This test pins the marker on the Since
/// header (a sortable column) and the absence on the ID column
/// (inactive for this test). The Stage column assertion is left
/// out of the negative half — it's a sortable column now, so when
/// Stage is the active sort key the arrow SHOULD render.
#[test]
fn milestones_header_marks_active_sort_column_with_arrow() {
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "x".into(),
        lifecycle: "draft".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.select_lane(Lane::Milestones);
    app.lane_sort_key.insert(Lane::Milestones, SortKey::Updated);

    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    // The header row sits a couple of rows below the tab bar. Walk the
    // buffer and confirm the Since column header carries a ▼ while
    // the ID column header does not (Stage is now sortable but
    // inactive for this test, so it shouldn't render ▼).
    let mut since_marked = false;
    let mut id_marked = false;
    let mut stage_marked = false;
    for y in 0..buf.area().height {
        let mut row = String::new();
        let mut row_has_reversed_or_bold = false;
        for x in 0..buf.area().width {
            let cell = &buf[(x, y)];
            row.push_str(cell.symbol());
            if cell.modifier.contains(Modifier::BOLD) {
                row_has_reversed_or_bold = true;
            }
        }
        if row_has_reversed_or_bold && row.contains("Since") {
            // Find the "Since ▼" run on this row.
            if row.contains("Since ▼") || row.contains("Since▼") {
                since_marked = true;
            }
        }
        if row_has_reversed_or_bold && row.contains("ID ▼") {
            id_marked = true;
        }
        if row_has_reversed_or_bold && row.contains("Stage ▼") {
            stage_marked = true;
        }
    }
    assert!(
        since_marked,
        "Since header must carry ▼ when it's the active sort"
    );
    assert!(
        !id_marked,
        "ID header must NOT carry ▼ when Since is the active sort"
    );
    assert!(
        !stage_marked,
        "Stage header (sortable but inactive) must NOT carry ▼"
    );
}

/// M187: backlog header also carries the arrow on the active column.
#[test]
fn backlog_header_marks_active_sort_column_with_arrow() {
    let mut app = App::new();
    app.load_backlog(vec![bl("TW-01", "a", "normal", "open")]);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Status);

    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut status_marked = false;
    for y in 0..buf.area().height {
        let mut row = String::new();
        for x in 0..buf.area().width {
            row.push_str(buf[(x, y)].symbol());
        }
        if row.contains("Status ▼") || row.contains("Status▼") {
            status_marked = true;
            break;
        }
    }
    assert!(
        status_marked,
        "Status header must carry ▼ when it's the active sort"
    );
}
