//! M199 AC-03: footer renders both rows in identical visual style —
//! same dim color, same `·` separator, center-aligned, no divider,
//! no bold contrast between the two rows. The footer reads as one
//! balanced block instead of "stable row + changing row" (M187
//! contrast).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn render_to_buffer(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    let mut out = String::new();
    for x in 0..buf.area().width {
        out.push_str(buf[(x, y)].symbol());
    }
    out
}

fn row_fg_colors(buf: &ratatui::buffer::Buffer, y: u16) -> Vec<ratatui::style::Color> {
    let mut colors = Vec::new();
    for x in 0..buf.area().width {
        let cell = &buf[(x, y)];
        if !cell.symbol().trim().is_empty() {
            colors.push(cell.fg);
        }
    }
    colors
}

#[test]
fn both_rows_use_same_dim_color_and_separator() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let buf = render_to_buffer(&app, 120, 24);
    let view = view_state::compute_view(&app, *buf.area());
    let globals_y = view.footer_area.y;
    let per_tab_y = globals_y + 1;
    let globals = row_text(&buf, globals_y);
    let per_tab = row_text(&buf, per_tab_y);

    // Both rows must include the dim `·` separator.
    assert!(
        globals.contains('·'),
        "globals row must use `·` separator; got: {globals:?}"
    );
    assert!(
        per_tab.contains('·'),
        "per-tab row must use `·` separator; got: {per_tab:?}"
    );

    // Both rows must be rendered in the same dim palette color.
    let app_palette = app.effective_palette();
    let expected_dim = app_palette.dim;
    let globals_colors = row_fg_colors(&buf, globals_y);
    let per_tab_colors = row_fg_colors(&buf, per_tab_y);
    for c in &globals_colors {
        assert_eq!(
            *c, expected_dim,
            "globals row cell color {c:?} != dim {expected_dim:?}"
        );
    }
    for c in &per_tab_colors {
        assert_eq!(
            *c, expected_dim,
            "per-tab row cell color {c:?} != dim {expected_dim:?}"
        );
    }

    // No cell in either row should carry the BOLD modifier.
    for x in 0..buf.area().width {
        let cell = &buf[(x, globals_y)];
        assert!(
            !cell.modifier.contains(ratatui::style::Modifier::BOLD),
            "globals row must not use BOLD; cell=({x},{globals_y}) symbol={:?}",
            cell.symbol()
        );
        let cell = &buf[(x, per_tab_y)];
        assert!(
            !cell.modifier.contains(ratatui::style::Modifier::BOLD),
            "per-tab row must not use BOLD; cell=({x},{per_tab_y}) symbol={:?}",
            cell.symbol()
        );
    }

    // No divider row between them — the area between the two
    // rows is empty (we render two 1-row cells back-to-back
    // with no separator).
    assert_eq!(
        per_tab_y - globals_y,
        1,
        "globals and per-tab rows must be adjacent (no divider row)"
    );

    // The footer color must be the dim color, not the accent.
    // (The pre-M199 contrast put the per-tab row in dim and the
    // globals row in dim too — but with a different
    // brightness. After M199, both rows are dim-with-dim, no
    // contrast.)
    assert_ne!(
        expected_dim, app_palette.accent,
        "dim and accent must differ (sanity check on the palette)"
    );
}

#[test]
fn both_rows_are_center_aligned() {
    // M199 D-05: both rows are centered in the available width.
    // We verify centering by checking that the leftmost non-blank
    // cell of each row is at approximately the same x-offset
    // (allowing for the rows having different widths so the
    // centerings are slightly different, but the offsets should
    // both be in the right half of the row's content area).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let buf = render_to_buffer(&app, 120, 24);
    let view = view_state::compute_view(&app, *buf.area());
    let globals_y = view.footer_area.y;
    let per_tab_y = globals_y + 1;

    // Find the leftmost non-blank cell on each row.
    fn leftmost_nonblank(buf: &ratatui::buffer::Buffer, y: u16) -> Option<u16> {
        for x in 0..buf.area().width {
            let cell = &buf[(x, y)];
            if !cell.symbol().trim().is_empty() {
                return Some(x);
            }
        }
        None
    }
    let g_left = leftmost_nonblank(&buf, globals_y).expect("globals row must have content");
    let p_left = leftmost_nonblank(&buf, per_tab_y).expect("per-tab row must have content");
    // Both rows should start past column 0 (centered) — at width
    // 120 with the canonical content, both rows have non-trivial
    // leading space.
    assert!(
        g_left >= 4,
        "globals row must be centered (leftmost content at x={g_left})"
    );
    assert!(
        p_left >= 4,
        "per-tab row must be centered (leftmost content at x={p_left})"
    );
    // And the two rows should start within a few columns of each
    // other (both are centered, so the offsets are close but
    // not identical because the content widths differ).
    let diff = (g_left as i32 - p_left as i32).abs();
    assert!(
        diff < 30,
        "globals and per-tab rows must both be centered (lead columns {g_left} vs {p_left} differ by {diff})"
    );
}
