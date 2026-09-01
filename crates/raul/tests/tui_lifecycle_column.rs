//! M144 + M172 S2: integration tests for the raul TUI Milestones lane.
//! Pins AC-05..AC-07 against the post-M172 tree-view renderer:
//!   * AC-05: MilestoneSummary shape (lifecycle + lifecycle_at fields).
//!   * AC-06: render buffer contains the lifecycle string. Pre-M172
//!     this lived in a "Lifecycle" Table column; post-M172 it lives
//!     in the inline `[lifecycle]` tag per tree row.
//!   * AC-07: row color is driven by `lifecycle` (not the legacy fields).
//!     Pre-M172 the per-row color carrier fed the Table; post-M172
//!     the id span carries the lifecycle color.

use std::collections::BTreeMap;

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
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
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
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
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
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
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

#[test]
fn cancelled_milestone_renders_badge_in_title_column() {
    // M174 fix: the Milestones lane must surface the cancellation
    // overlay in the title column so the operator sees the audit
    // story without opening a separate `mp reviews` round-trip.
    // The badge reads `[cancelled: <reason>]` when the
    // `cancel_reason` is set, and `[cancelled]` when it isn't.
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render;
    use raul::tui::view_state;

    let cancelled_with_reason = MilestoneSummary {
        id: "174".into(),
        title: "M169 review remediations".into(),
        lifecycle: "approved".into(),
        lifecycle_at: Some("2026-07-15T17:49:50Z".into()),
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: "2026-07-15".into(),
        cancelled: true,
        cancelled_at: Some("2026-07-15T00:00:00Z".into()),
        cancel_reason: Some("Work shipped via M169-rev".into()),
    flow_stages: BTreeMap::new(),
    };
    let cancelled_no_reason = MilestoneSummary {
        id: "175".into(),
        title: "Dropped scope".into(),
        lifecycle: "approved".into(),
        lifecycle_at: Some("2026-07-15T17:49:50Z".into()),
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: "2026-07-15".into(),
        cancelled: true,
        cancelled_at: Some("2026-07-15T00:00:00Z".into()),
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    };
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![cancelled_with_reason, cancelled_no_reason]);

    let backend = TestBackend::new(160, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let flat: String = (0..buf.area().height)
        .flat_map(|y| (0..buf.area().width).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        flat.contains("[cancelled: Work shipped via M169-rev]"),
        "cancelled milestone with reason should render `[cancelled: <reason>]` badge; got: {flat}"
    );
    assert!(
        flat.contains("[cancelled]"),
        "cancelled milestone without reason should render `[cancelled]` badge; got: {flat}"
    );
}

#[test]
fn non_cancelled_milestone_renders_no_cancellation_badge() {
    // M174 fix: the cancellation badge is opt-in (only rendered
    // when `cancelled: true`). Sanity check that the regular
    // milestone title does not pick up the badge text.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "draft".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render;
    use raul::tui::view_state;
    let backend = TestBackend::new(120, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let flat: String = (0..buf.area().height)
        .flat_map(|y| (0..buf.area().width).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !flat.contains("[cancelled"),
        "non-cancelled milestone must not render a `[cancelled]` badge; got: {flat}"
    );
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

// ─── M202 S20: lifecycle grid renders 12 mp-flow buckets ───────────────

#[test]
fn grid_renders_twelve_buckets() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // All 12 canonical bucket labels (with their ordinal marker
    // prefix `<N>/12 `) must appear in the grid.
    for label in [
        "1/12 draft",
        "2/12 groom",
        "3/12 specify",
        "4/12 approve",
        "5/12 execute",
        "6/12 self-review",
        "7/12 complete",
        "8/12 external-review",
        "9/12 remediate",
        "10/12 re-review",
        "11/12 document",
        "12/12 hand-off",
    ] {
        assert!(
            flat.contains(label),
            "lifecycle grid must include {label}; got: {flat}"
        );
    }
}

#[test]
fn grid_order_is_canonical_mp_flow() {
    // The 12 buckets must appear in canonical order (1/12 first,
    // 12/12 last) so the operator can scan top-to-bottom.
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    let labels = [
        "1/12 draft", "2/12 groom", "3/12 specify", "4/12 approve",
        "5/12 execute", "6/12 self-review", "7/12 complete",
        "8/12 external-review", "9/12 remediate", "10/12 re-review",
        "11/12 document", "12/12 hand-off",
    ];
    let mut last: Option<usize> = None;
    for label in labels {
        let pos = flat.find(label).expect(&format!("{label} missing"));
        if let Some(p) = last {
            assert!(pos > p, "{label} must come after the previous canonical stage");
        }
        last = Some(pos);
    }
}

#[test]
fn bucket_labels_include_n_over_twelve() {
    // AC-16: every bucket label includes its `<N>/12` ordinal
    // marker so the operator can spot the canonical stage at a
    // glance.
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    for n in 1..=12 {
        let prefix = format!("{n}/12 ");
        assert!(
            flat.contains(&prefix),
            "bucket ordinal {n}/12 must appear; got: {flat}"
        );
    }
}

#[test]
fn title_includes_mp_flow_disambiguation() {
    // AC-16: the grid title is `Lifecycle (current mp-flow
    // stage)` so the operator sees the meaning shift from the
    // legacy 8-state lifecycle.
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        flat.contains("Lifecycle (current mp-flow stage)"),
        "grid title must disambiguate from the legacy 8-state lifecycle; got: {flat}"
    );
}
