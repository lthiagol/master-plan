//! M172 S3: Board view — compact milestone boxes arranged in a
//! grid. Each box carries the milestone id, a 1-line title, and
//! a status-color fill. Rounded borders + ratatui's `ScrollbarState`
//! for scroll chrome.
//!
//! The Board view is rendered as a side panel that pins to the
//! bottom of the Overview lane (or to a dedicated `Board` lane
//! when one is added in a future milestone). It's intentionally a
//! renderer-only addition — no new data model, no new lane enum
//! variant; existing `App::milestones` is the source.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

use crate::tui::app::{App, MilestoneSummary};
use crate::tui::palette;

/// Width of one milestone box. Auto-fits the viewport via the
/// `box_width_for` helper — the constant here is the MIN width
/// when the terminal is too narrow to fit a useful column.
const MIN_BOX_WIDTH: u16 = 18;
const MAX_BOX_WIDTH: u16 = 32;

/// Compute the per-column width for the milestone grid given the
/// available area width. Targets 3-4 columns at 80-col, 5-6 at
/// 120-col. Each column is between MIN and MAX.
pub fn box_width_for(area_width: u16) -> u16 {
    // Aim for ~5 boxes across at 120-col, ~3 at 80-col.
    let target_columns = ((area_width as usize) / 28).max(3);
    let per = area_width / target_columns as u16;
    per.clamp(MIN_BOX_WIDTH, MAX_BOX_WIDTH)
}

/// Render the Board view as a row of boxes with scrollbar chrome.
/// `scroll` is the current vertical scroll offset; the function
/// updates `app.path_max_scroll` so the scrollbar can pick it up.
pub fn render_board(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    scroll: usize,
    selected: Option<&str>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Board ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let milestones: Vec<&MilestoneSummary> = app.visible_milestones();
    if milestones.is_empty() {
        let para = Paragraph::new("(no milestones — press r to refresh)")
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(app.effective_palette().warn));
        frame.render_widget(para, inner);
        return;
    }

    let palette = app.effective_palette();
    let box_w = box_width_for(inner.width);
    let inner_w = inner.width;
    let cols = (inner_w / box_w).max(1) as usize;
    // Reserve 1 column on the right for the scrollbar gutter (when
    // there's room) and 5 rows of vertical space per box so the
    // rounded-border (top + bottom = 2 rows) leaves 3 inner rows
    // for id + title + lifecycle.
    let scrollbar_gutter = if inner.width >= 12 { 1 } else { 0 };
    let box_h: u16 = 5;
    let rows_visible = (inner.height / box_h).max(1) as usize;
    let total_rows = milestones.len().div_ceil(cols);
    let max_scroll = total_rows.saturating_sub(rows_visible);
    let clamped = scroll.min(max_scroll);
    app.path_max_scroll.set(max_scroll as u16);

    // Build the column layout: each column is `box_w` wide. We
    // don't include the scrollbar gutter in this split (the
    // scrollbar is rendered separately into `scrollbar_col` below).
    let column_constraints: Vec<Constraint> =
        (0..cols).map(|_| Constraint::Length(box_w)).collect();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints)
        .split(Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(scrollbar_gutter),
            height: inner.height,
        });
    let scrollbar_col = if scrollbar_gutter > 0 {
        Rect {
            x: inner.x + inner.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        }
    } else {
        // No room for a gutter — drop the scrollbar entirely.
        Rect::default()
    };

    for (i, ms) in milestones.iter().enumerate().skip(clamped * cols) {
        let offset = i - clamped * cols;
        let row_idx = offset / cols;
        let col_idx = offset % cols;
        if row_idx >= rows_visible {
            break;
        }
        let col_rect = columns.get(col_idx).copied().unwrap_or(inner);
        let cell_rect = Rect {
            x: col_rect.x,
            y: col_rect.y + (row_idx as u16 * box_h),
            width: box_w.saturating_sub(2),
            height: box_h,
        };
        let bg = lifecycle_color(&ms.lifecycle, palette);
        let is_selected = selected.map(|s| s == ms.id).unwrap_or(false);
        render_box(frame, ms, cell_rect, bg, is_selected, palette);
    }

    // Scrollbar via ratatui's ScrollbarState (AC-03 contract — the
    // lane scrollbar uses ScrollbarState instead of the homerolled
    // chrome.rs scrollbar).
    let mut sb_state = ScrollbarState::new(total_rows).position(clamped);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█"),
        scrollbar_col,
        &mut sb_state,
    );
}

fn render_box(
    frame: &mut Frame,
    ms: &MilestoneSummary,
    area: Rect,
    bg: ratatui::style::Color,
    is_selected: bool,
    palette: &crate::theme::Palette,
) {
    if area.width < 6 || area.height < 3 {
        return;
    }
    // M172 S4 audit (F-04): the AC-04 grep check requires zero
    // direct color literals outside `palette.rs`. Use the
    // `on_accent_fg` / `caret_block` / `selection_border` helpers
    // so the foreground + selection colors flow through the palette.
    // M172 external review (F-16): pass the *effective* palette (not
    // a hardcoded MOCHA) so on_accent_fg honors the accent/Reset branch
    // for monochrome themes instead of forcing a black foreground.
    let border_color = if is_selected {
        palette::selection_border(palette)
    } else {
        bg
    };
    let on_bg = palette::on_accent_fg(palette);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        format!("M{}", ms.id),
        Style::default().fg(on_bg).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        truncate_title(&ms.title, inner.width as usize),
        Style::default().fg(on_bg),
    )]));
    lines.push(Line::from(vec![Span::styled(
        format!("[{}]", ms.lifecycle),
        Style::default().fg(on_bg).add_modifier(Modifier::ITALIC),
    )]));
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        return title.to_string();
    }
    // M172 external review (F-12): when truncating, reserve 1 char
    // for the ellipsis. The keep budget is `max_chars - 2` (so a
    // 6-char budget produces a 5-char string: 4 visible + 1 ellipsis).
    // The truncated form therefore reads as "abcd…" (5 chars) — 1
    // short of `max_chars` so the ellipsis is unambiguous rather than
    // an off-by-one visual artifact. Pinning in tests.
    let keep = max_chars.saturating_sub(2);
    let mut s: String = title.chars().take(keep).collect();
    s.push('…');
    s
}

/// M172 S4 contract: the Board view's status-color fill MUST come
/// from `effective_palette()`, not direct `Color::*` literals.
pub fn lifecycle_color(lifecycle: &str, palette: &crate::theme::Palette) -> ratatui::style::Color {
    match lifecycle {
        "complete" => palette.success,
        "in-progress" => palette.accent,
        "blocked" => palette.danger,
        "approved" | "ready" => palette.warn,
        _ => palette.dim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_box_width_for_clamps_to_min_max() {
        assert!(box_width_for(20) >= MIN_BOX_WIDTH);
        assert!(box_width_for(200) <= MAX_BOX_WIDTH);
    }

    #[test]
    fn board_box_width_scales_with_area() {
        let narrow = box_width_for(80);
        let wide = box_width_for(160);
        assert!(wide >= narrow, "wide areas should fit ≥ narrow columns");
    }

    #[test]
    fn board_truncate_title_preserves_chars() {
        assert_eq!(truncate_title("hello", 10), "hello");
        assert_eq!(truncate_title("hello world", 6), "hell…");
        assert_eq!(truncate_title("", 5), "");
    }
}
