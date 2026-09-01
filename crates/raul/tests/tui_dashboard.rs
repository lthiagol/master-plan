//! Dashboard state wiring tests (M71).

use raul::reads;
use raul::tui::app::{App, ContentState, DashboardSnapshot, InboxLine, Lane};
use raul::tui::dashboard;

#[test]
fn dashboard_is_default_view() {
    let app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn dashboard_snapshot_from_mp_json_fixture() {
    let status = serde_json::json!({
        "planning_status": "in-execution",
        "inbox_count": 2,
        "pending_review_count": 5,
        "track_pending": 1,
        "annotations_open": 3,
        "execution": { "mode": "autonomous" },
        "suggested_path": {
            "next_action": { "display": "M73/S1" },
            "preview": ["M73/S1", "M73/S2"]
        }
    });
    let inbox = serde_json::json!({
        "items": [
            {
                "kind": "track",
                "id": "TW-03",
                "display": "Fix backlog output"
            }
        ]
    });

    let snap = dashboard::snapshot_from_status_inbox(&status, &inbox);
    assert_eq!(snap.execution_mode, "autonomous");
    assert_eq!(snap.pending_review_count, 5);
    assert_eq!(snap.next_action, "M73/S1");
    assert_eq!(snap.path_preview.len(), 2);
    assert_eq!(snap.inbox_items[0].id, "TW-03");
}

#[test]
fn backlog_summary_uses_description_field() {
    let item = serde_json::json!({
        "id": "B-01",
        "description": "First line of backlog item\nmore detail"
    });
    assert_eq!(reads::backlog_summary(&item), "First line of backlog item");
}

#[test]
fn dashboard_snapshot_loads_fields() {
    let mut app = App::new();
    app.load_dashboard(DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 6,
        pending_review_count: 64,
        track_pending: 1,
        annotations_open: 0,
        next_action: "M71/S1".into(),
        path_preview: vec!["M71/S1".into(), "M71/S2".into()],
        inbox_items: vec![InboxLine {
            id: "TW-03".into(),
            kind: "track".into(),
            display: "Show assigned backlog ID".into(),
            reason: "pending tweak".into(),
            action: "mp track show tweak".into(),
        }],
        ..Default::default()
    });

    assert_eq!(app.dashboard.inbox_count, 6);
    assert_eq!(app.dashboard.pending_review_count, 64);
    assert_eq!(app.dashboard.next_action, "M71/S1");
    assert_eq!(app.dashboard.path_preview.len(), 2);
    assert_eq!(app.dashboard.inbox_items.len(), 1);
}

#[test]
fn navigate_from_dashboard_to_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    assert_eq!(app.active_lane, Lane::Milestones);
    assert_eq!(app.content, ContentState::List);
    app.go_back();
    assert_eq!(app.active_lane, Lane::Overview);
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn navigate_from_dashboard_to_backlog() {
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    assert_eq!(app.active_lane, Lane::Backlog);
    app.go_back();
    assert_eq!(app.active_lane, Lane::Overview);
}

#[test]
fn dashboard_help_toggle() {
    use raul::tui::mode::Mode;
    let mut app = App::new();
    assert!(!matches!(app.active_mode, Mode::Help));
    app.toggle_help();
    assert!(matches!(app.active_mode, Mode::Help));
    app.toggle_help();
    assert!(!matches!(app.active_mode, Mode::Help));
}

/// M181 AC-09: every dashboard section renders an explicit empty
/// state without panicking or stale selection indices when the
/// underlying payload is empty. Seed the typed snapshot with the
/// default (everything zero / empty), pin the chunk geometry, and
/// assert no panic + the empty-state copy is present.
#[test]
fn overview_empty_snapshot_renders_without_panic() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::overview_snapshot::OverviewSnapshot;
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.overview = OverviewSnapshot::default();
    // Empty inbox path (covers the most user-visible empty state).
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .expect("empty overview renders without panic");

    let buffer = terminal.backend().buffer().clone();
    let mut rendered = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }

    // The empty-state copy is rendered verbatim in the inbox and
    // activity blocks. Both sections explicitly say "empty" so the
    // user sees a non-confusing screen on a fresh project.
    assert!(
        rendered.contains("Inbox is empty"),
        "empty inbox state must render explicit copy; got:\n{rendered}"
    );
    assert!(
        rendered.contains("(no recent activity)"),
        "empty activity state must render explicit copy; got:\n{rendered}"
    );
    assert!(
        rendered.contains("(no path)"),
        "empty path state must render explicit copy; got:\n{rendered}"
    );

    // `selected_index` stays valid (the loader clamps it in
    // `load_overview_snapshot`; verify the clamp here against an
    // explicitly-empty inbox).
    assert_eq!(app.selected_index, 0);
}

/// M181 AC-01: responsive split. The Overview lane's lower panel
/// (Inbox | Activity) splits side-by-side on wide terminals and
/// stacks on narrow terminals. `compute_view` exposes the decision
/// via `DashboardChunks.inbox_side_by_side`; the breakpoint is
/// `LOWER_PANEL_MIN_WIDTH = 32` columns per half.
#[test]
fn overview_lower_panel_breakpoint_decision() {
    use ratatui::layout::Rect;
    use raul::tui::view_state;

    let app = App::new();
    let wide = view_state::compute_view(&app, Rect::new(0, 0, 100, 40));
    assert!(
        wide.dashboard_chunks.as_ref().unwrap().inbox_side_by_side,
        "100 columns wide → side-by-side split"
    );
    assert!(
        wide.dashboard_chunks
            .as_ref()
            .unwrap()
            .lower_activity
            .is_some(),
        "side-by-side layout must allocate an activity block"
    );

    let narrow = view_state::compute_view(&app, Rect::new(0, 0, 60, 40));
    assert!(
        !narrow.dashboard_chunks.as_ref().unwrap().inbox_side_by_side,
        "60 columns wide → stacked layout (60/2 < 32 breakpoint)"
    );

    let tiny = view_state::compute_view(&app, Rect::new(0, 0, 30, 40));
    let tiny_chunks = tiny.dashboard_chunks.as_ref().unwrap();
    assert!(!tiny_chunks.inbox_side_by_side, "30 columns wide → stacked");
    // When the lower section can't fit both panels even when stacked
    // (the height is below LOWER_PANEL_MIN_HEIGHT * 2), the activity
    // block is omitted entirely so the inbox still gets a valid
    // (non-zero) rect.
    assert!(
        tiny_chunks.lower_inbox.height > 0,
        "tiny terminal must still allocate a non-empty inbox block"
    );
}

/// M181 narrow-terminal remediation: at 30 rows the redesigned
/// dashboard used to collapse the lower Inbox/Activity split to
/// zero-height rects and the user saw only the top blocks. The
/// remediation compresses the top blocks (Health / Statistics /
/// Work queues / Lifecycle / Path) in compact mode so the lower
/// split fits at 30 rows. This test pins the contract:
///
/// 1. The full layout (≥32 rows) shows the rich per-section copy
///    ("Validation: ...", "Lifecycle" header on its own line, etc.).
/// 2. The compact layout (≤30 rows) compresses to fewer lines per
///    section but still surfaces every required field. The lower
///    Inbox + Activity panels are both visible.
/// 3. Empty-state copy renders in both modes.
#[test]
fn overview_narrow_terminal_keeps_full_hierarchy_visible() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.overview.health.execution_mode = "autonomous".into();
    app.overview.health.planning_state = "in-execution".into();
    app.overview.lifecycle.complete = 1;
    app.overview.path = vec![raul::overview_snapshot::PathItem {
        id: "180".into(),
        display: "M180".into(),
        ..Default::default()
    }];
    app.overview.activity = vec![raul::overview_snapshot::ActivityEvent {
        timestamp: "2026-07-17T18:00:00Z".into(),
        event_type: "milestone-created".into(),
        subject: "180".into(),
        summary: "milestone created".into(),
    }];

    fn render_and_dump(app: &App, height: u16) -> String {
        let backend = TestBackend::new(120, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let view = view_state::compute_view(app, frame.area());
                render::render(frame, app, &view);
            })
            .expect("render without panic");
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    // === Full layout at 40 rows: every section's rich copy shows ===
    let full = render_and_dump(&app, 40);
    assert!(
        full.contains("Validation:"),
        "full mode must show Validation field"
    );
    assert!(
        full.contains("Blockers:"),
        "full mode must show Blockers field"
    );
    assert!(
        full.contains("Execution:"),
        "full mode must show Execution field"
    );
    assert!(
        full.contains("Planning:"),
        "full mode must show Planning field"
    );
    assert!(full.contains("Watch:"), "full mode must show Watch field");
    assert!(
        full.contains("Milestones"),
        "full mode must show Milestones"
    );
    assert!(full.contains("Steps"), "full mode must show Steps header");
    assert!(
        full.contains("pending"),
        "full mode must show pending count"
    );
    assert!(
        full.contains("skipped"),
        "full mode must show skipped count"
    );
    assert!(full.contains("Queues"), "full mode must show Queues header");
    assert!(full.contains("Inbox"), "full mode must show Inbox queue");
    assert!(
        full.contains("Parked ideas"),
        "full mode must show ideas queue"
    );
    assert!(
        full.contains("Lifecycle"),
        "full mode must show Lifecycle grid"
    );
    // M202 S20: the grid is keyed by the 12 mp-flow stage buckets
    // (`<N>/12 <slug>`), so the bucket names are the stage slugs
    // (`remediate`, `re-review`, `hand-off`, …).
    assert!(
        full.contains("1/12 draft"),
        "full mode must show draft bucket"
    );
    assert!(
        full.contains("7/12 complete"),
        "full mode must show complete bucket"
    );
    assert!(
        full.contains("9/12 remediate"),
        "full mode must show remediate bucket"
    );
    assert!(full.contains("Next"), "full mode must show path Next");
    assert!(
        full.contains("Recent activity"),
        "full mode must show activity panel"
    );

    // === Compact layout at 30 rows: every required field still surfaces ===
    let compact = render_and_dump(&app, 30);
    // All six Health fields are present (compact collapses the
    // layout but keeps every field).
    assert!(
        compact.contains("Validation:"),
        "compact mode must show Validation field"
    );
    assert!(
        compact.contains("Blockers:"),
        "compact mode must show Blockers field"
    );
    assert!(
        compact.contains("Execution:"),
        "compact mode must show Execution field"
    );
    assert!(
        compact.contains("Planning:"),
        "compact mode must show Planning field"
    );
    assert!(
        compact.contains("Watch:"),
        "compact mode must show Watch field"
    );
    // Statistics collapses the Steps header + 5 rows into a single
    // summary line "Steps: pending X, in-progress X, done X, failed X, skipped X".
    assert!(
        compact.contains("Milestones"),
        "compact mode must show Milestones"
    );
    assert!(
        compact.contains("Steps: pending"),
        "compact mode must show Steps summary; got:\n{compact}"
    );
    // Work queues collapses to a single line. The default state
    // has all queue counts at 0, so the rendered line is
    // "Inbox 0  Pending 0  Backlog 0  Ideas 0  Annot 0  Blocked 0  Remed 0".
    // We only assert that the queue LABELS appear, not the counts
    // (the labels are stable; the counts vary with the test app).
    assert!(
        compact.contains("Inbox"),
        "compact mode must show Inbox queue"
    );
    assert!(
        compact.contains("Pending"),
        "compact mode must show Pending queue"
    );
    assert!(
        compact.contains("Backlog"),
        "compact mode must show Backlog queue"
    );
    assert!(
        compact.contains("Ideas"),
        "compact mode must show Ideas queue"
    );
    assert!(
        compact.contains("Annot"),
        "compact mode must show Annotations queue"
    );
    assert!(
        compact.contains("Blocked"),
        "compact mode must show Blocked queue"
    );
    assert!(
        compact.contains("Remediation")
            || compact.contains("Remed 0")
            || compact.contains("Remed "),
        "compact mode must show Remediation queue (possibly truncated to 'Remed'); got:\n{compact}"
    );
    // Lifecycle grid + path + activity still render. The grid uses
    // 2 rows of 4+5 buckets in compact mode, but every bucket name
    // still appears.
    assert!(
        compact.contains("Lifecycle"),
        "compact mode must show Lifecycle grid"
    );
    // M202 S20: compact grid uses `<N>/12 <slug>` labels too.
    assert!(
        compact.contains("1/12 draft"),
        "compact mode must show draft bucket"
    );
    assert!(
        compact.contains("7/12 complete"),
        "compact mode must show complete bucket"
    );
    assert!(
        compact.contains("9/12 remediate"),
        "compact mode must show remediate bucket"
    );
    assert!(compact.contains("Next"), "compact mode must show path Next");
    assert!(
        compact.contains("→ M180"),
        "compact mode must show path item"
    );
    assert!(
        compact.contains("Recent activity"),
        "compact mode must show activity panel"
    );
    // The lower Inbox + Activity panels must both have non-zero
    // height (no longer collapsed to zero). `compute_view` is the
    // source of truth here.
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 30));
    let chunks = view.dashboard_chunks.as_ref().unwrap();
    assert!(
        chunks.lower_inbox.height >= 3,
        "compact 30-row layout must keep the inbox block ≥ 3 rows; got {}",
        chunks.lower_inbox.height
    );
    // The activity block can be omitted at very narrow terminals
    // (lower section can't fit both panels stacked). 120 columns
    // wide is plenty for the side-by-side layout.
    assert!(
        chunks.lower_activity.is_some(),
        "120 columns wide at 30 rows: side-by-side must allocate an activity block"
    );
}

/// M181 AC-07 source-shape pin: M179 removed the W/w Overview
/// auto-refresh tick and M181 confirms the absence survived the
/// redesigned dashboard. A future refactor that re-introduces the
/// feature must update both this test and the milestone AC. The
/// four greps below cover each surface the auto-refresh would have
/// to re-touch:
///
/// 1. No `KeyCode::Char('w')` / `KeyCode::Char('W')` arms in any
///    Overview path (keybinds, inbox_nav, modes/normal).
/// 2. No `OverviewKeyAction::ToggleWatch` variant on the
///    per-key handler enum.
/// 3. The Overview footer string carries `refresh` (r/R), not
///    `watch` — the renderer-side surface.
/// 4. `format_countdown` (the watch-tick formatter) is not wired
///    into the Overview footer (would-be inviter of the auto-refresh).
#[test]
fn no_w_w_auto_refresh_pin() {
    use std::fs;

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();

    // Greps 1 & 2: walk every Rust source file under `crates/raul/src/`
    // and fail on the first sign of a W/w Overview auto-refresh arm
    // or a ToggleWatch variant.
    let src_root = workspace_root.join("crates/raul/src");
    let mut walked = 0usize;
    for entry in walk_rust_files(&src_root) {
        walked += 1;
        let text = fs::read_to_string(&entry).expect("read rust source");
        let rel = entry
            .strip_prefix(&workspace_root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            !text.contains("OverviewKeyAction::ToggleWatch"),
            "{rel}: OverviewKeyAction::ToggleWatch variant reintroduced — \
             M179 removed W/w auto-refresh; do not bring it back without a \
             new milestone AC for it. See crates/raul/src/tui/inbox_nav.rs \
             for the canonical OverviewKeyAction enum."
        );
        // The Overview handler in `inbox_nav.rs::map_overview_key` is
        // the only allowed place where `KeyCode::Char('w')`/`'W'` would
        // land. We grep the surrounding dispatch chain to confirm
        // nothing in `modes/normal.rs` re-introduces a W/w Overview arm.
        if rel.ends_with("tui/modes/normal.rs") {
            assert!(
                !text.contains("KeyCode::Char('w')") && !text.contains("KeyCode::Char('W')"),
                "{rel}: a `KeyCode::Char('w' | 'W')` arm landed in the \
                 Overview mode dispatcher; M179 removed W/w auto-refresh — \
                 keep r/R as the only manual refresh path."
            );
        }
    }
    assert!(
        walked > 0,
        "expected to walk at least one Rust source under {src_root:?}"
    );

    // Grep 3: the Overview footer must NOT mention `watch` (the
    // legacy "watch ON — next refresh" copy was the user-visible
    // half of the auto-refresh feature).
    use raul::tui::keybinds::Keybinds;
    let kb = Keybinds::default();
    let footer = kb.footer_overview();
    assert!(
        !footer.to_ascii_lowercase().contains("watch"),
        "Overview footer must not mention `watch` (M179 removed auto-refresh); \
         got: {footer:?}"
    );
    assert!(
        footer.contains("refresh"),
        "Overview footer must surface manual refresh; got: {footer:?}"
    );

    // Grep 4: the watch-tick formatter (`format_countdown`) is still
    // in `keybinds.rs` for the Watch lane footer (a separate,
    // legitimate surface). It must not be referenced from the
    // Overview footer formatter — a regression that would
    // re-introduce the auto-refresh countdown.
    let keybinds_src = fs::read_to_string(workspace_root.join("crates/raul/src/tui/keybinds.rs"))
        .expect("read keybinds.rs");
    // Find the body of `pub fn footer_overview` and check for
    // `format_countdown` inside it. The naive substring search is
    // good enough — the function body is small and `format_countdown`
    // only appears in one place (the Watch footer).
    let start = keybinds_src
        .find("pub fn footer_overview(&self) -> String")
        .expect("footer_overview present");
    let end = keybinds_src
        .find("pub fn footer_list(&self) -> String")
        .expect("footer_list follows footer_overview");
    let body = &keybinds_src[start..end];
    assert!(
        !body.contains("format_countdown"),
        "footer_overview must not call format_countdown (would \
         re-introduce the auto-refresh countdown)"
    );
}

fn walk_rust_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    fn rec(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    rec(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    out.push(path);
                }
            }
        }
    }
    rec(root, &mut out);
    out
}
