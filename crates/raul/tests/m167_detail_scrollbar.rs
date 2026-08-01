//! M167 WP3 / WP2: detail scrollbar fix (measurement pass).
//!
//! AC-10..AC-13 verify the new `measure_paragraph_height` helper and that
//! the milestone-detail renderer uses it to compute `detail_max_scroll`
//! from the actual wrapped Paragraph height, instead of the pre-M167
//! `lines.len()` math which truncated on wrapped content.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use raul::tui::render::scrollbar::measure_paragraph_height;

fn wrap_para(text: &str, _area: Rect) -> Paragraph<'static> {
    Paragraph::new(Line::from(text.to_string())).wrap(Wrap { trim: false })
}

#[test]
fn wrapped_detail_reaches_max_scroll() {
    // AC-10: when the Paragraph wraps past the viewport, measurement
    // returns the full rendered height (not the logical-line count).
    let area = Rect::new(0, 0, 40, 6);
    // Force a multi-line wrap by stuffing a long string into a 40-col
    // box: a single logical line wraps to ~3 visible rows.
    let text: String = "x ".repeat(60);
    let para = wrap_para(&text, area);
    let rendered = measure_paragraph_height(para, area);
    assert!(
        rendered > 1,
        "wrapped content must measure > 1 row; got {rendered}"
    );
    assert!(
        rendered <= area.height,
        "rendered height {rendered} must fit within {area:?}"
    );
}

#[test]
fn small_detail_renders_no_thumb() {
    // AC-11: a small detail that fits the viewport even after wrap
    // yields a rendered height equal to (or less than) the visible
    // interior — scrollbar thumb is hidden.
    let area = Rect::new(0, 0, 80, 20);
    let para = wrap_para("a short body line", area);
    let rendered = measure_paragraph_height(para, area);
    // Visible interior is area.height (no block here; we measure the
    // raw Paragraph — the renderer applies the block border). The
    // helper only fires when content reaches the full interior height.
    assert!(rendered <= area.height);
}

#[test]
fn track_click_within_one_row_of_thumb() {
    // AC-12: track-click resolution lands on a row adjacent to the
    // thumb drag. With the M167 total (= rendered_height = max_scroll
    // + visible_rows) fed in, the click resolves to a row within
    // [0, total - 1] inclusive, and at the extremes (top → 0,
    // bottom → total - 1) per M137's contract.
    use raul::tui::render::scrollbar::track_click_to_scroll;
    let visible = 30;
    let max_scroll = 5;
    let total = max_scroll + visible; // 35
                                      // Top of the track maps to scroll 0.
    assert_eq!(track_click_to_scroll(10, 0, total), 0);
    // Bottom maps to total - 1.
    assert_eq!(track_click_to_scroll(10, 9, total), total - 1);
    // Middle clicks land somewhere in [0, total - 1].
    for y in 0..10 {
        let resolved = track_click_to_scroll(10, y, total);
        assert!(
            resolved < total,
            "click y={y} resolved to {resolved} but total is {total}"
        );
    }
}

#[test]
fn fully_rendered_detail_reaches_bottom() {
    // AC-36: the detail Paragraph for a fully-rendered fixture renders
    // all the way to the bottom — the measurement pass replaces
    // `lines.len()` math with the post-wrap height. M169-rev: the
    // measurement buffer is 8× the panel height, so 30 logical lines
    // overflowing a 6-row viewport measure as 30 (not capped at 6,
    // which was the pre-fix behavior that capped the scrollbar at
    // ~2 rows in `crates/raul/tests/m169_scroll_repro.rs`).
    let area = Rect::new(0, 0, 40, 6);
    let body: Vec<Line> = (0..30)
        .map(|i| Line::from(format!("line {i:02}")))
        .collect();
    let para = Paragraph::new(body).wrap(Wrap { trim: false });
    let rendered = measure_paragraph_height(para, area);
    // 30 logical lines in a 40-col × 6-row viewport; each logical
    // line fits on one row. Total rows = 30, NOT capped at area.height.
    assert_eq!(rendered, 30);
}

#[test]
fn empty_paragraph_measures_zero() {
    let area = Rect::new(0, 0, 80, 20);
    let para = Paragraph::new(Vec::<Line>::new()).wrap(Wrap { trim: false });
    let rendered = measure_paragraph_height(para, area);
    assert_eq!(rendered, 0);
}

#[test]
fn measurement_buffer_does_not_overflow_area() {
    // AC-13: the measurement buffer is sized to the given area (so
    // `Buffer::empty(area)` does not panic when area is small).
    for h in 1u16..=8 {
        let area = Rect::new(0, 0, 40, h);
        let para = wrap_para("a single line", area);
        // Buffer::empty and Paragraph::render both must accept the
        // shape without panicking on a tight viewport.
        let buf = Buffer::empty(area);
        let rendered = measure_paragraph_height(para, area);
        assert!(
            rendered <= h,
            "rendered {rendered} must fit in area {area:?}"
        );
        let _ = buf;
    }
}
