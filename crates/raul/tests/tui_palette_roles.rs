//! M244: the quiet layering roles actually land in rendered cells.
//!
//! `theme.rs` pins the *values*; this file proves the renderers
//! *consume* them. Each test renders through `TestBackend` and
//! asserts on the buffer's cell styles, so a renderer that quietly
//! falls back to `accent` / a hardcoded color fails here.
//!
//! Contract under test:
//!   - selected list / board row → `surface_2` background + `focus_ring`
//!   - overlay / modal surfaces    → `surface_1` background
//!   - detail AC rows              → `surface_2` background
//!   - chrome (header / footer)    → `surface_1` background
//!   - the focused tab             → `focus_ring` background

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane, MilestoneSummary};
use raul::tui::mode::{Mode, SearchInputState};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

// --- helpers ---------------------------------------------------------------

fn ms(id: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: format!("title-{id}"),
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
    }
}

fn milestones_app() -> App {
    let mut app = App::new();
    app.load_milestones(vec![ms("01"), ms("02"), ms("03")]);
    app.select_lane(Lane::Milestones);
    app
}

fn draw(app: &App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Text of one visual row.
fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area().width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
}

/// First visual row containing `needle`, or `None`.
fn row_with(buf: &Buffer, needle: &str) -> Option<u16> {
    (0..buf.area().height).find(|&y| row_text(buf, y).contains(needle))
}

fn has_bg(buf: &Buffer, y: u16, color: Color) -> bool {
    (0..buf.area().width).any(|x| buf[(x, y)].style().bg == Some(color))
}

/// Every cell in the buffer, in row-major order.
fn cells(buf: &Buffer) -> impl Iterator<Item = &ratatui::buffer::Cell> + '_ {
    let area = *buf.area();
    (0..area.height).flat_map(move |y| (0..area.width).map(move |x| &buf[(x, y)]))
}

/// Count cells rendering `symbol` with `color` as their foreground.
fn fg_cells(buf: &Buffer, symbol: &str, color: Color) -> usize {
    cells(buf)
        .filter(|c| c.symbol() == symbol && c.style().fg == Some(color))
        .count()
}

/// Count cells whose background is `color`.
fn bg_cells(buf: &Buffer, color: Color) -> usize {
    cells(buf).filter(|c| c.style().bg == Some(color)).count()
}

// --- list / board selection -------------------------------------------------

#[test]
fn selected_row_uses_surface_2_bg_and_focus_ring_marker() {
    let mut app = milestones_app();
    app.selected_index = 2;
    let palette = app.effective_palette();
    let buf = draw(&app, 120, 24);

    let y = row_with(&buf, "M03").expect("selected row must render its id");
    assert!(
        has_bg(&buf, y, palette.surface_2),
        "the selected list row must be painted with surface_2"
    );
    let marker_hits = fg_cells(&buf, "▌", palette.focus_ring);
    assert!(
        marker_hits >= 1,
        "the selected list row must carry a focus_ring marker glyph"
    );
    assert_ne!(
        palette.surface_2, palette.accent,
        "surface_2 must not collapse onto accent"
    );
}

#[test]
fn board_selected_box_uses_surface_2_bg_and_focus_ring_border() {
    let app = milestones_app();
    let palette = app.effective_palette();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    terminal
        .draw(|frame| {
            render::board::render_board(frame, &app, area, 0, Some("02"));
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();

    let y = row_with(&buf, "M02").expect("the selected box must render its id");
    assert!(
        has_bg(&buf, y, palette.surface_2),
        "the selected board box must be filled with surface_2"
    );
    // The box border is the `focus_ring` marker.
    let ring_cells = cells(&buf)
        .filter(|c| {
            matches!(
                c.symbol(),
                "─" | "│" | "╭" | "╮" | "╰" | "╯" | "┌" | "┐" | "└" | "┘"
            ) && c.style().fg == Some(palette.focus_ring)
        })
        .count();
    assert!(
        ring_cells > 0,
        "the selected board box border must be drawn in focus_ring"
    );
}

// --- overlays / modals -----------------------------------------------------

#[test]
fn overlay_modal_backdrop_uses_surface_1_bg() {
    let mut app = milestones_app();
    app.active_mode = Mode::SearchInput(SearchInputState {
        buffer: "needle".into(),
        prior: String::new(),
    });
    let palette = app.effective_palette();
    let buf = draw(&app, 120, 30);

    let y = row_with(&buf, "Search").expect("the search overlay must render");
    assert!(
        has_bg(&buf, y, palette.surface_1),
        "an overlay / modal surface must be painted with surface_1"
    );
}

// --- detail view -----------------------------------------------------------

fn detail_app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![ms("86")]);
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "86", "title": "Layered theme", "effort": "M", "risk": "low",
            "change_kind": "", "priority": "normal", "depends_on": [],
            "lifecycle": "in-progress", "lifecycle_at": "2026-07-01T00:00:00Z"
        },
        "intent": { "outcome": "Quiet layering roles" },
        "problem": { "description": "Layers collapse onto accent" },
        "scope": { "in_scope": ["roles"], "out_of_scope": ["picker"] },
        "acceptance_criteria": [
            { "id": "AC-01", "description": "roles land in cells", "status": "pending",
              "verification": "cargo nextest run -p raul" }
        ],
        "steps": []
    }));
    app.content = ContentState::MilestoneDetail;
    app.selected_milestone_id = Some("86".into());
    app
}

#[test]
fn detail_ac_row_uses_surface_2_bg() {
    let app = detail_app();
    let palette = app.effective_palette();
    let buf = draw(&app, 120, 120);

    let y = row_with(&buf, "AC-01").expect("the AC row must render its id");
    assert!(
        has_bg(&buf, y, palette.surface_2),
        "a detail AC row must be painted with surface_2"
    );
}

// --- chrome ----------------------------------------------------------------

#[test]
fn chrome_header_and_footer_use_surface_1_bg() {
    let app = milestones_app();
    let palette = app.effective_palette();
    let buf = draw(&app, 120, 24);

    // Header row carries the R.A.U.L. title; the footer is the last
    // row of the frame.
    let header_y = row_with(&buf, "R.A.U.L.").expect("the header must render");
    assert!(
        has_bg(&buf, header_y, palette.surface_1),
        "the header band must be painted with surface_1"
    );
    let footer_y = buf.area().height - 1;
    assert!(
        has_bg(&buf, footer_y, palette.surface_1),
        "the footer band must be painted with surface_1"
    );
}

// --- tab bar ---------------------------------------------------------------

#[test]
fn tab_focused_tab_uses_focus_ring_bg() {
    let app = milestones_app();
    let palette = app.effective_palette();
    let buf = draw(&app, 120, 24);

    let ring_cells = bg_cells(&buf, palette.focus_ring);
    assert!(
        ring_cells > 0,
        "the focused tab must be filled with focus_ring"
    );
    assert_eq!(
        bg_cells(&buf, palette.accent),
        0,
        "no surface may still be filled with the accent after the role split"
    );
}
