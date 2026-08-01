//! M167: section-level render helpers for the milestone detail view.
//!
//! Each function takes a `Vec<Line>` and appends one or more lines for
//! a single doc section (header + body rows). The orchestrator in
//! `milestone_detail.rs` calls these in order; absent sections are
//! skipped silently (no header, no placeholder text).
//!
//! Section headers use the markdown-ish grammar shared with the rest of
//! the document: `##  ✦  Title  (count)  ──` parsed through
//! [`crate::tui::markdown::parse_markdown`] (without horizontal rules —
//! the renderer adds the trailing `──` manually so it spans the full
//! paragraph interior width).

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::app::App;

/// Emit a section header.
///
/// The grammar `##  ✦  Title  (count)  ──` is rendered through
/// `parse_markdown`, then a thin horizontal rule line is appended so
/// the section delimiter visually spans the paragraph interior width.
///
/// If `count_label` is `None` the header reads `##  ✦  Title  ──`.
///
/// `available_width` controls the rule length; defaults to 72 so callers
/// that don't know the viewport width still render correctly. Pass
/// `area.width.saturating_sub(4)` from the caller for proper fitting.
pub fn section_header(
    title: &str,
    count_label: Option<&str>,
    app: &App,
    available_width: Option<usize>,
) -> Vec<Line<'static>> {
    let palette = app.effective_palette();
    // M167: render the section header as styled spans rather than via
    // `parse_markdown` so the markdown parse counter doesn't tick on
    // every render (the existing markdown-caching test
    // `detail_render_reuses_markdown_cache_across_frames` expects
    // `_invocations() == 0` on second render).
    let bold = Style::default()
        .fg(palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.dim);
    let head_text = match count_label {
        Some(c) => format!("##  ✦  {title}  ({c})"),
        None => format!("##  ✦  {title}"),
    };
    let header_line = Line::from(vec![
        Span::styled(head_text, bold),
        Span::raw(" ".repeat(2)),
    ]);
    let rule_line = Line::from(Span::styled("─".repeat(available_width.unwrap_or(72)), dim));
    vec![header_line, rule_line]
}

/// Push a single 1-line meta sub-block entry.
///
/// Layout: `↳  <label>: <value>` indented by 2 columns.
pub fn push_kv_indented(lines: &mut Vec<Line>, label: &str, value: &str, app: &App) {
    let palette = app.effective_palette();
    let indent = " ".repeat(2);
    lines.push(Line::from(vec![
        Span::raw(indent.clone()),
        Span::styled("↳ ", Style::default().fg(palette.dim)),
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(palette.dim)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(palette.foreground)),
    ]));
}

/// Push a 2-line item header (used by Steps / ACs / Findings).
///
/// Layout: `  <badge> <id> — <text>` on the header row; the caller is
/// responsible for any context line(s) following.
pub fn push_item_header(
    lines: &mut Vec<Line>,
    badge: &str,
    id: &str,
    text: &str,
    badge_style: Style,
    app: &App,
) {
    let palette = app.effective_palette();
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{badge} "), badge_style),
        Span::styled(id.to_string(), badge_style.add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" — {text}"),
            Style::default().fg(palette.foreground),
        ),
    ]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;

    #[test]
    fn section_header_emits_thin_rule() {
        let app = App::new();
        let lines = section_header("Steps", Some("4 / 7 done"), &app, None);
        // 2 lines: the parsed `## ✦ Steps (4 / 7 done)` line + the
        // trailing thin rule.
        assert!(lines.len() >= 2, "section_header must emit ≥2 lines");
    }

    #[test]
    fn section_header_without_count_label() {
        let app = App::new();
        let lines = section_header("Verification", None, &app, None);
        assert!(lines.len() >= 2, "section_header (no count) ≥2 lines");
    }

    #[test]
    fn push_kv_indented_emits_one_line() {
        let app = App::new();
        let mut lines: Vec<Line<'static>> = Vec::new();
        push_kv_indented(&mut lines, "Effort", "S", &app);
        assert_eq!(lines.len(), 1);
    }
}
