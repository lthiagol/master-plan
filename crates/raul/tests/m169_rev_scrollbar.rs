//! M169-rev scrollbar fixes — regression tests for the milestone-detail
//! scrollbar regression: "scrollbar goes down only two steps, it
//! seems stuck" + "the mouse isn't working when scrolling into the
//! milestone" (user report, 2026-07-15).
//!
//! The keyboard-Down path was already wired (`App::move_down` handles
//! MilestoneDetail by incrementing `detail_scroll`), but:
//!   1. `measure_paragraph_height` returned the panel height (capped at
//!      the buffer edge), so `detail_max_scroll` was clamped to
//!      `panel.height - visible ≈ 2` for any detail that overflowed.
//!   2. The mouse-wheel handler required `app.content ==
//!      ContentState::List`, silently dropping wheel events on
//!      MilestoneDetail.

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::render::scrollbar::measure_paragraph_height;
use raul::tui::runner::test_helpers::handle_mouse;

fn mp_bin() -> std::path::PathBuf {
    // M194: probe both release and debug profiles. CI builds --release
    // so  doesn't exist; the previous lookup assumed
    // debug and silently fell back to PATH (where  isn't installed).
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("../../target/release/mp"),
        manifest.join("../../target/debug/mp"),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from("mp"))
}

fn fixture() -> (tempfile::TempDir, MpRunner) {
    let tmp = tempfile::TempDir::new().expect("temp");
    let root = tmp.path().to_path_buf();
    let status = std::process::Command::new(mp_bin())
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(&root)
        .status()
        .expect("mp init");
    assert!(status.success());
    let mut runner = MpRunner::with_mp_bin(mp_bin());
    runner.set_project_root(root.clone());
    runner.set_plan_dir(root.join("master-plan"));
    (tmp, runner)
}

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    let idx = Lane::ordered()
        .iter()
        .position(|l| *l == Lane::Settings)
        .unwrap();
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

// ---------------------------------------------------------------------------
// Bug 1 (user report: scrollbar only goes down 2 steps).
//
// `measure_paragraph_height` returned the panel height (because the
// bottom border was the last non-blank row in a panel-sized buffer),
// so `detail_max_scroll = panel.height - visible` was tiny. The fix
// renders into an 8×-panel buffer and counts rows that have at least
// one non-border, non-blank cell.
// ---------------------------------------------------------------------------

#[test]
fn rev_detail_max_scroll_reaches_full_content_for_tall_milestone() {
    let (tmp, runner) = fixture();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);

    // We can't easily force `mp show milestone <id>` to return a long
    // body without a real milestone, but we can drive the same code
    // path: build the same Paragraph the renderer uses and assert
    // measure_paragraph_height reports the full content height, not
    // the panel height. The renderer's `detail_max_scroll` setter
    // uses the same function.
    //
    // 30 logical lines, each ~40 chars → fits on 1 row each in a
    // 77-col inner area → 30 content rows.
    let detail_area = Rect::new(0, 0, 79, 20);
    let lines: Vec<Line> = (0..30)
        .map(|i| Line::from(format!("line {i:02} {}", "x".repeat(30))))
        .collect();
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("M1 Detail")
                .border_type(BorderType::Thick),
        )
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    let visible = detail_area.height.saturating_sub(2);
    let max_scroll = measured.saturating_sub(visible);
    assert!(
        max_scroll >= 10,
        "BUG (pre-fix): max_scroll was 2; post-fix must be ≥ 10 for a 30-line body"
    );
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Bug 2 (user report: mouse wheel doesn't scroll inside a milestone).
//
// The mouse-wheel handler dropped ScrollUp/ScrollDown when
// `app.content != ContentState::List`. The fix broadens the gate to
// include MilestoneDetail / BacklogDetail / AnnotationThread.
// ---------------------------------------------------------------------------

#[test]
fn rev_wheel_scrolls_milestone_detail_via_handle_mouse() {
    // We can't easily render a milestone detail in a unit test
    // without an mp fixture, so we drive `handle_mouse` directly with
    // the App in `ContentState::MilestoneDetail` and assert
    // `detail_scroll` advances. `handle_mouse` requires a `view` and
    // a `runner` — we use a dummy ViewState (no rects needed for the
    // wheel path) and the fixture runner.
    use raul::tui::view_state::ViewState;

    let (_tmp, runner) = fixture();
    let mut app = App::new();
    // Force the app into MilestoneDetail without going through the
    // dispatcher — simulate a real milestone detail render.
    app.content = ContentState::MilestoneDetail;
    app.detail_scroll = 5;
    // detail_max_scroll must be > 0 for move_down to advance scroll.
    app.detail_max_scroll.set(50);

    let _view = ViewState::default();

    // Wheel up at a content row (y >= 2).
    let mouse_up = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 40,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    handle_mouse(&mut app, &runner, mouse_up, (120, 40)).unwrap();
    assert_eq!(
        app.detail_scroll, 4,
        "Wheel up on MilestoneDetail must decrement detail_scroll"
    );

    // Wheel down.
    let mouse_down = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    handle_mouse(&mut app, &runner, mouse_down, (120, 40)).unwrap();
    assert_eq!(
        app.detail_scroll, 5,
        "Wheel down on MilestoneDetail must increment detail_scroll"
    );
}

#[test]
fn rev_wheel_does_not_scroll_on_tab_bar() {
    // AC-09: wheel on the tab bar (y == 1) is a no-op for all lanes.
    use raul::tui::view_state::ViewState;
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    app.detail_scroll = 5;
    app.detail_max_scroll.set(50);

    let _view = ViewState::default();

    let mouse = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40,
        row: 1, // tab bar row
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    handle_mouse(&mut app, &runner, mouse, (120, 40)).unwrap();
    assert_eq!(
        app.detail_scroll, 5,
        "Wheel on the tab bar must not scroll MilestoneDetail"
    );
}

#[test]
fn rev_wheel_does_not_scroll_on_header() {
    // Header row (y == 0) is also a no-op.
    use raul::tui::view_state::ViewState;
    let (_tmp, runner) = fixture();
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    app.detail_scroll = 5;
    app.detail_max_scroll.set(50);

    let _view = ViewState::default();

    let mouse = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40,
        row: 0, // header row
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    handle_mouse(&mut app, &runner, mouse, (120, 40)).unwrap();
    assert_eq!(
        app.detail_scroll, 5,
        "Wheel on the header must not scroll MilestoneDetail"
    );
}

#[test]
fn rev_keyboard_down_advances_detail_scroll() {
    // Pin the keyboard path: with `detail_max_scroll` set, repeated
    // Down presses must advance `detail_scroll` until the cap.
    let (_tmp, _runner) = fixture();
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    app.detail_scroll = 0;
    app.detail_max_scroll.set(10);
    for expected in 1..=10 {
        app.move_down();
        assert_eq!(
            app.detail_scroll, expected,
            "Down press #{expected} must advance detail_scroll"
        );
    }
    // Past the cap, additional presses are no-ops (no negative clamping
    // because detail_scroll is u16 and detail_max_scroll is the cap).
    app.move_down();
    assert_eq!(app.detail_scroll, 10);
}

#[test]
fn rev_detail_max_scroll_stable_across_partial_scroll_render_path() {
    // **M4 — sub-agent review follow-up:** drive the full render path
    // (not just `measure_paragraph_height` in isolation) and assert
    // `detail_max_scroll.get()` stays at the full-content cap after
    // partial scrolling. The H1 bug (sub-agent review, fix shipped
    // in 4b229f0) would have produced a shrink here; this test pins
    // the render-path invariance, complementing the helper-level
    // `measure_returns_full_content_height_regardless_of_scroll_offset`.
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render::render;
    use raul::tui::view_state::compute_view;

    let (_tmp, _runner) = fixture();
    let mut app = App::new();
    // Build a milestone detail whose body forces the renderer past
    // the panel height. 1500 chars in the intent.outcome wraps to
    // ~25 rows in a 75-wide inner area — well past the 20-row panel.
    let long_intent = "x ".repeat(750);
    let body = serde_json::json!({
        "milestone": {
            "id": "169",
            "title": "Long body",
            "lifecycle": "in-progress",
            "spec_status": "ready",
            "execution_status": "in-progress",
            "effort": "S",
            "risk": "low",
            "blocked": false,
            "cancelled": false,
            "needs_regrooming": false,
            "deferred": false,
            "created": "2026-01-01",
            "updated": "2026-01-01"
        },
        "intent": { "outcome": long_intent },
        "problem": { "description": "" },
        "scope": { "in_scope": [], "out_of_scope": [] },
        "acceptance_criteria": [],
        "steps": [],
        "design_decisions": [],
        "open_questions": [],
        "work_packages": [],
        "findings": [],
        "verification": {}
    });
    app.milestone_detail = Some(body);
    app.content = ContentState::MilestoneDetail;
    app.active_lane = Lane::Milestones;

    // Drive the full render path: terminal.draw → render() →
    // render_milestone_detail → measure → detail_max_scroll.set.
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();

    let cap_at_scroll_0 = app.detail_max_scroll.get();
    assert!(
        cap_at_scroll_0 >= 5,
        "tall body must produce a useful scroll cap; got {cap_at_scroll_0}"
    );

    // Scroll down 5 times, re-render, assert cap is unchanged.
    for _ in 0..5 {
        app.move_down();
    }
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();

    assert_eq!(
        app.detail_max_scroll.get(),
        cap_at_scroll_0,
        "BUG (H1, M4): detail_max_scroll must not shrink as the user scrolls"
    );

    // Scroll to the bottom — cap still unchanged.
    while app.detail_scroll < cap_at_scroll_0 {
        app.move_down();
    }
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();
    assert_eq!(
        app.detail_max_scroll.get(),
        cap_at_scroll_0,
        "BUG (H1, M4): detail_max_scroll must equal the full-content cap at every scroll position"
    );
}

#[test]
fn rev_detail_measurement_cache_hits_on_unchanged_body() {
    // **L3a (sub-agent review):** the measurement cache must skip
    // the 8×-panel Buffer allocation + Paragraph::render when the
    // body and panel width haven't changed. Pin the cache
    // contract: after two consecutive renders with the same
    // `app.milestone_detail`, the cache is populated and the second
    // measurement reuses it.
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render::render;
    use raul::tui::view_state::compute_view;

    let (_tmp, _runner) = fixture();
    let mut app = App::new();
    app.milestone_detail = Some(serde_json::json!({
        "milestone": {
            "id": "1", "title": "Body", "lifecycle": "in-progress",
            "spec_status": "ready", "execution_status": "in-progress",
            "effort": "S", "risk": "low", "blocked": false,
            "cancelled": false, "needs_regrooming": false,
            "deferred": false, "created": "2026-01-01", "updated": "2026-01-01"
        },
        "intent": { "outcome": "x".repeat(1500) },
        "problem": { "description": "" },
        "scope": { "in_scope": [], "out_of_scope": [] },
        "acceptance_criteria": [],
        "steps": [], "design_decisions": [], "open_questions": [],
        "work_packages": [], "findings": [], "verification": {}
    }));
    app.content = ContentState::MilestoneDetail;
    app.active_lane = Lane::Milestones;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // First render: cache miss → re-measure.
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();
    assert!(
        app.detail_measurement_cache.get().is_some(),
        "first render must populate the cache"
    );
    let cached = app.detail_measurement_cache.get().unwrap();
    let cap_after_first = app.detail_max_scroll.get();

    // Second render with the same body: cache hit (no re-measure).
    // The cached value is the SAME struct (Copy semantics), so the
    // field values must be unchanged.
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();
    assert_eq!(
        app.detail_measurement_cache.get().unwrap().content_hash,
        cached.content_hash,
        "cache hash must be stable across renders with the same body"
    );
    assert_eq!(
        app.detail_max_scroll.get(),
        cap_after_first,
        "cap must be stable across renders with the same body"
    );
}

#[test]
fn rev_detail_measurement_cache_invalidates_on_content_change() {
    // **L3a:** cache must miss when the body changes (different
    // content_hash → re-measure).
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render::render;
    use raul::tui::view_state::compute_view;

    let (_tmp, _runner) = fixture();
    let mut app = App::new();
    let mk_body = |intent: &str| -> serde_json::Value {
        serde_json::json!({
            "milestone": {
                "id": "1", "title": "Body", "lifecycle": "in-progress",
                "spec_status": "ready", "execution_status": "in-progress",
                "effort": "S", "risk": "low", "blocked": false,
                "cancelled": false, "needs_regrooming": false,
                "deferred": false, "created": "2026-01-01", "updated": "2026-01-01"
            },
            "intent": { "outcome": intent },
            "problem": { "description": "" },
            "scope": { "in_scope": [], "out_of_scope": [] },
            "acceptance_criteria": [],
            "steps": [], "design_decisions": [], "open_questions": [],
            "work_packages": [], "findings": [], "verification": {}
        })
    };
    app.content = ContentState::MilestoneDetail;
    app.active_lane = Lane::Milestones;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    app.milestone_detail = Some(mk_body("first content"));
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();
    let hash_1 = app.detail_measurement_cache.get().unwrap().content_hash;

    // Swap to a different body — cache must miss and re-measure.
    app.milestone_detail = Some(mk_body(
        "second content, longer than the first to force a different line count after wrap",
    ));
    terminal
        .draw(|f| {
            let view = compute_view(&app, f.area());
            render(f, &app, &view);
        })
        .unwrap();
    let hash_2 = app.detail_measurement_cache.get().unwrap().content_hash;

    assert_ne!(
        hash_1, hash_2,
        "cache hash must change when the body changes (different hash → re-measure)"
    );
}
