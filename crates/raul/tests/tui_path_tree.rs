//! M157: TUI Path tab renders the same vertical tree (smoke + scroll).
//!
//! AC-05: the Path tab in the TUI renders a scrollable vertical
//! tree mirroring the CLI — EXECUTION trunk + branches — replacing
//! the pre-M157 5-column lane layout. The test instantiates an
//! `App`, seeds `path_data` with a synthetic lane envelope (exec +
//! blocked + awaiting-approval), selects the Path lane, renders
//! the tab into a Frame buffer, and asserts the rendered text
//! contains the trunk header and branch labels. Scroll state is
//! exercised against a short viewport.

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::render::render as render_frame;
use raul::tui::view_state::{compute_view, ScrollableId};

fn sample_path_data() -> serde_json::Value {
    serde_json::json!({
        "strategy": "resume_then_ready",
        "lanes": [
            {
                "name": "execution",
                "item_type": "milestone",
                "item_count": 1,
                "head": null,
                "items": [
                    {
                        "rank": 1,
                        "type": "milestone",
                        "milestone": {
                            "id": "99",
                            "title": "Tree smoke milestone",
                            "lifecycle": "approved",
                            "priority": "normal",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M99",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "execution lane head",
                    },
                ],
            },
            {
                "name": "blocked",
                "item_type": "milestone",
                "item_count": 1,
                "head": null,
                "items": [
                    {
                        "rank": 1,
                        "type": "milestone",
                        "milestone": {
                            "id": "77",
                            "title": "Blocked on M99",
                            "lifecycle": "approved",
                            "priority": "normal",
                            "depends_on": ["99"],
                            "review_phase": "",
                            "kind": "",
                            "display": "M77",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "blocked",
                    },
                ],
            },
            {
                "name": "awaiting-approval",
                "item_type": "milestone",
                "item_count": 1,
                "head": null,
                "items": [
                    {
                        "rank": 1,
                        "type": "milestone",
                        "milestone": {
                            "id": "88",
                            "title": "Spec ready awaiting approve",
                            "lifecycle": "groomed",
                            "priority": "high",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M88",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "awaiting approval",
                    },
                ],
            },
        ],
        "summary": {
            "execution": 1,
            "awaiting_approval": 1,
            "blocked": 1,
            "review": 0,
            "grooming": 0,
            "backlog": 0,
        },
        "status": {
            "milestones": {
                "by_lifecycle": {
                    "complete": 12
                }
            }
        },
    })
}

fn render_to_string(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = compute_view(app, Rect::new(0, 0, width, height));
            render_frame(frame, app, &view);
        })
        .unwrap();
    let mut s = String::new();
    let buf = terminal.backend().buffer().clone();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

#[test]
fn path_tab_renders_tree_with_execution_trunk() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(sample_path_data());
    let output = render_to_string(&app, 140, 30);
    assert!(
        output.contains("EXECUTION"),
        "TUI Path tab must show EXECUTION trunk header; got:\n{output}"
    );
    assert!(
        output.contains("M99") && output.contains("Tree smoke milestone"),
        "TUI Path tab must show execution milestone; got:\n{output}"
    );
    assert!(
        output.contains("Blocked"),
        "TUI Path tab must show Blocked branch; got:\n{output}"
    );
    // CLI parity: branch header sits under the trunk (leading two spaces).
    assert!(
        output.contains("  └─") || output.contains("  ├─"),
        "TUI Path branch header must be trunk-indented (leading 2 spaces); got:\n{output}"
    );
    assert!(
        output.contains("  ╵") && output.contains("12 complete"),
        "TUI complete footer must be trunk-indented; got:\n{output}"
    );
    assert!(
        output.contains("blocked-by M99"),
        "TUI Path tab must show blocked-by-M99 fork header; got:\n{output}"
    );
    assert!(
        output.contains("M77"),
        "TUI Path tab must show the blocked milestone; got:\n{output}"
    );
    assert!(
        output.contains("Awaiting approval") && output.contains("M88"),
        "TUI Path tab must show awaiting-approval branch; got:\n{output}"
    );
    assert!(
        output.contains("12 complete"),
        "TUI Path tab must show collapsed complete footer; got:\n{output}"
    );
}

#[test]
fn path_tab_scrolls_and_reserves_single_scrollbar() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    // M167: app.tab_bar_focused = false; (no-op; field removed)
    app.load_path_data(sample_path_data());

    // Short viewport forces overflow.
    let area = Rect::new(0, 0, 100, 12);
    let view = compute_view(&app, area);
    let path_bars: Vec<_> = view
        .scrollbar_rects
        .iter()
        .filter(|h| h.id == ScrollableId::PathLane)
        .collect();
    assert_eq!(
        path_bars.len(),
        1,
        "Path tree must reserve exactly one scrollbar (not per-lane columns)"
    );
    let bar = path_bars[0];
    assert!(
        bar.total > bar.visible,
        "short viewport must report overflow; total={} visible={}",
        bar.total,
        bar.visible
    );
    assert!(
        app.path_max_scroll.get() > 0,
        "path_max_scroll must be positive when tree overflows"
    );

    // j / move_down advances path_scroll.
    let before = app.path_scroll;
    app.move_down();
    assert_eq!(app.path_scroll, before + 1);

    // After scroll, top content leaves the short viewport.
    let scrolled = render_to_string(&app, 100, 12);
    // Header chrome still present; EXECUTION may scroll off on tiny height.
    assert!(
        scrolled.contains("Path") || scrolled.contains("EXECUTION") || scrolled.contains("Blocked"),
        "scrolled Path tab still paints tree content; got:\n{scrolled}"
    );

    // Page down clamps at max.
    app.move_page_down();
    app.move_page_down();
    app.move_page_down();
    assert!(
        app.path_scroll <= app.path_max_scroll.get(),
        "path_scroll must clamp at max"
    );
    let at_max = app.path_scroll;
    app.move_down();
    assert_eq!(app.path_scroll, at_max, "move_down at max is a no-op");

    // Page up / move_up recover.
    app.move_page_up();
    assert!(app.path_scroll < at_max || at_max == 0);
    while app.path_scroll > 0 {
        app.move_up();
    }
    assert_eq!(app.path_scroll, 0);
}

#[test]
fn load_path_data_resets_scroll() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(sample_path_data());
    let _ = compute_view(&app, Rect::new(0, 0, 100, 10));
    app.path_scroll = app.path_max_scroll.get().max(1);
    app.load_path_data(sample_path_data());
    assert_eq!(app.path_scroll, 0);
    assert_eq!(app.path_max_scroll.get(), 0);
}
