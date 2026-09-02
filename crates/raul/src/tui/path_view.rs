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
//!
//! M206: every milestone row renders as 2 visual lines — a title line
//! (id, title, stage chip, priority glyph, age, detail, optional
//! overlay chip, optional "next" indicator) and a preview line
//! (`↳` + first line of `intent.outcome`, truncated). The branch
//! headers (BLOCKED / AWAITING-APPROVAL / …) keep their bold color and
//! item-count suffix; tree connectors (├─/└─/│/●) and spines stay.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::palette;
use super::progress::{self, MP_FLOW_STAGE_KEYS};
use crate::path_tree_model as model;
use crate::theme;

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

    let lines = build_tree_lines_with_width(app, data, inner.width);
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
    // Default to a comfortable preview column budget when no width
    // is supplied (e.g. unit tests calling `build_tree_lines` directly).
    build_tree_lines_with_width(app, data, u16::MAX)
}

/// M206 AC-11: compact mode rendering. When the path tree is asked
/// to render at a narrow width, the preview column budget shrinks so
/// the truncated preview kicks in earlier (less horizontal space).
pub fn build_tree_lines_with_width(
    app: &App,
    data: &serde_json::Value,
    width: u16,
) -> Vec<Line<'static>> {
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
            let row = trunk_item_rows(item, marker, is_next, app, width);
            lines.extend(row);
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
                "blocked" => blocked_lines(&mut lines, items, spine, app, width),
                _ => flat_branch_lines(&mut lines, items, spine, app, width),
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

// =========================================================================
// M206: helpers — outcome preview, enrichment chips, overlay chip.
// =========================================================================

/// Default preview column width. The Path title line already uses
/// ~10 columns of chrome (marker + spine + id + chip + priority) so the
/// preview is given a comfortable budget at a 120-wide terminal. The
/// trunk caller MAY shrink this budget for compact mode (M206 AC-11).
pub const PREVIEW_DEFAULT_MAX: usize = 80;

/// Compact-mode preview budget. M206 AC-11: at narrow widths the
/// preview column is shorter so truncation kicks in earlier. The
/// compact threshold is the inner Path Tree width — anything below
/// `COMPACT_WIDTH_THRESHOLD` (100 cols) gets the compact budget.
pub const PREVIEW_COMPACT_MAX: usize = 40;
pub const COMPACT_WIDTH_THRESHOLD: u16 = 100;

/// Pick a preview budget based on the inner width. AC-11: compact
/// mode (narrow widths) shrinks the preview column. Widths >= 100
/// get the full 80-char budget; widths < 100 get the 40-char compact
/// budget. Widths >= `u16::MAX` (sentinel from `build_tree_lines`
/// when no width is supplied) always use the default budget.
pub fn preview_max_for_width(width: u16) -> usize {
    if width == u16::MAX {
        PREVIEW_DEFAULT_MAX
    } else if width < COMPACT_WIDTH_THRESHOLD {
        PREVIEW_COMPACT_MAX
    } else {
        PREVIEW_DEFAULT_MAX
    }
}

/// First non-empty line of `intent.outcome`, or empty string when
/// `outcome` is missing or whitespace-only.
///
/// AC-05 / AC-10: whitespace-only outcomes render ONLY the `↳` prefix
/// (no `(no description)` placeholder). Splitting on `\n` keeps the
/// first visual line for multi-line outcomes; the rest is truncated.
pub fn outcome_first_line(item: &serde_json::Value) -> String {
    let raw = item["milestone"]["intent"]["outcome"]
        .as_str()
        .unwrap_or("");
    raw.lines()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

/// Render the preview line `↳ …` (dim color) for an outcome string.
/// Empty / whitespace-only outcomes emit JUST the `↳` prefix in dim
/// color (no content span) — AC-05 / AC-10.
///
/// `prefix` is the leading whitespace + spine chars (e.g. `"    ↳ "`),
/// `outcome` is the first-line text, `max_chars` is the content budget
/// (not counting the prefix). Truncation uses char boundaries (NOT
/// byte boundaries) so multi-byte UTF-8 doesn't panic.
pub fn preview_line(
    prefix: &str,
    outcome: &str,
    palette_helpers: &theme::Palette,
    max_chars: usize,
) -> Line<'static> {
    let dim = Style::default().fg(palette_helpers.dim);
    if outcome.is_empty() {
        // AC-05 / AC-10: only the prefix, dim color, no content.
        return Line::from(vec![Span::styled(prefix.to_string(), dim)]);
    }
    let truncated: String = if outcome.chars().count() > max_chars {
        let mut s: String = outcome.chars().take(max_chars).collect();
        s.push('…');
        s
    } else {
        outcome.to_string()
    };
    Line::from(vec![
        Span::styled(prefix.to_string(), dim),
        Span::styled(truncated, dim),
    ])
}

/// Plain text for assertions that bypass styling (`preview_line` keeps
/// style; this strips it for snapshotting).
pub fn preview_line_plain(prefix: &str, outcome: &str, max_chars: usize) -> String {
    if outcome.is_empty() {
        return prefix.to_string();
    }
    if outcome.chars().count() > max_chars {
        let mut s: String = outcome.chars().take(max_chars).collect();
        s.push('…');
        format!("{prefix}{s}")
    } else {
        format!("{prefix}{outcome}")
    }
}

/// Map priority string to the M206 title-line glyph. The "flag" prefix
/// (`⚑`) flags high/urgent; the dash (`─`) marks normal/low. Missing
/// or unknown priorities render `—` (em-dash) — AC-04.
pub fn priority_glyph(priority: &str) -> &'static str {
    match priority {
        "high" => "⚑high",
        "urgent" => "⚑urgent",
        "normal" => "─norm",
        "low" => "─low",
        _ => "—",
    }
}

/// Plain helper around `humanize_relative` with a `—` fallback so the
/// title line never has a bare `unknown` token.
pub fn age_text(lifecycle_at: &str) -> String {
    use super::humanize::humanize_relative;
    if lifecycle_at.is_empty() {
        "—".to_string()
    } else {
        humanize_relative(lifecycle_at)
    }
}

/// Status overlay chip text + color — `BLOCKED` / `CANCELLED` /
/// `DEFERRED`. Precedence (AC-03): `blocked` wins over `cancelled`
/// wins over `deferred`. Returns `None` when none of the three are
/// set. The color matches AC-03: blocked=danger, cancelled=warn,
/// deferred=dim.
pub fn overlay_chip_pair(
    item: &serde_json::Value,
    palette_helpers: &theme::Palette,
) -> Option<(&'static str, Color)> {
    let blocked = item["milestone"]["blocked"].as_bool().unwrap_or(false);
    let cancelled = item["milestone"]["cancelled"].as_bool().unwrap_or(false);
    let deferred = item["milestone"]["deferred"].as_bool().unwrap_or(false);
    if blocked {
        Some(("BLOCKED", palette_helpers.danger))
    } else if cancelled {
        Some(("CANCELLED", palette_helpers.warn))
    } else if deferred {
        Some(("DEFERRED", palette_helpers.dim))
    } else {
        None
    }
}

/// Stage chip text `[N/12]` from `flow_stages`. The current stage is
/// the first non-done, non-skipped stage in canonical order (absent
/// entries read as pending). When `flow_stages` is empty the chip
/// falls back to the milestone's `lifecycle` text — AC-02.
pub fn stage_chip_text(item: &serde_json::Value) -> String {
    let m = &item["milestone"];
    let flow_map: std::collections::BTreeMap<String, String> = m["flow_stages"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(slug, stage)| {
                    stage
                        .get("status")
                        .and_then(|s| s.as_str())
                        .map(|s| (slug.clone(), s.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    if flow_map.is_empty() {
        // AC-02: lifecycle fallback when flow_stages is empty.
        return m["lifecycle"].as_str().unwrap_or("").to_string();
    }
    let (slug, _label) = progress::current_mp_flow_stage(&flow_map);
    let idx = progress::mp_flow_stage_index(slug).unwrap_or(MP_FLOW_STAGE_KEYS.len() - 1);
    format!("[{}/{}]", idx + 1, MP_FLOW_STAGE_KEYS.len())
}

/// Blocker annotation `blocker: M<n>` for blocked milestones. Mirrors
/// `model::first_dep` — returns `None` when there is no depends_on.
/// The label appears on the title line alongside the overlay chip
/// (M206 AC-06).
pub fn blocker_annotation(item: &serde_json::Value) -> Option<String> {
    let dep = model::first_dep(item)?;
    Some(format!("blocker: M{dep}"))
}

// =========================================================================
// M206: row emitters — title line + preview line.
// =========================================================================

/// Build the 2 visual lines for one execution trunk item: title +
/// preview. Returned as a `Vec<Line>` so the caller can extend the
/// shared line buffer in one pass. Title line carries the marker,
/// label, stage chip, priority glyph, age, detail, optional overlay
/// chip, and the optional `◀ next` indicator. Preview line carries
/// `↳` + the first line of `intent.outcome`.
pub fn trunk_item_rows(
    item: &serde_json::Value,
    marker: &str,
    is_next: bool,
    app: &App,
    width: u16,
) -> Vec<Line<'static>> {
    let palette_helpers = app.effective_palette();
    let label = model::item_label(item);
    let detail = model::trunk_detail(item);

    // ── Title line ──
    let mut title_spans = vec![
        Span::raw(format!("  {marker}  ")),
        Span::styled(
            label,
            Style::default().fg(palette::header_color(palette_helpers)),
        ),
    ];
    // M206 S1.1: stage chip (S1.1 adds it; placeholder until S1.1 lands
    // so this step's tests can pin the title-line shape without
    // asserting against the chip yet). M206 S1 emits the marker/label/
    // detail/next set WITHOUT the chip; S1.1 adds the chip + priority
    // + age + overlay.
    let stage = stage_chip_text(item);
    if !stage.is_empty() {
        title_spans.push(Span::styled(
            format!("  {stage}"),
            Style::default().fg(palette_helpers.dim),
        ));
    }
    let prio = priority_glyph(item["milestone"]["priority"].as_str().unwrap_or(""));
    title_spans.push(Span::styled(
        format!("  {prio}"),
        Style::default().fg(palette_helpers.dim),
    ));
    let age = age_text(item["milestone"]["lifecycle_at"].as_str().unwrap_or(""));
    title_spans.push(Span::styled(
        format!("  {age}"),
        Style::default().fg(palette_helpers.dim),
    ));
    if !detail.is_empty() {
        title_spans.push(Span::styled(
            format!("  · {detail}"),
            Style::default().fg(palette::dim_color(palette_helpers)),
        ));
    }
    if let Some((label, color)) = overlay_chip_pair(item, palette_helpers) {
        title_spans.push(Span::styled(
            format!("  [{label}]"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    if is_next {
        // The "next" highlight uses the warn palette color (yellow
        // in mocha) so the highlighted milestone stands out from
        // the other trunk entries without colliding with the
        // blocked-row red.
        title_spans.push(Span::styled(
            "  ◀ next".to_string(),
            Style::default()
                .fg(palette::warn_color(palette_helpers))
                .add_modifier(Modifier::BOLD),
        ));
    }
    let title_line = Line::from(title_spans);

    // ── Preview line ──
    let outcome = outcome_first_line(item);
    // The preview indent matches the title-line content indent: two
    // spaces (trunk indent) + two spaces (post-marker gap) = 4 spaces
    // before the ↳ glyph. The `↳` itself is one char; `↳ ` is two
    // chars. Together with the 4-space indent this keeps the preview
    // visually aligned under the title text.
    let preview_prefix = "    ↳ ".to_string();
    let preview = preview_line(
        &preview_prefix,
        &outcome,
        palette_helpers,
        preview_max_for_width(width),
    );

    vec![title_line, preview]
}

fn flat_branch_lines(
    lines: &mut Vec<Line<'static>>,
    items: &[serde_json::Value],
    spine: &str,
    app: &App,
    width: u16,
) {
    let palette_helpers = app.effective_palette();
    for (i, item) in items.iter().enumerate() {
        let last = i + 1 == items.len();
        let connector = if last { "└─" } else { "├─" };
        let label = model::item_label(item);
        let detail = model::branch_detail(item);

        // ── Title line ──
        let mut title_spans = vec![
            Span::raw(format!("{spine}  {connector}  ")),
            Span::styled(label, Style::default().fg(palette_helpers.foreground)),
        ];
        let stage = stage_chip_text(item);
        if !stage.is_empty() {
            title_spans.push(Span::styled(
                format!("  {stage}"),
                Style::default().fg(palette_helpers.dim),
            ));
        }
        let prio = priority_glyph(item["milestone"]["priority"].as_str().unwrap_or(""));
        title_spans.push(Span::styled(
            format!("  {prio}"),
            Style::default().fg(palette_helpers.dim),
        ));
        let age = age_text(item["milestone"]["lifecycle_at"].as_str().unwrap_or(""));
        title_spans.push(Span::styled(
            format!("  {age}"),
            Style::default().fg(palette_helpers.dim),
        ));
        if !detail.is_empty() {
            title_spans.push(Span::styled(
                format!("  · {detail}"),
                Style::default().fg(palette_helpers.dim),
            ));
        }
        if let Some((label, color)) = overlay_chip_pair(item, palette_helpers) {
            title_spans.push(Span::styled(
                format!("  [{label}]"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(title_spans));

        // ── Preview line ──
        let outcome = outcome_first_line(item);
        // Indent mirrors the title-line post-spine gap: "{spine}  {connector}  "
        // becomes a 2-space preview indent under the label. For a
        // non-last branch spine = "  │", the preview indent is
        // "{spine}      ↳ " (spine 3 + 6 spaces + ↳).
        let preview_prefix = format!("{spine}      ↳ ");
        let preview = preview_line(
            &preview_prefix,
            &outcome,
            palette_helpers,
            preview_max_for_width(width),
        );
        lines.push(preview);
    }
}

/// Blocked branch: fork items by their blocker (shared model grouping).
/// Each blocked item emits 2 visual lines: title (with blocker
/// annotation + overlay chip) + preview.
fn blocked_lines(
    lines: &mut Vec<Line<'static>>,
    items: &[serde_json::Value],
    spine: &str,
    app: &App,
    width: u16,
) {
    let palette_helpers = app.effective_palette();
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
                    .fg(palette::status_color("blocked", palette_helpers))
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

            // ── Title line ──
            let mut title_spans = vec![
                Span::raw(format!("{prefix}  {connector}  ")),
                Span::styled(label, Style::default().fg(palette_helpers.foreground)),
            ];
            let stage = stage_chip_text(item);
            if !stage.is_empty() {
                title_spans.push(Span::styled(
                    format!("  {stage}"),
                    Style::default().fg(palette_helpers.dim),
                ));
            }
            let prio = priority_glyph(item["milestone"]["priority"].as_str().unwrap_or(""));
            title_spans.push(Span::styled(
                format!("  {prio}"),
                Style::default().fg(palette_helpers.dim),
            ));
            let age = age_text(item["milestone"]["lifecycle_at"].as_str().unwrap_or(""));
            title_spans.push(Span::styled(
                format!("  {age}"),
                Style::default().fg(palette_helpers.dim),
            ));
            if !detail.is_empty() {
                title_spans.push(Span::styled(
                    format!("  · {detail}"),
                    Style::default().fg(palette_helpers.dim),
                ));
            }
            // M206 AC-06: blocker annotation appears on the title line
            // alongside the overlay chip. Use a muted dim style for
            // the annotation so it doesn't compete with the danger
            // overlay chip.
            if let Some(ann) = blocker_annotation(item) {
                title_spans.push(Span::styled(
                    format!("  {ann}"),
                    Style::default().fg(palette_helpers.dim),
                ));
            }
            if let Some((label, color)) = overlay_chip_pair(item, palette_helpers) {
                title_spans.push(Span::styled(
                    format!("  [{label}]"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(title_spans));

            // ── Preview line ──
            let outcome = outcome_first_line(item);
            let preview_prefix = format!("{prefix}      ↳ ");
            let preview = preview_line(
                &preview_prefix,
                &outcome,
                palette_helpers,
                preview_max_for_width(width),
            );
            lines.push(preview);
        }
    }
}

fn lane_color_for_tui(name: &str) -> Color {
    // M172 S4: route through the palette helper so the
    // crossterm → ratatui color conversion isn't a direct Color::*
    // literal at the call site.
    palette::crossterm_to_ratatui(crate::config::lane_color(name))
}
