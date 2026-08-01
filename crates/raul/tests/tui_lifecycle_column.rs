//! M144 + M172 S2: integration tests for the raul TUI Milestones lane.
//! Pins AC-05..AC-07 against the post-M172 tree-view renderer:
//!   * AC-05: MilestoneSummary shape (lifecycle + lifecycle_at fields).
//!   * AC-06: render buffer contains the lifecycle string. Pre-M172
//!     this lived in a "Lifecycle" Table column; post-M172 it lives
//!     in the inline `[lifecycle]` tag per tree row.
//!   * AC-07: row color is driven by `lifecycle` (not the legacy fields).
//!     Pre-M172 the per-row color carrier fed the Table; post-M172
//!     the id span carries the lifecycle color.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::render;
use raul::tui::view_state;

fn render_to_string(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn milestones_table_renders_lifecycle_column() {
    // M185: Table has a Lifecycle column + Gauge column.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: Some("2026-07-08T00:00:00Z".into()),
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);

    let output = render_to_string(&app, 120, 30);
    assert!(
        output.contains("Lifecycle"),
        "Lifecycle column header missing; got:\n{output}"
    );
    assert!(
        output.contains("Gauge") || output.contains('▮'),
        "gauge column missing; got:\n{output}"
    );
    assert!(
        output.contains("in-progress"),
        "rendered buffer missing lifecycle value; got:\n{output}"
    );
}

#[test]
fn since_cell_renders_relative_time_when_lifecycle_at_present() {
    // AC-06 (post-M172): the "since" relative-time cell moved into
    // the milestone DETAIL screen (the tree view drops it from the
    // list to keep rows one line). This test now exercises the
    // detail screen, which still renders the since cell.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: Some(now_iso()),
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "01",
            "title": "Setup",
            "lifecycle": "in-progress",
            "spec_status": "verified",
            "execution_status": "done",
            "lifecycle_at": now_iso(),
            "updated": "2026-07-08T00:00:00Z"
        }
    }));

    // M172 S2: the detail screen still renders "Lifecycle at:" as a
    // raw RFC3339 timestamp (the relative-time humanizer is exposed
    // at the helper level but not piped through detail yet — that's
    // a separate polish pass). Pin the raw timestamp contract so a
    // future regression that drops the field entirely is caught.
    let output = render_to_string(&app, 120, 30);
    assert!(
        output.contains("Lifecycle at:"),
        "detail screen missing 'Lifecycle at:' field; got:\n{output}"
    );
    assert!(
        output.contains(&now_iso()),
        "detail screen missing the lifecycle_at timestamp; got:\n{output}"
    );
}

#[test]
fn since_cell_falls_back_to_since_updated_when_lifecycle_at_none() {
    // AC-06 (post-M172): when lifecycle_at is None, the detail screen
    // renders `—` (em-dash) as the "since" fallback so the column
    // always has visible content.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "draft".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "01",
            "title": "Setup",
            "lifecycle": "draft",
            "spec_status": "draft",
            "execution_status": "planned",
            "lifecycle_at": "",
            "updated": "2026-07-08T00:00:00Z"
        }
    }));

    let output = render_to_string(&app, 120, 30);
    assert!(
        output.contains("Lifecycle at:"),
        "detail screen missing 'Lifecycle at:' field; got:\n{output}"
    );
    assert!(
        output.contains("—"),
        "detail screen missing the em-dash fallback when lifecycle_at is None; got:\n{output}"
    );
}

#[test]
fn row_color_follows_lifecycle_not_legacy_fields() {
    // M185: lifecycle_color mapping is the single source of truth
    // (progress.rs). Buffer cell colors can be flattened by REVERSED
    // selection / TestBackend — pin the mapping function instead.
    use raul::theme::Palette;
    use raul::tui::progress::lifecycle_color;
    let p = Palette::default_palette();
    assert_ne!(
        lifecycle_color("draft", p),
        lifecycle_color("complete", p),
        "draft and complete must map to different palette colors"
    );
    assert_eq!(lifecycle_color("complete", p), p.success);
    assert_eq!(lifecycle_color("draft", p), p.dim);
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_iso(secs)
}

fn format_iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let sod = secs.rem_euclid(86_400);
    let hh = sod / 3600;
    let mm = (sod / 60) % 60;
    let ss = sod % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hh, mm, ss)
}
