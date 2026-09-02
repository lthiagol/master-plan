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

// =========================================================================
// M206 — Path tab rework: 2-line preview + stage chip + status overlay
// =========================================================================

/// M206: rich path envelope — milestones carry `intent.outcome`,
/// `flow_stages`, `priority`, `lifecycle_at`, `blocked` /
/// `cancelled` / `deferred`. The fixture exercises the full
/// title-line + preview-line shape so the integration tests can
/// inspect rendered cells end-to-end.
fn rich_path_data() -> serde_json::Value {
    serde_json::json!({
        "strategy": "resume_then_ready",
        "lanes": [
            {
                "name": "execution",
                "item_type": "milestone",
                "item_count": 2,
                "head": null,
                "items": [
                    {
                        "rank": 1,
                        "type": "milestone",
                        "milestone": {
                            "id": "10",
                            "title": "First execution item",
                            "lifecycle": "approved",
                            "priority": "high",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M10",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                            "intent": {
                                "outcome": "Implement the first execution item with intent preview visible in the Path tree.\nSecond line of outcome that must not leak into the preview."
                            },
                            "flow_stages": {
                                "draft": { "status": "done" },
                                "groom-spec": { "status": "done" },
                                "approve-spec": { "status": "in_progress" },
                                "execute": { "status": "pending" },
                            },
                            "lifecycle_at": "2026-08-30T10:00:00Z",
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "execution lane head",
                    },
                    {
                        "rank": 2,
                        "type": "milestone",
                        "milestone": {
                            "id": "11",
                            "title": "Second execution item",
                            "lifecycle": "approved",
                            "priority": "normal",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M11",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                            "intent": {
                                "outcome": "Implement the second execution item."
                            },
                            "flow_stages": {},
                            "lifecycle_at": "2026-08-31T08:00:00Z",
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "execution queue tail",
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
                            "id": "20",
                            "title": "Blocked on M10",
                            "lifecycle": "approved",
                            "priority": "high",
                            "depends_on": ["10"],
                            "review_phase": "",
                            "kind": "",
                            "display": "M20",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                            "blocked": true,
                            "block_reason": "waiting on M10",
                            "blocked_by": "M10",
                            "intent": {
                                "outcome": "Ship the second milestone once M10 lands."
                            },
                            "flow_stages": {},
                            "lifecycle_at": "2026-08-30T11:00:00Z",
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
                            "id": "30",
                            "title": "Awaiting approval milestone",
                            "lifecycle": "groomed",
                            "priority": "low",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M30",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                            "cancelled": true,
                            "intent": {
                                "outcome": "Cancellation reason: superseded by M205."
                            },
                            "flow_stages": {},
                            "lifecycle_at": "2026-08-29T10:00:00Z",
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "awaiting approval",
                    },
                ],
            },
            {
                "name": "grooming",
                "item_type": "milestone",
                "item_count": 1,
                "head": null,
                "items": [
                    {
                        "rank": 1,
                        "type": "milestone",
                        "milestone": {
                            "id": "40",
                            "title": "Grooming milestone",
                            "lifecycle": "draft",
                            "priority": "normal",
                            "depends_on": [],
                            "review_phase": "",
                            "kind": "",
                            "display": "M40",
                            "needs_regrooming": false,
                            "open_external_findings": 0,
                            "open_self_findings": 0,
                            "deferred": true,
                            "intent": {
                                "outcome": "   \n  \n"
                            },
                            "flow_stages": {},
                            "lifecycle_at": "2026-08-28T10:00:00Z",
                        },
                        "step": null,
                        "work_package": null,
                        "reason": "grooming",
                    },
                ],
            },
            {
                "name": "review",
                "item_type": "milestone",
                "item_count": 0,
                "head": null,
                "items": [],
            },
        ],
        "summary": {
            "execution": 2,
            "awaiting_approval": 1,
            "blocked": 1,
            "review": 0,
            "grooming": 1,
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

fn render_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = compute_view(app, Rect::new(0, 0, width, height));
            render_frame(frame, app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Locate the first row in the buffer that contains `needle` (substring).
/// Returns the row index, or `None` when not found. Useful for
/// asserting that a particular line — title vs preview — actually
/// appears in the rendered output.
fn find_row_with(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<u16> {
    for y in 0..buf.area.height {
        let mut row = String::new();
        for x in 0..buf.area.width {
            row.push_str(buf[(x, y)].symbol());
        }
        if row.contains(needle) {
            return Some(y);
        }
    }
    None
}

#[test]
fn path_milestone_renders_two_visual_lines() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    // The first execution item (M10) must occupy TWO consecutive rows:
    // a title row carrying the marker + label and a preview row
    // carrying the ↳ + first line of intent.outcome.
    let title_y = find_row_with(&buf, "M10 — First execution item")
        .expect("title row for M10 must be present");
    // Preview row sits one row below the title row.
    let preview_y = find_row_with(&buf, "Implement the first execution item")
        .expect("preview row for M10 must be present");
    assert_eq!(
        preview_y - title_y,
        1,
        "preview row must be the line directly below the title row"
    );
}

#[test]
fn preview_uses_first_line_of_intent_outcome() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    // M10's intent.outcome has TWO lines (split by \n); the preview
    // must show only the FIRST line. The second line must NOT leak
    // into the rendered tree.
    assert!(
        buf_search(&buf, "Implement the first execution item with intent preview visible in the Path tree."),
        "first line of outcome must render in the preview"
    );
    assert!(
        !buf_search(&buf, "Second line of outcome that must not leak"),
        "second line of outcome must not appear in the preview"
    );
}

fn buf_search(buf: &ratatui::buffer::Buffer, needle: &str) -> bool {
    find_row_with(buf, needle).is_some()
}

#[test]
fn preview_truncates_at_eighty_chars() {
    // Long outcome (> 80 chars on the first line) gets truncated and
    // ends with `…`. We assert:
    //   1. The preview row contains exactly 80 'a' characters followed
    //      by `…` (NOT 200 — proving truncation).
    //   2. The ellipsis marker `…` is present.
    //   3. The preview row length is bounded (80 chars + `…`).
    let mut data = rich_path_data();
    let long = "a".repeat(200);
    data["lanes"][0]["items"][0]["milestone"]["intent"]["outcome"] = serde_json::Value::String(long.clone());
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(data);
    let buf = render_buffer(&app, 200, 60);
    let title_y = find_row_with(&buf, "M10 — First execution item")
        .expect("M10 title row must exist");
    let preview_y = title_y + 1;
    let mut row = String::new();
    for x in 0..buf.area.width {
        row.push_str(buf[(x, preview_y)].symbol());
    }
    // Count 'a' cells on the preview row.
    let a_count = row.chars().filter(|c| *c == 'a').count();
    assert_eq!(
        a_count, 80,
        "preview row must contain exactly 80 'a' chars (truncated); got {a_count}"
    );
    // Ellipsis must be present.
    assert!(
        row.contains('…'),
        "preview row must end with the … ellipsis marker; row: {row:?}"
    );
    // Total chars of 'a' + '…' on the preview row = 81 (the truncated content).
    // The row itself may have other text (prefix), so we just verify
    // the a/… content length is bounded by checking that no 90-char
    // substring of pure a's exists.
    let ninety_a: String = "a".repeat(90);
    assert!(
        !row.contains(&ninety_a),
        "preview row must NOT carry 90+ consecutive 'a' chars (would prove no truncation)"
    );
}

#[test]
fn preview_line_uses_arrow_prefix() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    // The preview row carrying "Implement the first execution item…"
    // must ALSO carry the `↳` glyph on the same row.
    let preview_y = find_row_with(&buf, "Implement the first execution item")
        .expect("preview row must exist");
    let mut row = String::new();
    for x in 0..buf.area.width {
        row.push_str(buf[(x, preview_y)].symbol());
    }
    assert!(
        row.contains('↳'),
        "preview row must carry the ↳ glyph; got row: {row:?}"
    );
}

#[test]
fn preview_line_dim_color() {
    // The preview content spans use the dim palette color.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    let preview_y = find_row_with(&buf, "Implement the first execution item")
        .expect("preview row must exist");
    // Locate the first non-whitespace cell on the preview row, then
    // walk until we hit the first letter of the outcome text. Those
    // cells should be styled with the dim color.
    let mut found_dim = false;
    for x in 0..buf.area.width {
        let cell = &buf[(x, preview_y)];
        let sym = cell.symbol();
        if sym.starts_with('I') {
            // 'I' is the first letter of "Implement …" — its color
            // must equal the palette's dim color.
            assert_eq!(
                cell.style().fg,
                Some(app.effective_palette().dim),
                "preview content cell `{}` must use dim color; got {:?}",
                sym,
                cell.style().fg
            );
            found_dim = true;
            break;
        }
    }
    assert!(found_dim, "could not locate preview content cell to test color");
}

#[test]
fn preview_line_empty_when_intent_outcome_empty() {
    // Milestone with NO intent.outcome key at all → preview row
    // carries only the ↳ prefix (no content).
    let mut data = rich_path_data();
    data["lanes"][0]["items"][1]["milestone"]
        .as_object_mut()
        .unwrap()
        .remove("intent");
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(data);
    let buf = render_buffer(&app, 140, 60);
    // M11 has empty outcome — preview row must still exist and carry
    // the ↳ glyph but no content text.
    let m11_title_y = find_row_with(&buf, "M11 — Second execution item")
        .expect("M11 title row must exist");
    let preview_y = m11_title_y + 1;
    let mut row = String::new();
    for x in 0..buf.area.width {
        row.push_str(buf[(x, preview_y)].symbol());
    }
    assert!(
        row.contains('↳'),
        "M11 preview row must carry the ↳ prefix; got row: {row:?}"
    );
    assert!(
        !row.contains("Implement the second execution item"),
        "M11 preview row must NOT carry the outcome text"
    );
}

#[test]
fn next_indicator_on_first_execution_item_only() {
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    // M10 is rank 1 → carries `◀ next`. M11 is rank 2 → does NOT.
    // M10 occupies 2 visual lines (title + preview) + a spine line
    // separator → 3 rows between M10's title and M11's title.
    let m10_y = find_row_with(&buf, "M10 — First execution item")
        .expect("M10 row must exist");
    let m11_y = find_row_with(&buf, "M11 — Second execution item")
        .expect("M11 row must exist");
    assert_eq!(
        m11_y - m10_y,
        3,
        "M10 title + preview + spine = 3 rows before M11's title"
    );
    let mut m10_row = String::new();
    for x in 0..buf.area.width {
        m10_row.push_str(buf[(x, m10_y)].symbol());
    }
    let mut m11_row = String::new();
    for x in 0..buf.area.width {
        m11_row.push_str(buf[(x, m11_y)].symbol());
    }
    assert!(
        m10_row.contains("◀ next"),
        "first execution item must carry the ◀ next indicator; row: {m10_row:?}"
    );
    assert!(
        !m11_row.contains("◀ next"),
        "second execution item must NOT carry the ◀ next indicator; row: {m11_row:?}"
    );
}

#[test]
fn next_indicator_warn_color_bold() {
    // The `◀ next` cells use the warn palette color + BOLD modifier.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    let m10_y = find_row_with(&buf, "M10 — First execution item")
        .expect("M10 row must exist");
    let mut found = false;
    for x in 0..buf.area.width {
        let cell = &buf[(x, m10_y)];
        if cell.symbol() == "◀" {
            assert_eq!(
                cell.style().fg,
                Some(app.effective_palette().warn),
                "◀ cell must use warn color; got {:?}",
                cell.style().fg
            );
            assert!(
                cell.style().add_modifier.contains(ratatui::style::Modifier::BOLD),
                "◀ cell must use BOLD modifier"
            );
            found = true;
            break;
        }
    }
    assert!(found, "could not locate ◀ cell on M10 row");
}

#[test]
fn preview_empty_intent_outcome_renders_only_prefix() {
    // M40 (grooming branch) carries a whitespace-only intent.outcome.
    // The preview row must render only the ↳ prefix (NO content,
    // NO `(no description)` placeholder).
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    // M40 lives on the grooming branch. Find its title row, then
    // check the row immediately below for `↳` (and the absence of
    // any content).
    let m40_title_y = find_row_with(&buf, "M40 — Grooming milestone")
        .expect("M40 title row must exist");
    let preview_y = m40_title_y + 1;
    let mut row = String::new();
    for x in 0..buf.area.width {
        row.push_str(buf[(x, preview_y)].symbol());
    }
    assert!(
        row.contains('↳'),
        "M40 preview row must carry the ↳ prefix; got row: {row:?}"
    );
    // No content text from the outcome (it was whitespace-only).
    assert!(
        !row.contains("M40"),
        "preview row must not echo the title text"
    );
    assert!(
        !row.contains("(no description)"),
        "preview row must not render a `(no description)` placeholder; got row: {row:?}"
    );
}

#[test]
fn no_placeholder_when_empty() {
    // Same fixture as above but inspects the WHOLE buffer for any
    // "(no description)" placeholder — none must appear.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    app.load_path_data(rich_path_data());
    let buf = render_buffer(&app, 140, 60);
    let mut full = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            full.push_str(buf[(x, y)].symbol());
        }
        full.push('\n');
    }
    assert!(
        !full.contains("(no description)"),
        "Path tab must not render `(no description)` placeholders; got:\n{full}"
    );
}
