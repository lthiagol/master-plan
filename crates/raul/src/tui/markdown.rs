use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Palette;

/// Styles for markdown rendering (palette-aware).
pub struct MarkdownStyles<'a> {
    pub palette: &'a Palette,
}

impl MarkdownStyles<'_> {
    pub fn body(&self) -> Style {
        Style::default()
    }

    pub fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub fn code(&self) -> Style {
        Style::default().fg(self.palette.accent)
    }

    pub fn bullet(&self) -> Style {
        Style::default().fg(self.palette.accent)
    }

    pub fn hr(&self) -> Style {
        Style::default().fg(self.palette.dim)
    }
}

#[cfg(debug_assertions)]
thread_local! {
    static PARSE_INVOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test hook (debug builds only): how many times `parse_markdown` ran this thread.
pub fn parse_invocations() -> usize {
    #[cfg(debug_assertions)]
    {
        PARSE_INVOCATIONS.with(|c| c.get())
    }
    #[cfg(not(debug_assertions))]
    {
        0
    }
}

/// Reset parse counter (tests only, debug builds).
pub fn reset_parse_invocations() {
    #[cfg(debug_assertions)]
    PARSE_INVOCATIONS.with(|c| c.set(0));
}

fn record_parse() {
    #[cfg(debug_assertions)]
    PARSE_INVOCATIONS.with(|c| c.set(c.get() + 1));
}

/// Parse lightweight markdown into ratatui lines.
pub fn parse_markdown(
    text: &str,
    styles: &MarkdownStyles<'_>,
    detail_width: usize,
) -> Vec<Line<'static>> {
    record_parse();
    if text.trim().is_empty() {
        return vec![Line::from("")];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_list: Option<ListKind> = None;

    for raw in text.lines() {
        let trimmed = raw.trim_end();
        if trimmed.trim().is_empty() {
            in_list = None;
            lines.push(Line::from(""));
            continue;
        }

        if is_hr(trimmed) {
            in_list = None;
            lines.push(hr_line(detail_width, styles.hr()));
            continue;
        }

        if let Some((kind, content)) = parse_list_marker(trimmed) {
            if in_list != Some(kind) {
                in_list = Some(kind);
            }
            let indent = match kind {
                ListKind::Bullet => "  ",
                ListKind::Numbered => "    ",
            };
            let marker = match kind {
                ListKind::Bullet => "• ".to_string(),
                ListKind::Numbered => {
                    let n = trimmed
                        .split('.')
                        .next()
                        .and_then(|s| s.trim().parse::<usize>().ok())
                        .unwrap_or(1);
                    format!("{n}. ")
                }
            };
            let mut line_spans = vec![Span::raw(indent), Span::styled(marker, styles.bullet())];
            line_spans.extend(parse_inline_spans(content, styles));
            lines.push(Line::from(line_spans));
            continue;
        }

        in_list = None;
        lines.push(Line::from(parse_inline_spans(trimmed, styles)));
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
}

fn is_hr(line: &str) -> bool {
    line.trim() == "---"
}

fn hr_line(width: usize, style: Style) -> Line<'static> {
    let w = width.clamp(10, 80);
    Line::from(Span::styled("─".repeat(w), style))
}

fn parse_list_marker(line: &str) -> Option<(ListKind, &str)> {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return Some((ListKind::Bullet, &t[2..]));
    }
    if let Some(dot) = t.find(". ") {
        let prefix = &t[..dot];
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            return Some((ListKind::Numbered, &t[dot + 2..]));
        }
    }
    None
}

/// Parse inline **bold** and `code` spans (best-effort on unmatched delimiters).
pub fn parse_inline_spans(input: &str, styles: &MarkdownStyles<'_>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = input;

    while !rest.is_empty() {
        if let Some(idx) = rest.find("**") {
            if idx > 0 {
                spans.push(Span::styled(rest[..idx].to_string(), styles.body()));
            }
            rest = &rest[idx + 2..];
            if let Some(end) = rest.find("**") {
                spans.push(Span::styled(rest[..end].to_string(), styles.bold()));
                rest = &rest[end + 2..];
            } else {
                spans.push(Span::styled(format!("**{rest}"), styles.body()));
                break;
            }
            continue;
        }

        if let Some(idx) = rest.find('`') {
            if idx > 0 {
                spans.push(Span::styled(rest[..idx].to_string(), styles.body()));
            }
            rest = &rest[idx + 1..];
            if let Some(end) = rest.find('`') {
                spans.push(Span::styled(rest[..end].to_string(), styles.code()));
                rest = &rest[end + 1..];
            } else {
                spans.push(Span::styled(format!("`{rest}"), styles.body()));
                break;
            }
            continue;
        }

        spans.push(Span::styled(rest.to_string(), styles.body()));
        break;
    }

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    fn styles() -> MarkdownStyles<'static> {
        MarkdownStyles {
            palette: Palette::default_palette(),
        }
    }

    #[test]
    fn parse_bold_and_code() {
        reset_parse_invocations();
        let styles = styles();
        let lines = parse_markdown("**hi** and `code`", &styles, 40);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect();
        assert!(text.contains("hi"));
        assert!(text.contains("code"));
        assert!(lines
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn parse_bullet_and_numbered_lists() {
        let styles = styles();
        let input = "- a\n- b\n1. first";
        let lines = parse_markdown(input, &styles, 40);
        assert!(lines.len() >= 3);
        let joined: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(joined.contains('•'));
        assert!(joined.contains("1."));
    }

    #[test]
    fn parse_horizontal_rule() {
        let styles = styles();
        let lines = parse_markdown("\n---\n", &styles, 20);
        assert!(lines
            .iter()
            .any(|l| { l.spans.iter().any(|s| s.content.contains('─')) }));
    }

    #[test]
    fn edge_cases_no_panic() {
        let styles = styles();
        for input in [
            "",
            "   ",
            "unmatched **bold",
            "unmatched `code",
            "nested *``**",
            "**`mix`**",
        ] {
            let lines = parse_markdown(input, &styles, 30);
            assert!(!lines.is_empty() || input.trim().is_empty());
        }
    }
}
