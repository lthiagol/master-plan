//! Lane tab bar.
//!
//! M167 + BF-revert: the previous attempt to migrate this to
//! `ratatui::widgets::Tabs` regressed the visible highlight because
//! `ratatui::widgets::Tabs::highlight_style` changes only the *text*
//! style of the selected title (no background painting); a per-bar
//! `style` with a non-Reset `bg` bleeds the background across the
//! whole bar and washes out the selected tab. The pre-M167 manual
//! renderer renders each tab label as its own span with a per-tab
//! `fg=Black, bg=accent, BOLD` for the active lane, which is the look
//! the user is used to. We keep the manual renderer unchanged for
//! BOTH wide and narrow modes; the only thing we gain from the
//! migration was a third-party dependency, not visible behavior.
//!
//! Module still exports `render_tab_bar` and consumes the
//! `TabBarLayout` produced by `view_state::compute_tab_bar_layout`
//! (hit areas stay aligned with the rendered spans by construction).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TextLine, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::app::{App, Lane};
use crate::tui::view_state::TabBarLayout;

pub(super) fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect, layout: &TabBarLayout) {
    // M198 / M214 follow-up: the layout indices are computed
    // against the FILTERED lane list
    // (`ordered_visible(app.show_autopilot_tab)` — see
    // `view_state::compute_view`). The renderer must use the same
    // list, otherwise the visible indices walk the wrong list and
    // the bar shows the lane one slot over (e.g. Autopilot where
    // Settings should be when `show_autopilot_tab=false`).
    let lanes = Lane::ordered_visible(app.show_autopilot_tab);
    let active_idx = lanes
        .iter()
        .position(|l| l == &app.active_lane)
        .unwrap_or(0);

    let palette = app.effective_palette();
    // Active tab: black text on the bright accent background,
    // bold. This is the "highlighted tab" look the user expects.
    let active_style = Style::default()
        .fg(crate::tui::palette::on_accent_fg(palette))
        .bg(palette.accent)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(palette.dim);
    let indicator_style = Style::default().fg(palette.dim);
    let ellipsis_text = " \u{2026} "; // 4 cols

    let mut spans: Vec<Span> = Vec::new();

    // M115 review F-1: empty visible set → single leading space.
    if layout.visible.is_empty() {
        spans.push(Span::styled(" ", Style::default()));
        let line = TextLine::from(spans);
        frame.render_widget(
            Paragraph::new(line).alignment(ratatui::layout::Alignment::Left),
            area,
        );
        return;
    }

    let ultra_narrow_no_indicators = !layout.has_left_indicator && !layout.has_right_indicator;
    if !ultra_narrow_no_indicators {
        spans.push(Span::styled(
            if layout.has_left_indicator {
                " \u{25c2} "
            } else {
                "   "
            },
            indicator_style,
        ));
        if layout.has_left_ellipsis {
            spans.push(Span::styled(ellipsis_text, indicator_style));
        }
    }

    // First visible tab — preceded by `│` only if it's not lane 0 (the
    // M137-2 contract: dividers are their own dim span, not the first
    // char of an active tab's label span, so the active-tab highlight
    // covers exactly the label, not the divider to its left).
    let first_idx = layout.visible[0];
    if first_idx > 0 {
        spans.push(Span::styled("│", inactive_style));
    }
    {
        let lane = &lanes[first_idx];
        let label = if layout.compact {
            lane.compact_label()
        } else {
            lane.label()
        };
        let is_active = first_idx == active_idx;
        let style = if is_active {
            active_style
        } else {
            inactive_style
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    // Middle visible tabs (when 3+ are visible).
    if layout.visible.len() >= 3 {
        for &i in &layout.visible[1..layout.visible.len() - 1] {
            spans.push(Span::styled("│", inactive_style));
            let lane = &lanes[i];
            let label = if layout.compact {
                lane.compact_label()
            } else {
                lane.label()
            };
            let is_active = i == active_idx;
            let style = if is_active {
                active_style
            } else {
                inactive_style
            };
            spans.push(Span::styled(format!(" {label} "), style));
        }
    }

    if layout.has_right_ellipsis {
        spans.push(Span::styled(ellipsis_text, indicator_style));
    }

    // Last visible tab — followed by `│` only if it's not the last
    // lane in `Lane::ordered()` (skipping a divider at the end of the
    // bar reads cleaner).
    if layout.visible.len() >= 2 {
        let last_idx = *layout.visible.last().unwrap();
        spans.push(Span::styled("│", inactive_style));
        let lane = &lanes[last_idx];
        let label = if layout.compact {
            lane.compact_label()
        } else {
            lane.label()
        };
        let is_active = last_idx == active_idx;
        let style = if is_active {
            active_style
        } else {
            inactive_style
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    if !ultra_narrow_no_indicators {
        spans.push(Span::styled(
            if layout.has_right_indicator {
                " \u{25b8} "
            } else {
                "   "
            },
            indicator_style,
        ));
    }

    let line = TextLine::from(spans);
    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}
