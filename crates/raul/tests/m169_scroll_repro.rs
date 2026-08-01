//! M169-rev scrollbar fix — repro tests for the bug where the milestone
//! detail scrollbar was capped at ~2 rows because
//! `measure_paragraph_height` rendered the Paragraph into a buffer the
//! size of the panel, so the bottom border was the last non-blank row
//! and the function returned the panel height instead of the actual
//! content height.
//!
//! Pre-fix: pressing Down on a tall milestone detail was limited to 2
//! steps (panel.height - visible = 20 - 18 = 2).
//! Post-fix: the function renders into an 8× panel-height buffer,
//! detects the bottom-border row by walking top-down looking for a row
//! that's entirely box-drawing glyphs, and returns the inner content
//! row count.

use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use raul::tui::render::scrollbar::measure_paragraph_height;

#[test]
fn tall_panel_30_lines_returns_actual_content_height() {
    // 79-wide × 20-tall panel (matches the milestone-detail layout: full
    // content_area.width minus the 1-col scrollbar gutter, panel height
    // includes Borders::ALL on top + bottom).
    let detail_area = Rect::new(0, 0, 79, 20);
    // 30 logical lines, each ~40 chars. Inner width = 79 - 2 = 77 cols,
    // so each line fits on 1 visual row → 30 content rows.
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
    // The fix: 30 content rows. The pre-fix code returned 20 (panel
    // height), capping detail_max_scroll at 20 - 18 = 2.
    assert_eq!(
        measured, 30,
        "measure_paragraph_height must return the inner content row count, not the panel height"
    );
    // detail_max_scroll = measured - visible = 30 - 18 = 12.
    let visible = detail_area.height.saturating_sub(2);
    let max_scroll = measured.saturating_sub(visible);
    assert_eq!(max_scroll, 12);
    assert!(
        max_scroll > 5,
        "user must be able to scroll past 5 rows on a tall detail"
    );
}

#[test]
fn tall_panel_thin_border_returns_actual_content_height() {
    // Same scenario but with BorderType::Plain (the default) — the
    // bottom border is `─` (U+2500) instead of `━` (U+2501).
    let detail_area = Rect::new(0, 0, 79, 20);
    let lines: Vec<Line> = (0..30)
        .map(|i| Line::from(format!("line {i:02} {}", "x".repeat(30))))
        .collect();
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("M1 Detail")
                .border_type(BorderType::Plain),
        )
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    assert_eq!(measured, 30);
}

#[test]
fn short_content_returns_visible_minus_borders() {
    // Content fits within the panel. measured = content rows (≤ visible).
    let detail_area = Rect::new(0, 0, 79, 20);
    let lines: Vec<Line> = (0..5)
        .map(|i| Line::from(format!("short line {i}")))
        .collect();
    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    assert_eq!(measured, 5, "short content measures just its row count");
}

#[test]
fn wrapped_long_line_counts_post_wrap_rows() {
    // A single 200-char line in a 77-col inner area wraps to ~3 rows.
    let detail_area = Rect::new(0, 0, 79, 20);
    let line = Line::from("x ".repeat(100)); // 200 chars, wraps to ~3 rows
    let para = Paragraph::new(vec![line])
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    assert!(
        (2..=4).contains(&measured),
        "wrapped long line should measure 2-4 rows; got {measured}"
    );
}

#[test]
fn zero_area_returns_zero() {
    let para = Paragraph::new("anything").wrap(Wrap { trim: false });
    assert_eq!(
        measure_paragraph_height(para.clone(), Rect::new(0, 0, 0, 0)),
        0
    );
    assert_eq!(measure_paragraph_height(para, Rect::new(0, 0, 79, 0)), 0);
}

#[test]
fn empty_paragraph_measures_zero() {
    let detail_area = Rect::new(0, 0, 79, 20);
    let para = Paragraph::new(Vec::<Line>::new())
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    assert_eq!(measured, 0);
}

#[test]
fn measure_counts_blank_separator_lines_between_content() {
    // M169-rev2 user report: the previous implementation counted
    // only rows with non-blank non-border cells, which under-counted
    // any Paragraph that had `Line::from("")` blank separators
    // between sections. The milestone-detail renderer adds 17 such
    // blank lines; the user could never scroll past them because
    // `detail_max_scroll` was set too low.
    //
    // Pin: a Paragraph with 5 logical lines (3 non-blank + 2 blanks
    // — one inter-section separator, one trailing) measures 4 rows.
    // The 3 non-blank lines are reachable; the inter-section blank
    // is reachable; the trailing blank is indistinguishable from
    // rows beyond the Paragraph (both render as side-borders + spaces)
    // and is intentionally not counted — the user sees all
    // meaningful content, which is what "reaching the bottom" means.
    let detail_area = Rect::new(0, 0, 79, 20);
    let lines: Vec<Line> = vec![
        Line::from("line 0"),
        Line::from("line 1"),
        Line::from(""), // separator between sections — reachable
        Line::from("line 3"),
        Line::from(""), // trailing separator — indistinguishable from
                        // rows beyond the Paragraph, not counted
    ];
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("D")
                .border_type(BorderType::Thick),
        )
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    assert_eq!(
        measured, 4,
        "blank separator lines BETWEEN content must count toward the scroll cap (got {measured}, want 4)"
    );
}

#[test]
fn measure_reaches_bottom_of_realistic_milestone_with_blanks() {
    // M169-rev2 user report integration test: build a Paragraph
    // that mirrors the milestone-detail renderer's blank-line
    // pattern (multiple sections separated by `Line::from("")`)
    // and assert the measured height covers every non-blank row.
    // Pre-fix this would under-count by the number of inter-section
    // blanks, capping `detail_max_scroll` too low and leaving the
    // user unable to scroll to the bottom.
    let detail_area = Rect::new(0, 0, 79, 20);
    let mut lines: Vec<Line> = Vec::new();
    for section in 0..6 {
        lines.push(Line::from(format!("[section {section}] header")));
        for i in 0..4 {
            lines.push(Line::from(format!("  body {section}.{i}")));
        }
        lines.push(Line::from("")); // separator
    }
    // 6 sections × (1 header + 4 body + 1 blank) = 36 lines.
    assert_eq!(lines.len(), 36);
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("M")
                .border_type(BorderType::Thick),
        )
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    let measured = measure_paragraph_height(para, detail_area);
    // Expect 35 (36 lines minus the trailing blank, per the
    // "trailing blank indistinguishable from rows beyond the
    // Paragraph" rationale above).
    assert_eq!(
        measured, 35,
        "realistic milestone with 5 inter-section blanks must reach every non-blank row (got {measured}, want 35)"
    );
}

#[test]
fn measure_returns_full_content_height_regardless_of_scroll_offset() {
    // M169-rev sub-agent review H1: the renderer builds a Paragraph
    // whose `.scroll()` is set to the user's current scroll offset,
    // then feeds it to `measure_paragraph_height`. Pre-fix this
    // returned `(total - scroll_offset)` instead of `total`, so
    // `detail_max_scroll` was recomputed to a too-low cap as soon as
    // the user scrolled past the visible boundary, jamming the
    // scrollbar again.
    //
    // The fix lands in `milestone_detail.rs`: the measurement Paragraph
    // is built *without* `.scroll()`. We pin the invariant here by
    // asserting `measure_paragraph_height` returns the same value
    // for `scroll=(0,0)` and `scroll=(k,0)` on the same body — the
    // helper should report the **full** content extent, not the
    // remaining-extent-below-the-viewport.
    let detail_area = Rect::new(0, 0, 79, 20);
    let lines: Vec<Line> = (0..30)
        .map(|i| Line::from(format!("line {i:02} {}", "x".repeat(30))))
        .collect();
    let mk = |scroll: (u16, u16)| -> Paragraph<'_> {
        Paragraph::new(lines.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("M1 Detail")
                    .border_type(BorderType::Thick),
            )
            .wrap(Wrap { trim: false })
            .scroll(scroll)
    };
    let m0 = measure_paragraph_height(mk((0, 0)), detail_area);
    let m5 = measure_paragraph_height(mk((5, 0)), detail_area);
    let m15 = measure_paragraph_height(mk((15, 0)), detail_area);
    assert_eq!(
        m0, m5,
        "measurement must not change with scroll offset (got {m0} at scroll=0 vs {m5} at scroll=5)"
    );
    assert_eq!(
        m0, m15,
        "measurement must not change with scroll offset (got {m0} at scroll=0 vs {m15} at scroll=15)"
    );
    assert_eq!(m0, 30, "full content height is 30 rows");
}
