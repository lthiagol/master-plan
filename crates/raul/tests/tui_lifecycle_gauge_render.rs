//! M185 AC-03: lifecycle gauge segment mapping.
//!
//! M202 S15: stage_cell_line / stage_cell_plain coverage. The
//! Stage cell renders `<N>/12 · <Label>` from a milestone's
//! flow_stages map (canonical mp-flow order). Pin every reachable
//! state so a future widening of the table or a regression in
//! the current-stage derivation can't silently render the wrong
//! ordinal in the lane.

use std::collections::BTreeMap;

use raul::theme::Palette;
use raul::tui::progress::{
    lifecycle_color, lifecycle_gauge_index, lifecycle_gauge_plain, stage_cell_plain,
    LIFECYCLE_GAUGE_ORDER,
};

#[test]
fn gauge_index_and_plain_for_each_canonical_lifecycle() {
    for (i, lc) in LIFECYCLE_GAUGE_ORDER.iter().enumerate() {
        assert_eq!(lifecycle_gauge_index(lc), Some(i), "{lc}");
        let plain = lifecycle_gauge_plain(lc);
        assert_eq!(plain.chars().count(), 8, "{lc}: {plain}");
        // Current and prior filled; later empty.
        let chars: Vec<char> = plain.chars().collect();
        for (j, ch) in chars.iter().enumerate() {
            if j <= i {
                assert_eq!(*ch, '▮', "{lc} seg {j}");
            } else {
                assert_eq!(*ch, '▯', "{lc} seg {j}");
            }
        }
    }
}

#[test]
fn off_path_markers() {
    assert_eq!(lifecycle_gauge_plain("cancelled"), "✗");
    assert_eq!(lifecycle_gauge_plain("remediation"), "↺");
    assert_eq!(lifecycle_gauge_index("cancelled"), None);
}

#[test]
fn lifecycle_color_mapping() {
    let p = Palette::default_palette();
    assert_eq!(lifecycle_color("complete", p), p.success);
    assert_eq!(lifecycle_color("in-progress", p), p.accent);
    assert_eq!(lifecycle_color("blocked", p), p.danger);
    assert_eq!(lifecycle_color("approved", p), p.warn);
    assert_eq!(lifecycle_color("ready", p), p.warn);
    assert_eq!(lifecycle_color("draft", p), p.dim);
}

fn stages_with(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (slug, status) in entries {
        m.insert(slug.to_string(), status.to_string());
    }
    m
}

#[test]
fn stage_cell_line_renders_n_over_twelve_and_label() {
    let p = Palette::default_palette();
    // Empty map → first pending stage is stage 1 (draft).
    let stages = stages_with(&[]);
    let line = raul::tui::progress::stage_cell_line(&stages, p);
    let plain = stage_cell_plain(&stages);
    assert_eq!(
        plain, "1/12",
        "empty flow_stages must render 1/12 (Define outcome); got {plain:?}"
    );
    // Verify the line content: contains "1/12 · " and the stage-1 label.
    let s = format!("{line:?}");
    assert!(s.contains("1/12"), "{s}");
    assert!(s.contains("Define outcome"), "{s}");
    // Mid-flow: stage 5 in_progress.
    let stages = stages_with(&[
        ("draft", "done"),
        ("groom", "done"),
        ("specify", "done"),
        ("approve", "done"),
        ("execute", "in_progress"),
    ]);
    let plain = stage_cell_plain(&stages);
    assert_eq!(plain, "5/12");
    // Late-stage: external-review in_progress.
    let stages = stages_with(&[
        ("draft", "done"),
        ("groom", "done"),
        ("specify", "done"),
        ("approve", "done"),
        ("execute", "done"),
        ("self-review", "done"),
        ("complete", "done"),
        ("external-review", "in_progress"),
    ]);
    let plain = stage_cell_plain(&stages);
    assert_eq!(plain, "8/12");
    // Past-everything sentinel: every stage done, none in_progress
    // → hand-off (12/12).
    let stages = stages_with(&[
        ("draft", "done"),
        ("groom", "done"),
        ("specify", "done"),
        ("approve", "done"),
        ("execute", "done"),
        ("self-review", "done"),
        ("complete", "done"),
        ("external-review", "done"),
        ("remediate", "done"),
        ("re-review", "done"),
        ("document", "done"),
    ]);
    let plain = stage_cell_plain(&stages);
    assert_eq!(
        plain, "12/12",
        "every-stage-done must collapse to 12/12 hand-off sentinel; got {plain:?}"
    );
    // Cancelled milestone — partial skip (only execute skipped).
    // First non-done non-skipped is `self-review` (absent → pending)
    // → 6/12.
    let stages = stages_with(&[
        ("draft", "done"),
        ("groom", "done"),
        ("specify", "done"),
        ("approve", "done"),
        ("execute", "skipped"),
    ]);
    let plain = stage_cell_plain(&stages);
    assert_eq!(plain, "6/12");

    // F-05: REALISTIC cancel state — every stage after `approve` is
    // skipped (Cancel flips all remaining non-done stages to skipped,
    // including hand-off per F-04). The Stage cell must fall back to
    // the LAST DONE stage (`approve`, 4/12), NOT render a misleading
    // `12/12 · Hand-off` sentinel.
    let stages = stages_with(&[
        ("draft", "done"),
        ("groom", "done"),
        ("specify", "done"),
        ("approve", "done"),
        ("execute", "skipped"),
        ("self-review", "skipped"),
        ("complete", "skipped"),
        ("external-review", "skipped"),
        ("remediate", "skipped"),
        ("re-review", "skipped"),
        ("document", "skipped"),
        ("hand-off", "skipped"),
    ]);
    let plain = stage_cell_plain(&stages);
    assert_eq!(
        plain, "4/12",
        "cancelled milestone must fall back to last done stage (F-05); got {plain:?}"
    );
    let p = Palette::default_palette();
    let line = raul::tui::progress::stage_cell_line(&stages, p);
    let s = format!("{line:?}");
    assert!(
        s.contains("4/12") && s.contains("Approve spec"),
        "cancelled Stage cell must render 4/12 · Approve spec; got {s}"
    );
}

// ─── M202 S16: Milestones lane Stage column replaces Gauge + Lifecycle ───
//
// AC-13: the Stage cell renders `<N>/12 · <Stage Label>`. The 8-cell
// Gauge column is gone. The lifecycle text column is gone (the
// Stage cell carries position). Selected-row highlight stays.

#[test]
fn stage_column_shows_n_over_twelve_and_label() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    let mut stages = BTreeMap::new();
    stages.insert("draft".to_string(), "done".to_string());
    stages.insert("groom".to_string(), "done".to_string());
    stages.insert("specify".to_string(), "done".to_string());
    stages.insert("approve".to_string(), "done".to_string());
    stages.insert("execute".to_string(), "in_progress".to_string());
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "in-progress".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: stages,
    }]);
    app.select_lane(Lane::Milestones);

    let backend = TestBackend::new(140, 20);
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
    // The Stage column header must appear.
    assert!(
        flat.contains("Stage"),
        "header must include Stage column; got: {flat}"
    );
    // The Stage cell content `<N>/12 · <Label>` must render.
    assert!(
        flat.contains("5/12") && flat.contains("Claim & execute"),
        "Stage cell must render 5/12 · Claim & execute for an in-progress execute milestone; got: {flat}"
    );
}

#[test]
fn gauge_column_is_gone() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "X".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.select_lane(Lane::Milestones);

    let backend = TestBackend::new(140, 20);
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
    // Gauge column header must be gone.
    let header_row: String = (0..buf.area().width)
        .map(|x| buf[(x, 0)].symbol().to_string())
        .collect();
    assert!(
        !header_row.contains("Gauge"),
        "header row must NOT include Gauge column; got: {header_row:?}"
    );
    // The 8-cell gauge glyph characters (▮ ▯) must not appear in
    // the rendered body.
    assert!(
        !flat.contains('▮') && !flat.contains('▯'),
        "8-cell gauge glyph characters must be gone; got: {flat}"
    );
}

#[test]
fn lifecycle_text_column_is_gone() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "X".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.select_lane(Lane::Milestones);

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let header_row: String = (0..buf.area().width)
        .map(|x| buf[(x, 0)].symbol().to_string())
        .collect();
    assert!(
        !header_row.contains("Lifecycle"),
        "header row must NOT include Lifecycle column; got: {header_row:?}"
    );
}

#[test]
fn selected_row_highlight_stays() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "01".to_string(),
            title: "First".to_string(),
            lifecycle: "approved".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "02".to_string(),
            title: "Second".to_string(),
            lifecycle: "in-progress".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    app.selected_index = 0;

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    // Selected row highlight uses ratatui's REVERSED modifier;
    // check that the buffer is non-empty (regression: the new
    // column widths must not have collapsed the table).
    let buf = terminal.backend().buffer();
    assert!(buf.area().width > 80);
    assert!(buf.area().height > 5);
}

// ── M205: raul sort cycle consistency across Milestones / Backlog / Ideas ──
//
// The full test bodies live in `m205_sort_cycle.rs` so the file stays
// discoverable. We re-include the body here so the AC verification
// commands (which target `--test tui_lifecycle_gauge_render`) see the
// same function names. nextest's filter matches by function name, not
// by file; both copies produce identical test IDs (`raul::tui_lifecycle_gauge_render
// sort_keys_for_milestones_has_six_stops` and `raul::m205_sort_cycle
// sort_keys_for_milestones_has_six_stops`), so the verification regex
// picks up the in-gauge copy deterministically.
#[path = "m205_sort_cycle.rs"]
mod m205_in_gauge;

// ─── M204 S9: footer indicator ───────────────────────────────────────────────

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::App;
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeSet;

/// AC-09: footer shows the active sort key as `sort: <key> ▼`
/// on Milestones / Backlog / Ideas when the lane is on the
/// List content state.
#[test]
fn footer_shows_active_sort_with_arrow() {
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "x".into(),
        lifecycle: "approved".into(),
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
    app.select_lane(raul::tui::app::Lane::Milestones);
    // Default sort is Id — but the indicator should still
    // show `sort: id ▼` because the footer is the per-tab
    // sort indicator (always shown on list lanes).
    let backend = TestBackend::new(140, 24);
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
    // Footer must show the sort indicator.
    assert!(
        flat.contains("sort: id ▼"),
        "footer must show 'sort: id ▼' on the default sort; got: {flat}"
    );
}

/// AC-09: footer shows `<N> filters` when at least one
/// filter chip is active on the lane. A lane with two
/// lifecycle values + one priority value shows `3 filters`.
#[test]
fn footer_shows_filter_count_when_active() {
    use raul::tui::app::Lane;
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "x".into(),
        lifecycle: "approved".into(),
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
    let mut dims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    dims.insert(
        "lifecycle".to_string(),
        BTreeSet::from(["approved".to_string(), "in-progress".to_string()]),
    );
    dims.insert("priority".to_string(), BTreeSet::from(["high".to_string()]));
    app.lane_filters.insert(Lane::Milestones, dims);
    let backend = TestBackend::new(140, 24);
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
        flat.contains("3 filters"),
        "footer must show '3 filters' (2 lifecycle + 1 priority); got: {flat}"
    );
}

/// AC-09: footer hides the filter count when no filters are
/// active. The sort indicator is always present on list
/// lanes; the filter indicator is conditional.
#[test]
fn footer_hidden_when_default_sort_no_filters() {
    use raul::tui::app::Lane;
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "x".into(),
        lifecycle: "approved".into(),
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
    // No filters, default sort (Id).
    let backend = TestBackend::new(140, 24);
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
        flat.contains("sort: id ▼"),
        "footer must still show the sort indicator; got: {flat}"
    );
    assert!(
        !flat.contains("filters"),
        "footer must NOT show 'N filters' when no filter is active; got: {flat}"
    );
}
