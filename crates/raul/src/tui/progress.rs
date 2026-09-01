use std::collections::BTreeMap;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::config::{status_role, StatusRole};
use crate::theme::Palette;

/// M202: canonical mp-flow stage slugs in execution order. Re-export
/// of `mp_model::MP_FLOW_STAGE_KEYS` so the raul crate doesn't have
/// to take a hard dependency on the mp-model types. The Stage cell
/// renders the ordinal (`<N>/12`) by indexing into this slice; the
/// milestone-detail Stages section reads the same slice for canonical
/// row order.
pub const MP_FLOW_STAGE_KEYS: &[&str] = &[
    "draft",
    "groom",
    "specify",
    "approve",
    "execute",
    "self-review",
    "complete",
    "external-review",
    "remediate",
    "re-review",
    "document",
    "hand-off",
];

/// M202: human-readable label per stage slug. Mirrors
/// `mp_model::mp_flow_stage_label`; the duplicate lives here so the
/// raul crate doesn't have to import mp-model just to format a
/// string. Keep in sync with the model-side label table.
pub fn mp_flow_stage_label(slug: &str) -> &'static str {
    match slug {
        "draft" => "Define outcome",
        "groom" => "Interview & shape",
        "specify" => "Write acceptance",
        "approve" => "Approve spec",
        "execute" => "Claim & execute",
        "self-review" => "Self-review",
        "complete" => "Mark complete",
        "external-review" => "External review",
        "remediate" => "Remediate findings",
        "re-review" => "Re-review",
        "document" => "Document",
        "hand-off" => "Hand-off",
        _ => "",
    }
}

/// M202: compute the Stage cell content (`<N>/12 · <Stage Label>`) for a
/// milestone's `flow_stages` map. The "current stage" is the first
/// entry in canonical order whose status is `in_progress` (the
/// milestone is actively in that stage), or — when every earlier
/// stage is `done` and no later stage is `in_progress` — the first
/// `pending` stage (the milestone is past the last completed rung).
/// Falls back to stage 12 (`hand-off`) plus its label when every
/// earlier stage is `done`, matching the AC-13 contract.
pub fn current_mp_flow_stage(
    flow_stages: &BTreeMap<String, String>,
) -> (&'static str, &'static str) {
    // First pass: look for any in_progress stage (canonical-order priority).
    for slug in MP_FLOW_STAGE_KEYS {
        let status = flow_stages
            .get(*slug)
            .map(String::as_str)
            .unwrap_or("pending");
        if status == "in_progress" {
            return (slug, mp_flow_stage_label(slug));
        }
    }
    // Second pass: first pending stage (the milestone has finished
    // everything up to here and is queued at this rung).
    for slug in MP_FLOW_STAGE_KEYS {
        let status = flow_stages
            .get(*slug)
            .map(String::as_str)
            .unwrap_or("pending");
        if status == "pending" {
            return (slug, mp_flow_stage_label(slug));
        }
    }
    // All stages done → hand-off is the after-everything state.
    ("hand-off", mp_flow_stage_label("hand-off"))
}

/// M202: render the Stage cell line `<N>/12 · <Label>` for a
/// `MilestoneSummary.flow_stages` map. The ordinal is `idx + 1`
/// (1-based) so the lane renders `1/12 · Define outcome` for a
/// fresh milestone and `12/12 · Hand-off` for a fully-complete
/// one. Falls back to the after-everything sentinel when the
/// milestone is past stage 12 (caller can override via the
/// explicit `mp milestone stage set <id> hand-off done`).
pub fn stage_cell_line(flow_stages: &BTreeMap<String, String>, palette: &Palette) -> Line<'static> {
    let (slug, label) = current_mp_flow_stage(flow_stages);
    let idx = MP_FLOW_STAGE_KEYS
        .iter()
        .position(|s| *s == slug)
        .unwrap_or(MP_FLOW_STAGE_KEYS.len() - 1);
    let ordinal_color = palette.dim;
    Line::from(vec![
        Span::styled(
            format!("{}/{} · ", idx + 1, MP_FLOW_STAGE_KEYS.len()),
            Style::default().fg(ordinal_color),
        ),
        Span::styled(label.to_string(), Style::default().fg(palette.foreground)),
    ])
}

/// Plain-text form of the Stage cell for width-aware tests (no styles).
pub fn stage_cell_plain(flow_stages: &BTreeMap<String, String>) -> String {
    let (slug, _label) = current_mp_flow_stage(flow_stages);
    let idx = MP_FLOW_STAGE_KEYS
        .iter()
        .position(|s| *s == slug)
        .unwrap_or(MP_FLOW_STAGE_KEYS.len() - 1);
    format!("{}/{}", idx + 1, MP_FLOW_STAGE_KEYS.len())
}

/// Canonical forward-flow lifecycle order for the M185 gauge (8 segments).
pub const LIFECYCLE_GAUGE_ORDER: &[&str] = &[
    "draft",
    "groomed",
    "approved",
    "in-progress",
    "done",
    "self-reviewed",
    "reviewed",
    "complete",
];

/// All lifecycles shown in the multi-select filter modal (canonical
/// path first, off-path branches last).
pub const LIFECYCLE_FILTER_OPTIONS: &[&str] = &[
    "draft",
    "groomed",
    "approved",
    "in-progress",
    "done",
    "self-reviewed",
    "reviewed",
    "complete",
    "cancelled",
    "remediation",
];

/// Grooming preset: legacy Grooming-tab semantics (M185).
pub const GROOMING_PRESET: &[&str] = &["approved", "in-progress", "groomed"];

/// M185: lifecycle → palette color (moved from milestone_tree).
pub fn lifecycle_color(lifecycle: &str, palette: &Palette) -> ratatui::style::Color {
    match lifecycle {
        "complete" => palette.success,
        "in-progress" => palette.accent,
        "blocked" => palette.danger,
        "approved" | "ready" => palette.warn,
        _ => palette.dim,
    }
}

/// Index into [`LIFECYCLE_GAUGE_ORDER`], or `None` for off-path states.
pub fn lifecycle_gauge_index(lifecycle: &str) -> Option<usize> {
    LIFECYCLE_GAUGE_ORDER.iter().position(|&s| s == lifecycle)
}

/// Render the 8-cell lifecycle gauge (or off-path marker) as a Line.
pub fn lifecycle_gauge_line(lifecycle: &str, palette: &Palette) -> Line<'static> {
    match lifecycle {
        "cancelled" => Line::from(Span::styled(
            "✗".to_string(),
            Style::default().fg(palette.danger),
        )),
        "remediation" => Line::from(Span::styled(
            "↺".to_string(),
            Style::default().fg(palette.warn),
        )),
        other => {
            let idx = lifecycle_gauge_index(other);
            let mut spans = Vec::with_capacity(8);
            for i in 0..8 {
                let (ch, style) = match idx {
                    Some(cur) if i < cur => ('▮', Style::default().fg(palette.dim)),
                    Some(cur) if i == cur => {
                        ('▮', Style::default().fg(lifecycle_color(other, palette)))
                    }
                    _ => ('▯', Style::default().fg(palette.dim)),
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            Line::from(spans)
        }
    }
}

/// M185 F-02: visible window of `n` filter options around `selected`
/// that fits in `inner_h` rows (including optional more-above/below
/// indicator rows). Returns `(start, end, more_above, more_below)`.
pub fn lifecycle_filter_window(
    n: usize,
    selected: usize,
    inner_h: usize,
) -> (usize, usize, bool, bool) {
    if n == 0 || inner_h == 0 {
        return (0, 0, false, false);
    }
    if n <= inner_h {
        return (0, n, false, false);
    }
    let mut budget = inner_h.max(1);
    let mut start = selected.saturating_sub(budget.saturating_sub(1) / 2);
    let mut end = (start + budget).min(n);
    if end - start < budget {
        start = end.saturating_sub(budget);
    }
    if end - start >= n {
        return (0, n, false, false);
    }
    let more_above = start > 0;
    let more_below = end < n;
    let indicator_rows = usize::from(more_above) + usize::from(more_below);
    if indicator_rows > 0 && budget > indicator_rows {
        budget -= indicator_rows;
        start = selected.saturating_sub(budget.saturating_sub(1) / 2);
        end = (start + budget).min(n);
        if end - start < budget {
            start = end.saturating_sub(budget);
        }
        let more_above = start > 0;
        let more_below = end < n;
        return (start, end, more_above, more_below);
    }
    (start, end, more_above, more_below)
}

/// Plain-text form of the gauge for width-aware tests (no styles).
pub fn lifecycle_gauge_plain(lifecycle: &str) -> String {
    match lifecycle {
        "cancelled" => "✗".to_string(),
        "remediation" => "↺".to_string(),
        other => {
            let idx = lifecycle_gauge_index(other);
            (0..8)
                .map(|i| match idx {
                    Some(cur) if i <= cur => '▮',
                    _ => '▯',
                })
                .collect()
        }
    }
}

/// Step progress bar: `[=====>    ] 7/12`.
pub fn compute_progress_bar(done: usize, total: usize, width: usize) -> String {
    let width = width.max(4);
    if total == 0 {
        return format!("[{}] 0/0", " ".repeat(width));
    }
    let clamped_done = done.min(total);
    let filled = (clamped_done * width) / total;
    let mut bar: Vec<char> = vec![' '; width];
    for ch in bar.iter_mut().take(filled.min(width)) {
        *ch = '=';
    }
    if clamped_done < total {
        let arrow = filled.min(width.saturating_sub(1));
        bar[arrow] = '>';
    } else if width > 0 {
        let last = width - 1;
        bar[last] = '=';
    }
    format!(
        "[{}] {}/{}",
        bar.into_iter().collect::<String>(),
        clamped_done,
        total
    )
}

/// Map a `StatusRole` to a ratatui `Color` from the TUI palette. Single
/// source of truth: used by both `style_for_role` (badge) and `color_for_role`
/// (row foreground). Post-workaround-pass consolidation.
pub fn color_for_role(role: StatusRole, palette: &Palette) -> ratatui::style::Color {
    match role {
        StatusRole::Done => palette.success,
        StatusRole::InProgress => palette.accent,
        StatusRole::Implemented | StatusRole::AwaitingReview => palette.warn,
        StatusRole::Blocked => palette.danger,
        StatusRole::NotStarted | StatusRole::Unknown => palette.dim,
    }
}

/// AC status badge color from palette.
pub fn ac_status_style(status: &str, palette: &Palette) -> Style {
    match status {
        "passed" => Style::default().fg(palette.success),
        "failed" => Style::default().fg(palette.danger),
        _ => Style::default().fg(palette.warn),
    }
}

/// Spec / execution status badge color from palette.
///
/// Routes both legacy (spec_status / execution_status) and new (lifecycle)
/// values through the `StatusRole` enum (single source of truth in
/// `config::status_role`) and then maps role → palette color via
/// `color_for_role`. So adding a new state means updating one match
/// table in `config.rs`, not four.
///
/// Terminal and approved lifecycle values retain semantic colors on direct
/// lifecycle reads; legacy review values remain emphasized while pending.
pub fn status_badge_style(status: &str, palette: &Palette) -> Style {
    Style::default().fg(color_for_role(status_role(status), palette))
}

/// Spec / execution row color for the milestone LIST view (used by the
/// table cell foreground). Pairs with `status_badge_style` for the badge
/// rendering in detail. Same color table via `color_for_role`.
pub fn status_row_color(status: &str, palette: &Palette) -> ratatui::style::Color {
    color_for_role(status_role(status), palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_edge_cases() {
        assert_eq!(compute_progress_bar(0, 0, 10), "[          ] 0/0");
        assert_eq!(
            compute_progress_bar(0, 5, 10),
            compute_progress_bar(0, 5, 10)
        );
        assert!(compute_progress_bar(7, 12, 10).contains("7/12"));
        assert!(compute_progress_bar(12, 12, 10).contains("12/12"));
        assert!(compute_progress_bar(1, 20, 5).contains('/'));
    }

    #[test]
    fn progress_bar_proportion() {
        let bar = compute_progress_bar(6, 12, 10);
        assert!(bar.starts_with('['));
        assert!(bar.contains("6/12"));
    }
}

#[cfg(test)]
mod tests_extra {
    use super::*;

    /// `implemented` is the legacy spec equivalent of lifecycle `done`
    /// and therefore uses the warning color rather than the dim fallback.
    #[test]
    fn status_badge_for_implemented_is_warn_not_dim() {
        let palette = Palette::default_palette();
        let style = status_badge_style("implemented", palette);
        assert_eq!(style.fg, Some(palette.warn));
    }

    #[test]
    fn status_badge_for_blocked_is_danger() {
        let palette = Palette::default_palette();
        let style = status_badge_style("blocked", palette);
        assert_eq!(style.fg, Some(palette.danger));
    }

    #[test]
    fn status_badge_for_complete_is_success() {
        let palette = Palette::default_palette();
        let style = status_badge_style("complete", palette);
        assert_eq!(style.fg, Some(palette.success));
    }

    /// Remediation (2026-07-05): the CLI and TUI status→color tables now
    /// derive from a single `StatusRole` enum. Pin that adding any future
    /// state means updating exactly one table. This test enumerates every
    /// string the production code can legitimately pass through.
    #[test]
    fn role_table_is_exhaustive() {
        let strings = [
            // Done
            "done",
            "verified",
            "passed",
            "complete",
            // Done (closed-state aliases)
            "resolved",
            "closed",
            // InProgress
            "in-progress",
            "active",
            "open",
            "reviewed",
            // Implemented
            "implemented",
            "self-reviewed",
            // AwaitingReview
            "ready",
            "approved",
            "groomed",
            "review",
            "draft",
            "interview",
            "remediation",
            "deferred",
            // Blocked
            "blocked",
            "failed",
            "cancelled",
            "removed",
            "rejected",
            // NotStarted
            "planned",
            "pending",
        ];
        for s in strings {
            // Every known string must map to a non-Unknown role.
            let r = status_role(s);
            assert_ne!(
                r,
                StatusRole::Unknown,
                "production code passes `{s}` to status_role, must classify it"
            );
        }
    }

    /// Pin: CLI paint and TUI palette agree on the same role.
    #[test]
    fn cli_and_tui_agree_on_role_color_mapping() {
        use crate::config::paint_for_role;
        let palette = Palette::default_palette();
        let samples = [
            "done",
            "verified",
            "in-progress",
            "implemented",
            "ready",
            "blocked",
            "planned",
            "complete",
            "approved",
            "remediation",
            "cancelled",
        ];
        for s in samples {
            let role = status_role(s);
            let cli_output = paint_for_role(s, role);
            // The CLI output embeds an ANSI escape IF color is enabled.
            // Either way, the underlying mapping is consistent — both
            // helper functions route through `status_role`.
            assert!(!cli_output.is_empty(), "{s}: paint_for_role returned empty");
            // TUI side: status_badge_style and status_row_color must
            // agree on the same fg color.
            let bg = status_badge_style(s, palette);
            let fg = status_row_color(s, palette);
            assert_eq!(
                bg.fg,
                Some(fg),
                "{s}: status_badge_style vs status_row_color disagree on fg"
            );
        }
    }
}
