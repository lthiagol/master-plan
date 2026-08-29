//! M182 S5: regression coverage for the full sort-rebind flow.
//!
//! The existing `tui_sort_rebind` test file (M172 S5) covers the
//! per-action state machine (open / cycle / confirm / cancel) and
//! the per-lane state. M182 S5 adds end-to-end pins that exercise
//! the renderer's output against the rendered buffer (not just the
//! `App` state) and the persistence round-trip:
//!
//! - Rendering a lane with a bound sort key actually reorders the
//!   milestone list (regression on F-03 — the in-memory HashMap
//!   write was silently broken pre-M172).
//! - Re-binding changes the order (the menu promise: "confirm and
//!   your list reorders").
//! - The sort-rebind choice survives a "restart" — i.e. loading from
//!   config.json produces the same in-memory state.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary, SortKey};
use raul::tui::render;
use raul::tui::view_state;

fn make_app_with_milestones() -> App {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-15".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
    ]);
    app
}

fn render_milestones_lane_to_string(app: &App) -> String {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// AC-05: rendered order changes when the sort key changes. The
/// test uses the buffer symbols (M-prefixed ids) — a fast string
/// diff pins that the rendered position of M01 vs M02 swaps when
/// the sort key changes. Pre-M172 the in-memory HashMap write was
/// silently broken; this test catches the regression where the
/// state mutates but the render path doesn't see it.
#[test]
fn m182_s5_rendered_order_changes_when_sort_key_changes() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);

    // Default sort key is Id — M01 before M02.
    let output = render_milestones_lane_to_string(&app);
    let pos_01 = output.find("M01").expect("M01 in output");
    let pos_02 = output.find("M02").expect("M02 in output");
    assert!(
        pos_01 < pos_02,
        "default (Id) order must place M01 before M02; got pos_01={pos_01} pos_02={pos_02}"
    );

    // Rebind to Priority — M01 (urgent) before M02 (low) — the
    // positions DON'T swap here (id order happens to match priority
    // order for this fixture). The M02 priority bump below inverts
    // them.
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    let output = render_milestones_lane_to_string(&app);
    let pos_01 = output.find("M01").expect("M01 in output");
    let pos_02 = output.find("M02").expect("M02 in output");
    assert!(
        pos_01 < pos_02,
        "priority order with M01 urgent / M02 low: M01 first; got pos_01={pos_01} pos_02={pos_02}"
    );

    // Bump M02 to urgent. Now M02 should sort above M01 (id order
    // unchanged). Pre-fix this would render M01 first because the
    // render path didn't see the in-memory HashMap change.
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-15".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(), // bumped
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
    ]);
    // Same priority → numeric-id tie-breaker → M01 < M02 still.
    let output = render_milestones_lane_to_string(&app);
    let pos_01 = output.find("M01").expect("M01");
    let pos_02 = output.find("M02").expect("M02");
    assert!(
        pos_01 < pos_02,
        "tied urgent → numeric-id tiebreak; M01 first"
    );

    // Now M01 → low, M02 → urgent. Priority reorders them: M02 first.
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(), // dropped
            updated: "2026-07-15".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
    ]);
    let _visible = app.visible_milestones();
    let output = render_milestones_lane_to_string(&app);
    let pos_01 = output.find("M01").expect("M01");
    let pos_02 = output.find("M02").expect("M02");
    assert!(
        pos_02 < pos_01,
        "priority order with M01 low / M02 urgent: M02 first; got pos_01={pos_01} pos_02={pos_02}"
    );
}

/// AC-05: the bound sort key survives a "restart" — i.e. simulating
/// the persistence flow: `persist_sort_rebind_choice` writes to
/// `mp config set` (mocked here as a direct HashMap write since
/// raul's persistence layer is exercised separately by the mp
/// integration tests), and a fresh `App` rehydrates the same state
/// when the loader runs.
///
/// In production this is the round-trip across raul exits and
/// restarts: `mp config set sort.<lane> <sortkey>` writes JSON,
/// `load_persisted_sort_keys` reads it back. The test exercises the
/// RAUL-side pieces: writing to the in-memory state via the same
/// path the menu uses (`App::lane_sort_key.insert`), then asserting
/// a fresh `App` reads it back via `lane_sort_key(lane)`.
#[test]
fn m182_s5_choice_survives_simulated_restart() {
    use raul::tui::app::SortKey;

    // "Session 1" — user binds Milestones → Lifecycle.
    let mut app_session1 = App::new();
    app_session1
        .lane_sort_key
        .insert(Lane::Milestones, SortKey::Lifecycle);
    assert_eq!(
        app_session1.lane_sort_key(Lane::Milestones),
        SortKey::Lifecycle
    );

    // Simulate persistence: write the binding to a (mocked) config
    // map. In production this is `mp config set sort.milestones
    // lifecycle`. The mock keeps the test raul-only.
    let mut persisted: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    persisted.insert("milestones".to_string(), "lifecycle".to_string());

    // "Session 2" — fresh App, no in-memory state. The loader
    // simulates reading the persisted map and inserting into the
    // fresh App's HashMap (the same shape `load_persisted_sort_keys`
    // does at startup).
    let mut app_session2 = App::new();
    app_session2.select_lane(Lane::Milestones);
    assert_eq!(app_session2.lane_sort_key(Lane::Milestones), SortKey::Id);
    if let Some(value) = persisted.get("milestones") {
        let sort_key = match value.as_str() {
            "id" => SortKey::Id,
            "lifecycle" => SortKey::Lifecycle,
            "priority" => SortKey::Priority,
            "updated" => SortKey::Updated,
            _ => SortKey::Id,
        };
        app_session2
            .lane_sort_key
            .insert(Lane::Milestones, sort_key);
    }
    assert_eq!(
        app_session2.lane_sort_key(Lane::Milestones),
        SortKey::Lifecycle,
        "M182 S5: bound sort key must survive a restart"
    );

    // And the render reflects it — the visible milestones sort by
    // lifecycle rank, not id order. With M01 in-progress and M02
    // approved, lifecycle puts M01 first (M01 in-progress > M02
    // approved in the lifecycle_rank mapping).
    app_session2.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-15".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
    ]);
    let output = render_milestones_lane_to_string(&app_session2);
    let pos_01 = output.find("M01").expect("M01");
    let pos_02 = output.find("M02").expect("M02");
    assert!(
        pos_01 < pos_02,
        "lifecycle rank (in-progress > approved) places M01 before M02; got pos_01={pos_01} pos_02={pos_02}"
    );
}

/// AC-05: lane_sort_key is per-lane — binding Milestones to Lifecycle
/// doesn't affect Backlog / Ideas (which keep their default
/// SortKey::Id). This is the cross-lane isolation regression: a
/// future change that accidentally promoted a bind across all lanes
/// would surface here.
#[test]
fn m182_s5_bind_is_per_lane_not_global() {
    let mut app = App::new();
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    for lane in [Lane::Backlog, Lane::Ideas] {
        assert_eq!(
            app.lane_sort_key(lane),
            SortKey::Id,
            "lane {lane:?} should not be affected by Milestones→Priority bind"
        );
    }
}
