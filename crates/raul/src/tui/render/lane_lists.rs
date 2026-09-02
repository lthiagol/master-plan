use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::tui::app::{App, Lane, MilestoneSummary, SortKey};
use crate::tui::progress;
use crate::tui::view_state::{ScrollableId, ViewState};

fn header_cell(label: &str, is_active_sort: bool, header_style: Style) -> Cell<'static> {
    if is_active_sort {
        Cell::new(Span::styled(format!("{label} ▼"), header_style))
    } else {
        Cell::new(Span::styled(label.to_string(), header_style))
    }
}

pub(super) fn render_lane_list(frame: &mut Frame, app: &App, area: Rect, view: &ViewState) {
    match app.active_lane {
        Lane::Milestones => render_milestones_table(frame, app, area, view),
        Lane::Backlog | Lane::Ideas => render_backlog_list(frame, app, area, view),
        // M179 S3-S6: the Watch lane has its own renderer
        // (picker + lifecycle graph + compact queue + log +
        // active-pane output). The renderer does not use the
        // legacy `selected_index` + scrollbar path; the
        // picker / queue cursors live on `app.watch`.
        Lane::Watch => super::watch::render_watch_lane(frame, app, area),
        _ => {
            let msg = Paragraph::new("")
                .block(Block::default().borders(Borders::ALL).title(""))
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
        }
    }
}

pub(super) fn render_backlog_detail(frame: &mut Frame, app: &App, area: Rect) {
    let id = app.selected_backlog_id.as_deref().unwrap_or("?");
    let item = app.backlog.iter().find(|b| b.id == id);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        format!("{} {}", crate::lanes::LANE_BACKLOG, id),
        Style::default()
            .fg(app.effective_palette().accent)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    if let Some(b) = item {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&b.title),
        ]));
        if !b.priority.is_empty() && b.priority != "?" {
            lines.push(Line::from(vec![
                Span::styled("Priority: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&b.priority),
            ]));
        }
        if !b.status.is_empty() && b.status != "?" {
            lines.push(Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&b.status),
            ]));
        }
        if !b.resolution.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    "Resolution: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&b.resolution),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Item not found in loaded backlog.",
            Style::default().fg(app.effective_palette().dim),
        )));
    }

    app.detail_max_scroll
        .set((lines.len() as u16).saturating_sub(area.height.saturating_sub(2)));
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{} Detail", crate::lanes::LANE_BACKLOG)),
        )
        .scroll((app.detail_scroll, 0));
    frame.render_widget(para, area);
}

pub(super) fn render_backlog_list(frame: &mut Frame, app: &App, area: Rect, view: &ViewState) {
    let visible = app.visible_backlog();
    if visible.is_empty() {
        let msg: std::borrow::Cow<'_, str> = if !app.lane_search_term().is_empty() {
            // M186 F-03: empty result with an active search.
            std::borrow::Cow::Owned(format!(
                "No matches for /{} — press Esc to clear.",
                app.lane_search_term()
            ))
        } else if app.hide_done && !app.backlog.is_empty() {
            std::borrow::Cow::Borrowed("All backlog items resolved — press h to show hidden.")
        } else {
            std::borrow::Cow::Owned(format!("{} is empty.", crate::lanes::LANE_BACKLOG))
        };
        let para = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(crate::lanes::LANE_BACKLOG),
            )
            .style(Style::default().fg(app.effective_palette().warn))
            .alignment(Alignment::Center);
        frame.render_widget(para, area);
        return;
    }
    // M137+: scroll offset from view; slice visible backlog so the window
    // tracks selected_index (same pattern as milestones table).
    let scroll = view
        .scrollbar_rects
        .iter()
        .find(|hit| matches!(hit.id, ScrollableId::BacklogList))
        .map(|hit| hit.scroll)
        .unwrap_or(0);

    let header_style = Style::default()
        .fg(app.effective_palette().accent)
        .add_modifier(Modifier::BOLD);
    let active_sort = app.lane_sort_key(app.active_lane);
    let header = Row::new(vec![
        header_cell("ID", active_sort == SortKey::Id, header_style),
        header_cell("Title", active_sort == SortKey::Title, header_style),
        header_cell("Priority", active_sort == SortKey::Priority, header_style),
        header_cell("Status", active_sort == SortKey::Status, header_style),
    ])
    .style(header_style)
    .height(1);

    // Same visual pattern as milestones: fixed side columns, Title fills.
    const ID_W: u16 = 10;
    const PRIORITY_W: u16 = 10;
    const STATUS_W: u16 = 12;
    const COL_SPACING: u16 = 1;
    // M203 S6: in compact mode the title column narrows further; the
    // preview text wraps when its 80-char budget would clip past the
    // title. We compute the actual title column width and reserve 80
    // chars for the preview only when the title column is wider; on
    // narrow panes the preview truncates earlier (handled inside the
    // title cell at render time).
    let title_w = list_title_col_width(area.width, ID_W, PRIORITY_W, STATUS_W, COL_SPACING);

    let mut rows: Vec<Row> = Vec::new();
    for (i, b) in visible.iter().enumerate().skip(scroll) {
        // Selected rows use one row-wide highlight; unselected rows retain
        // semantic per-cell colors.
        let is_selected = i == app.selected_index;
        let priority = if b.priority.is_empty() {
            "—".to_string()
        } else {
            b.priority.clone()
        };
        let status = if b.status.is_empty() || b.status == "?" {
            if b.resolution.is_empty() {
                "—".to_string()
            } else {
                b.resolution.clone()
            }
        } else {
            b.status.clone()
        };
        let pri_style = priority_style(&priority, app);

        // M203 S5: each logical row renders as 2 visual rows. The
        // title cell holds line 1 = bold title, line 2 = dim
        // preview with a `↳` arrow prefix. The preview budget is
        // ~80 chars but shrinks with the title column on narrow
        // panes (compact mode).
        let title_cell = build_title_cell(b, title_w, is_selected, app);

        // Title cell with both lines.
        let title_cell = match title_cell {
            Some(c) => c,
            None => Cell::from(Line::from(Span::styled(
                String::new(),
                Style::default().fg(app.effective_palette().foreground),
            ))),
        };

        // Side cells: ID, Priority, Status — single line. The Table
        // row height is 2 so the second visual line below the side
        // cells stays empty (matches the title cell's preview).
        let id_cell = if is_selected {
            Cell::from(Span::styled(
                b.id.clone(),
                Style::default().fg(crate::tui::palette::on_accent_fg(app.effective_palette())),
            ))
        } else {
            Cell::from(Span::styled(
                b.id.clone(),
                Style::default().fg(app.effective_palette().foreground),
            ))
        };
        let pri_cell = Cell::from(Span::styled(priority, pri_style));
        let status_cell = if is_selected {
            Cell::from(Span::styled(
                status.clone(),
                Style::default().fg(app.effective_palette().foreground),
            ))
        } else {
            Cell::from(Span::styled(
                status.clone(),
                Style::default().fg(app.effective_palette().dim),
            ))
        };

        let row =
            Row::new(vec![id_cell, title_cell, pri_cell, status_cell]).height(BACKLOG_ROW_HEIGHT);
        let row = if is_selected {
            row.style(
                Style::default()
                    .fg(crate::tui::palette::on_accent_fg(app.effective_palette()))
                    .bg(app.effective_palette().accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            row
        };
        rows.push(row);
    }

    let widths = [
        Constraint::Length(ID_W),
        Constraint::Fill(1),
        Constraint::Length(PRIORITY_W),
        Constraint::Length(STATUS_W),
    ];

    let title = if app.hide_done {
        format!(
            " {} ({}/{}) ",
            crate::lanes::LANE_BACKLOG,
            visible.len(),
            app.backlog.len()
        )
    } else {
        format!(" {} ({}) ", crate::lanes::LANE_BACKLOG, visible.len())
    };

    let table = Table::new(rows, widths)
        .column_spacing(COL_SPACING)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_type(BorderType::Plain),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    // Clamp stale selection indices after filtering changes the visible list.
    let clamped = app.selected_index.min(visible.len().saturating_sub(1));
    let table_selected = clamped.saturating_sub(scroll);
    frame.render_stateful_widget(
        table,
        area,
        &mut ratatui::widgets::TableState::default().with_selected(Some(table_selected)),
    );
}

/// M203 S5: visual height (in cell lines) of a single logical backlog
/// row. The title is on line 1, the preview on line 2 — same shape in
/// compact mode (S6). The hit-area + scrollbar accounting in
/// `view_state::compute_backlog_list_rects` divides the available data
/// height by this constant.
pub(super) const BACKLOG_ROW_HEIGHT: u16 = 2;

/// Build the title cell for one backlog row. The cell carries two
/// visual lines: line 1 = the bold title; line 2 = the dim preview
/// prefixed with `↳`. The second line is left empty when the preview
/// is empty.
///
/// **Why we truncate tightly:** the title column width is computed
/// from the pane width (compact mode shrinks it). The preview budget
/// must be small enough that the truncated preview PLUS the `...`
/// ellipsis and the `↳ ` prefix fits within the column — otherwise
/// ratatui wraps the preview to a second visual line, blowing the
/// row's 2-line height contract. `text::truncate(s, max)` returns
/// `max + 3` chars (head + "..."), so we pass `budget - 3` to keep
/// the total at `budget` chars.
fn build_title_cell(
    b: &crate::tui::app::BacklogLine,
    title_w: usize,
    is_selected: bool,
    app: &App,
) -> Option<Cell<'static>> {
    use ratatui::text::Text;
    let title_str = truncate_for_col(&b.title, title_w);
    let title_style = if is_selected {
        Style::default()
            .fg(crate::tui::palette::on_accent_fg(app.effective_palette()))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(app.effective_palette().foreground)
            .add_modifier(Modifier::BOLD)
    };
    let preview_text = b.preview.trim();
    let preview_style = if is_selected {
        Style::default().fg(crate::tui::palette::on_accent_fg(app.effective_palette()))
    } else {
        Style::default().fg(app.effective_palette().dim)
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(title_str, title_style)));
    if !preview_text.is_empty() {
        let prefix = "↳ ";
        let prefix_w = prefix.chars().count();
        // Reserve prefix + ellipsis room so the truncated preview
        // fits within the title column on a single line.
        let total_budget = title_w.saturating_sub(prefix_w).max(1);
        let truncate_budget = total_budget.saturating_sub(3).max(1);
        let truncated = if preview_text.chars().count() > total_budget {
            crate::text::truncate(preview_text, truncate_budget)
        } else {
            preview_text.to_string()
        };
        let composed = format!("{prefix}{truncated}");
        // Last-resort guard: hard-clip the composed string so even
        // a pathological input (multi-byte chars, sanitizer
        // expansion) can't overflow the column.
        let clipped: String = composed.chars().take(total_budget).collect();
        lines.push(Line::from(Span::styled(clipped, preview_style)));
    } else {
        lines.push(Line::from(String::new()));
    }
    Some(Cell::from(Text::from(lines)))
}

/// M185: Milestones as a Table (REVERSED cursor) with indent + gauge.
pub(super) fn render_milestones_table(frame: &mut Frame, app: &App, area: Rect, view: &ViewState) {
    let visible = app.visible_milestones();
    if visible.is_empty() {
        let msg = if !app.lane_search_term().is_empty() {
            // M186 F-03: empty result with an active search — the search
            // (not the data state) caused the empty list.
            format!(
                "No matches for /{} — press Esc to clear.",
                app.lane_search_term()
            )
        } else if !app.lifecycle_filter_set().is_empty() {
            "No milestones match the lifecycle filter — press F to edit, g for Grooming preset."
                .to_string()
        } else if app.hide_done {
            "All milestones done — press h to show hidden.".to_string()
        } else {
            "No milestones found. Run 'mp list milestones' to check.".to_string()
        };
        let para = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(crate::lanes::LANE_MILESTONES),
            )
            .style(Style::default().fg(app.effective_palette().warn))
            .alignment(Alignment::Center);
        frame.render_widget(para, area);
        return;
    }

    let scroll = view
        .scrollbar_rects
        .iter()
        .find(|hit| matches!(hit.id, ScrollableId::MilestonesList))
        .map(|hit| hit.scroll)
        .unwrap_or(0);

    let header_style = Style::default()
        .fg(app.effective_palette().accent)
        .add_modifier(Modifier::BOLD);
    let active_sort = app.lane_sort_key(app.active_lane);
    // M202: replace the 8-cell Gauge + Lifecycle pair with the
    // single Stage column (`<N>/12 · <Label>`). The Stage cell
    // carries position (the ordinal) and label together, so the
    // Lifecycle text column is gone (AC-13: the Stage cell already
    // carries position).
    //
    // M205: each header cell renders ▼ when the active sort key
    // matches that column. The Stage header was previously
    // hardcoded to `false` (the M202 Stage cell didn't carry a
    // sort affordance). With the M205 cycle including Stage +
    // Created (in addition to the legacy Id/Title/Priority/
    // Updated), the header cells now reflect the active key —
    // matching AC-07.
    let header = Row::new(vec![
        Cell::new(Span::styled(String::new(), header_style)),
        header_cell("ID", active_sort == SortKey::Id, header_style),
        header_cell("Title", active_sort == SortKey::Title, header_style),
        header_cell("Pri", active_sort == SortKey::Priority, header_style),
        header_cell("Stage", active_sort == SortKey::Stage, header_style),
        header_cell("Since", active_sort == SortKey::Updated, header_style),
    ])
    .style(header_style)
    .height(1);

    // M202: column widths rebalanced around the Stage cell.
    // Stage needs room for `12/12 · Hand-off` (longest label) so
    // bump it from the legacy 8-cell gauge (GAUGE_W=8) to 24 cells.
    const INDENT_W: u16 = 4;
    const ID_W: u16 = 6;
    const PRI_W: u16 = 8;
    const STAGE_W: u16 = 24;
    const SINCE_W: u16 = 12;
    const COL_SPACING: u16 = 1;
    let fixed = INDENT_W + ID_W + PRI_W + STAGE_W + SINCE_W + COL_SPACING * 5;
    let title_w = area.width.saturating_sub(2).saturating_sub(fixed).max(8) as usize;

    let depths = depends_on_depths(&visible);

    let mut rows: Vec<Row> = Vec::new();
    for (i, m) in visible.iter().enumerate().skip(scroll) {
        let row_color = progress::status_row_color(&m.lifecycle, app.effective_palette());
        let depth = depths.get(i).copied().unwrap_or(0).min(4);
        let indent = " ".repeat(depth);
        // M174 fix: prefix cancelled milestones with a badge so the
        // TUI Milestones lane surfaces the cancellation directly. The
        // `cancel_reason` is appended when present (truncated to fit
        // the column) so the operator gets the audit context without
        // opening a separate `mp reviews` round-trip. `cancelled_at`
        // stays in the `Since` column via the existing relative-time
        // rendering when the lifecycle_at is the cancellation date.
        let (title, title_style) = if m.cancelled {
            let badge = match m.cancel_reason.as_deref() {
                Some(reason) if !reason.is_empty() => {
                    let snippet = truncate_for_col(reason, title_w.saturating_sub(20).max(8));
                    format!("[cancelled: {}] {}", snippet, m.title)
                }
                _ => format!("[cancelled] {}", m.title),
            };
            (
                truncate_for_col(&badge, title_w),
                Style::default().fg(app.effective_palette().warn),
            )
        } else {
            (truncate_for_col(&m.title, title_w), Style::default())
        };
        let priority = if m.priority.is_empty() {
            "—".to_string()
        } else {
            m.priority.clone()
        };
        let since = m
            .lifecycle_at
            .as_deref()
            .map(crate::tui::humanize::humanize_relative)
            .unwrap_or_else(|| "since updated".to_string());
        // M202: Stage cell replaces the 8-cell Gauge + Lifecycle
        // text pair. Render `<N>/12 · <Label>` for this milestone's
        // current mp-flow stage (derived from flow_stages via the
        // canonical-order helper).
        let stage = progress::stage_cell_line(&m.flow_stages, app.effective_palette());

        rows.push(Row::new(vec![
            Cell::from(indent),
            Cell::from(format!("M{}", m.id)),
            Cell::from(Span::styled(title, title_style)),
            Cell::from(Span::styled(priority, priority_style(&m.priority, app))),
            Cell::from(stage),
            Cell::from(Span::styled(since, Style::default().fg(row_color))),
        ]));
    }

    let widths = [
        Constraint::Length(INDENT_W),
        Constraint::Length(ID_W),
        Constraint::Fill(1),
        Constraint::Length(PRI_W),
        Constraint::Length(STAGE_W),
        Constraint::Length(SINCE_W),
    ];

    let title = milestones_block_title(app, visible.len());
    let table = Table::new(rows, widths)
        .column_spacing(COL_SPACING)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_type(BorderType::Plain),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    // Filtering can leave a stale selection index; clamp it to the visible window.
    let clamped = app.selected_index.min(visible.len().saturating_sub(1));
    let table_selected = clamped.saturating_sub(scroll);
    frame.render_stateful_widget(
        table,
        area,
        &mut ratatui::widgets::TableState::default().with_selected(Some(table_selected)),
    );
}

fn milestones_block_title(app: &App, count: usize) -> String {
    let lf = app.lifecycle_filter_set();
    if lf.is_empty() {
        format!(" {} · All ({count}) ", crate::lanes::LANE_MILESTONES)
    } else {
        let parts: Vec<&str> = lf.iter().map(String::as_str).collect();
        format!(
            " {} · {} ({count}) ",
            crate::lanes::LANE_MILESTONES,
            parts.join(", ")
        )
    }
}

/// Depth via `depends_on[0]` chain, capped at 4 (M185 indent column).
pub fn depends_on_depths(visible: &[&MilestoneSummary]) -> Vec<usize> {
    use std::collections::HashMap;
    let by_id: HashMap<&str, &MilestoneSummary> =
        visible.iter().map(|m| (m.id.as_str(), *m)).collect();
    visible
        .iter()
        .map(|m| {
            let mut depth = 0usize;
            let mut cur = m.depends_on.first().map(String::as_str);
            let mut guard = 0usize;
            while let Some(pid) = cur {
                if guard > 16 {
                    break;
                }
                guard += 1;
                if let Some(parent) = by_id.get(pid) {
                    depth = depth.saturating_add(1);
                    cur = parent.depends_on.first().map(String::as_str);
                } else {
                    // Parent outside the visible set still counts as one level.
                    depth = depth.saturating_add(1);
                    break;
                }
            }
            depth.min(4)
        })
        .collect()
}

/// Title column width for list tables (milestones / backlog): content
/// width minus fixed side columns, inter-column spacing, and borders.
fn list_title_col_width(
    area_width: u16,
    id_w: u16,
    col_a_w: u16,
    col_b_w: u16,
    col_spacing: u16,
) -> usize {
    let inner = area_width.saturating_sub(2); // Borders::ALL
    let gaps = col_spacing.saturating_mul(3); // 4 columns → 3 gaps
    let fixed = id_w
        .saturating_add(col_a_w)
        .saturating_add(col_b_w)
        .saturating_add(gaps);
    inner.saturating_sub(fixed).max(8) as usize
}

/// Fit a title into a column: `text::truncate` keeps `max` head chars
/// then appends `"..."`, so pass width−3 when overflowing.
fn truncate_for_col(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        crate::text::truncate(s, width.saturating_sub(3).max(1))
    }
}

fn priority_style(priority: &str, app: &App) -> Style {
    let p = app.effective_palette();
    match priority {
        "high" | "urgent" => Style::default().fg(p.warn),
        "low" => Style::default().fg(p.dim),
        _ => Style::default().fg(p.foreground),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_col_uses_remaining_width_not_half() {
        let w = list_title_col_width(120, 6, 14, 14, 1);
        assert!(
            w >= 70,
            "title column should claim remaining width on a wide pane; got {w}"
        );
        assert!(w > 47, "title width {w} must beat the old 47-char cap");
    }

    #[test]
    fn backlog_title_col_also_fills() {
        let w = list_title_col_width(120, 10, 10, 12, 1);
        assert!(w > 47, "backlog title width {w} should fill remaining");
    }
}
