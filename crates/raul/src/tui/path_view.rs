//! M157: TUI Path tab — vertical tree renderer (ratatui emitter).
//!
//! Renders one execution trunk with labeled side-branches
//! (awaiting-approval, blocked, grooming, review). Blocked milestones
//! fork at their dependency milestone. Built as styled `Line`s
//! (ratatui primitives) so colors come from the active palette and the
//! tree scrolls via `app.path_scroll` when content overflows the Path
//! tab viewport. Backlog items are filtered out — the tree is
//! milestones-only (backlog has its own tab).
//!
//! This is a thin **emitter** over the shared data-shaping in
//! [`crate::path_tree_model`]. M164 removed the CLI counterpart, but
//! the shape still lives in the shared model so future emitters (e.g.
//! a markdown or dot exporter) reuse it without copying the helpers.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::palette;
use crate::path_tree_model as model;

pub fn render(frame: &mut Frame, app: &App, area: Rect, data: &serde_json::Value) {
    let palette_helpers = app.effective_palette();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Path Tree ")
        .title_style(
            Style::default()
                .fg(palette::header_color(palette_helpers))
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_tree_lines(app, data);
    let viewport = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(viewport);
    // Clamp stored scroll if the tree shrank since last frame (e.g. refresh).
    let scroll = (app.path_scroll as usize).min(max_scroll);
    app.path_max_scroll.set(max_scroll as u16);

    let para = Paragraph::new(lines).scroll((scroll as u16, 0));
    frame.render_widget(para, inner);
}

/// Build the full vertical tree as styled lines (shared by render + tests).
pub fn build_tree_lines(app: &App, data: &serde_json::Value) -> Vec<Line<'static>> {
    let palette_helpers = app.effective_palette();
    let lanes_map = model::lane_map(data);
    let mut lines: Vec<Line> = Vec::new();

    let exec_items = lanes_map
        .get("execution")
        .map(|l| l.items.clone())
        .unwrap_or_default();
    if exec_items.is_empty() {
        lines.push(Line::styled(
            "(execution trunk empty — nothing ready to run)",
            Style::default().fg(palette::dim_color(palette_helpers)),
        ));
    } else {
        lines.push(Line::styled(
            "EXECUTION  ready · approved · deps met",
            Style::default()
                .fg(palette::header_color(palette_helpers))
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(spine_line());
        for (i, item) in exec_items.iter().enumerate() {
            let is_next = i == 0;
            let marker = if is_next { "●" } else { "├─" };
            lines.push(trunk_item_line(item, marker, is_next, app));
            lines.push(spine_line());
        }
    }

    let branches: Vec<(&str, Vec<serde_json::Value>)> = model::BRANCH_ORDER
        .iter()
        .filter_map(|name| {
            lanes_map
                .get(*name)
                .filter(|l| !l.items.is_empty())
                .map(|l| (*name, l.items.clone()))
        })
        .collect();

    if branches.is_empty() {
        if !exec_items.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "  ╵",
                Style::default().fg(app.effective_palette().dim),
            )]));
        }
    } else {
        let last = branches.len();
        for (idx, (name, items)) in branches.iter().enumerate() {
            let is_last = idx + 1 == last;
            let fork = if is_last { "└─" } else { "├─" };
            let color = lane_color_for_tui(name);
            // Leading "  " matches CLI path_tree branch headers so the
            // Blocked/… fork lines up under the EXECUTION trunk spine.
            lines.push(Line::from(vec![
                Span::raw(format!("  {fork} ")),
                Span::styled(
                    model::display_name(name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", items.len()),
                    Style::default().fg(app.effective_palette().dim),
                ),
            ]));
            let spine = if is_last { "   " } else { "  │" };
            match *name {
                "blocked" => blocked_lines(&mut lines, items, spine, app),
                _ => flat_branch_lines(&mut lines, items, spine, app),
            }
            lines.push(Line::raw(spine));
        }
    }

    // Collapsed done baseline — same shape as CLI path_tree footer.
    let complete_count = complete_count(app, data);
    if let Some(n) = complete_count {
        if n > 0 {
            lines.push(Line::styled(
                format!("  ╵  {n} complete · open Milestones to list"),
                Style::default().fg(app.effective_palette().dim),
            ));
        }
    }

    lines
}

/// Prefer the status rollup injected on the path envelope (CLI parity);
/// fall back to the dashboard lifecycle bucket when status is absent
/// (typical after a Path-only refresh without a status fan-out).
fn complete_count(app: &App, data: &serde_json::Value) -> Option<usize> {
    if let Some(n) = data
        .get("status")
        .and_then(|s| s.get("milestones"))
        .and_then(|m| m.get("by_lifecycle"))
        .and_then(|lc| lc.get("complete"))
        .and_then(|v| v.as_u64())
    {
        return Some(n as usize);
    }
    let n = app.dashboard.lifecycle_counts.complete;
    if n > 0 {
        Some(n as usize)
    } else {
        None
    }
}

fn spine_line() -> Line<'static> {
    Line::from(vec![Span::raw("  │")])
}

fn trunk_item_line(
    item: &serde_json::Value,
    marker: &str,
    is_next: bool,
    app: &App,
) -> Line<'static> {
    let palette_helpers = app.effective_palette();
    let label = model::item_label(item);
    let detail = model::trunk_detail(item);
    let mut spans = vec![
        Span::raw(format!("  {marker}  ")),
        Span::styled(
            label,
            Style::default().fg(palette::header_color(palette_helpers)),
        ),
    ];
    if !detail.is_empty() {
        spans.push(Span::styled(
            format!("  · {detail}"),
            Style::default().fg(palette::dim_color(palette_helpers)),
        ));
    }
    if is_next {
        // The "next" highlight uses the warn palette color (yellow
        // in mocha) so the highlighted milestone stands out from
        // the other trunk entries without colliding with the
        // blocked-row red.
        spans.push(Span::styled(
            "  ◀ next".to_string(),
            Style::default()
                .fg(palette::warn_color(palette_helpers))
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn flat_branch_lines(
    lines: &mut Vec<Line<'static>>,
    items: &[serde_json::Value],
    spine: &str,
    app: &App,
) {
    for (i, item) in items.iter().enumerate() {
        let last = i + 1 == items.len();
        let connector = if last { "└─" } else { "├─" };
        let label = model::item_label(item);
        let detail = model::branch_detail(item);
        let mut spans = vec![
            Span::raw(format!("{spine}  {connector}  ")),
            Span::styled(
                label,
                Style::default().fg(app.effective_palette().foreground),
            ),
        ];
        if !detail.is_empty() {
            spans.push(Span::styled(
                format!("  · {detail}"),
                Style::default().fg(app.effective_palette().dim),
            ));
        }
        lines.push(Line::from(spans));
    }
}

/// Blocked branch: fork items by their blocker (shared model grouping).
fn blocked_lines(
    lines: &mut Vec<Line<'static>>,
    items: &[serde_json::Value],
    spine: &str,
    app: &App,
) {
    let groups = model::blocked_groups(items);
    let total = groups.len();
    for (gi, (blocker, group_items)) in groups.iter().enumerate() {
        let is_last_group = gi + 1 == total;
        let group_fork = if is_last_group { "└─" } else { "├─" };
        let header_label = model::blocked_group_label(blocker.as_deref());
        lines.push(Line::from(vec![
            Span::raw(format!("{spine}  {group_fork} ")),
            Span::styled(
                header_label,
                Style::default()
                    .fg(palette::status_color("blocked", app.effective_palette()))
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        let group_spine = if is_last_group { "      " } else { "  │   " };
        let prefix = format!("{spine}{group_spine}");
        for (i, item) in group_items.iter().enumerate() {
            let last = i + 1 == group_items.len();
            let connector = if last { "└─" } else { "├─" };
            let label = model::item_label(item);
            let detail = model::branch_detail(item);
            let mut spans = vec![
                Span::raw(format!("{prefix}  {connector}  ")),
                Span::styled(
                    label,
                    Style::default().fg(app.effective_palette().foreground),
                ),
            ];
            if !detail.is_empty() {
                spans.push(Span::styled(
                    format!("  · {detail}"),
                    Style::default().fg(app.effective_palette().dim),
                ));
            }
            lines.push(Line::from(spans));
        }
    }
}

fn lane_color_for_tui(name: &str) -> Color {
    // M172 S4: route through the palette helper so the
    // crossterm → ratatui color conversion isn't a direct Color::*
    // literal at the call site.
    palette::crossterm_to_ratatui(crate::config::lane_color(name))
}
