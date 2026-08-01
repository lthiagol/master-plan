use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::scrollbar::Set as ScrollbarSet;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::tui::app::{App, ContentState, Lane};
use crate::tui::key_combo::format_key_combo;
use crate::tui::keybinds::Keybinds;
use crate::tui::mode::Mode;
use crate::tui::view_state::ViewState;

/// BF-01: walk `view.scrollbar_rects` and draw a track + thumb for
/// each scrollable region via `ratatui::widgets::Scrollbar` +
/// `ScrollbarState`. The gutter geometry was computed by
/// `compute_view` (M135); this function is read-only on `view`.
///
/// The pre-ratatuui dispatcher (M137) wrote track `│` + thumb `█`
/// glyphs directly via `Buffer`. The framework `Scrollbar` widget
/// produces an equivalent vertical track + thumb using its default
/// symbols (track `│` + thumb `█` in default Symbols); we configure
/// the begin/end markers to keep the gutter single-column.
pub(super) fn render_scrollbars(frame: &mut Frame, view: &ViewState, app: &App) {
    let palette = app.effective_palette();
    for hit in &view.scrollbar_rects {
        if hit.rect.width == 0 || hit.rect.height == 0 {
            continue;
        }
        // ScrollbarState positions the thumb by `position` (the
        // current top-row index) and `content_length` (total scrollable
        // rows). The thumb size is derived from position/content_length
        // ratio against the track height automatically.
        let mut state =
            ScrollbarState::new(hit.total).position(hit.scroll.min(hit.total.saturating_sub(1)));
        // Per-M137 visual contract: track = `│` dim, thumb = `█`
        // accent-bold. The framework uses `style` for the track
        // and `thumb_style` for the thumb; apply palette colors.
        // Begin/end markers are single emtpy cells (the gutter is one
        // column wide).
        let style = Style::default().fg(palette.dim);
        let thumb_style = Style::default()
            .fg(palette.accent)
            .bg(palette.foreground)
            .add_modifier(Modifier::BOLD);
        let symbols = ScrollbarSet {
            track: "│",
            thumb: "█",
            begin: " ",
            end: " ",
        };
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .symbols(symbols)
            .style(style)
            .thumb_style(thumb_style);
        frame.render_stateful_widget(bar, hit.rect, &mut state);
    }
}

pub(super) fn footer_for(app: &App) -> String {
    // M138 + M167: footer text is generated from `app.keybinds`
    // (the single source of truth), so a rebind is reflected here
    // automatically. M167 removed the tab-bar focus toggle, so the
    // tab-bar footer (`footer_tab_bar`) is no longer dispatched here;
    // we always render the content-pane footer for the current lane.
    // M179: the Overview W/w auto-refresh branch is gone — manual
    // refresh (r/R) is the only refresh path on the Overview lane.
    // M185: LifecycleFilter modal has its own per-tab keys.
    if matches!(app.active_mode, Mode::LifecycleFilter(_)) {
        return " ↑↓ navigate · Space toggle · ⏎ commit · Esc revert ".to_string();
    }
    // M186: SearchInput modal has its own per-tab keys.
    if matches!(app.active_mode, Mode::SearchInput(_)) {
        return " type to filter · ⏎ commit · Esc cancel ".to_string();
    }
    // Sort-rebind inline menu.
    if app.sort_rebind_open() {
        return " ↑↓ cycle · ⏎ bind · Esc cancel ".to_string();
    }
    let kb = &app.keybinds;
    match (&app.active_lane, &app.content) {
        (Lane::Overview, ContentState::List) => kb.footer_overview(),
        (Lane::Settings, _) => Keybinds::footer_settings(app.settings.as_ref()),
        (_, ContentState::List) => kb.footer_list(),
        _ => kb.footer_content(app.open_only),
    }
}

/// M183: first key glyph for a binding slot (width-constrained footer).
fn primary_key(combos: &[crate::tui::key_combo::KeyCombo]) -> String {
    combos
        .first()
        .map(|c| format_key_combo(*c))
        .unwrap_or_default()
}

/// Prefer a combo matching `pred` (e.g. Tab for lane-switch); fall back
/// to the primary binding when the preferred glyph is unbound.
fn preferred_key(
    combos: &[crate::tui::key_combo::KeyCombo],
    pred: impl Fn(crate::tui::key_combo::KeyCombo) -> bool,
) -> String {
    combos
        .iter()
        .copied()
        .find(|&c| pred(c))
        .or_else(|| combos.first().copied())
        .map(format_key_combo)
        .unwrap_or_default()
}

/// Compact display for shifted letter combos (`Shift+s` → `S`) so the
/// globals line matches the on-keyboard glyph users type.
fn footer_key_glyph(combos: &[crate::tui::key_combo::KeyCombo]) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    if let Some((KeyCode::Char(c), mods)) = combos.first().copied() {
        if mods == KeyModifiers::SHIFT && c.is_ascii_alphabetic() {
            return c.to_ascii_uppercase().to_string();
        }
    }
    primary_key(combos)
}

/// M183: hand-curated global keybind line (live keys only, post-M186).
/// Truncates to `width` columns when the full list would overflow
/// (AC-03 graceful fallback).
pub(super) fn footer_globals_line(app: &App, width: u16) -> Line<'static> {
    let kb = &app.keybinds;
    let palette = app.effective_palette();
    let live = Style::default().fg(palette.dim);

    // M183 F-05: skip slots whose binding glyph is empty so a cleared
    // keybind does not render a leading-colon token (`:quit`).
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(9);
    let push_live = |spans: &mut Vec<Span<'static>>, glyph: String, label: &str, leading: &str| {
        if glyph.is_empty() {
            return;
        }
        spans.push(Span::styled(format!("{leading}{glyph}:{label}"), live));
    };
    push_live(&mut spans, primary_key(&kb.quit), "quit", " ");
    push_live(&mut spans, primary_key(&kb.help), "help", "  ");
    push_live(&mut spans, primary_key(&kb.refresh), "refresh", "  ");
    // Spec surfaces Tab / Shift+Tab for lane switch even though the
    // primary bindings are ←/→ (multi-binding aliases stay first for
    // resolve order). Prefer those display glyphs when present.
    // Compare formatted labels (not KeyCode tokens) so chrome stays
    // out of the Tab-dispatch site count (tui_tab_bar S4 pin).
    let prev = preferred_key(&kb.previous_lane, |c| format_key_combo(c) == "Shift+Tab");
    let next = preferred_key(&kb.next_lane, |c| format_key_combo(c) == "Tab");
    if !prev.is_empty() || !next.is_empty() {
        let lane_glyph = match (prev.is_empty(), next.is_empty()) {
            (false, false) => format!("{prev}/{next}"),
            (false, true) => prev,
            (true, false) => next,
            (true, true) => String::new(),
        };
        spans.push(Span::styled(format!("  {lane_glyph}:lanes"), live));
    }
    push_live(&mut spans, footer_key_glyph(&kb.sort_rebind), "sort", "  ");
    push_live(&mut spans, primary_key(&kb.cycle_sort), "cycle-sort", "  ");
    push_live(&mut spans, primary_key(&kb.hide_done), "hide-done", "  ");
    push_live(
        &mut spans,
        footer_key_glyph(&kb.lifecycle_filter),
        "filter",
        "  ",
    );
    push_live(&mut spans, primary_key(&kb.search), "search ", "  ");

    let plain: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let plain_w = UnicodeWidthStr::width(plain.as_str());
    let max = width as usize;
    if plain_w > max {
        // Overflow: collapse to a single truncated span (keeps quit..prefix).
        return Line::from(Span::styled(fit_footer_width(&plain, width), live));
    }
    // Center the globals line in the available width.
    let pad = max.saturating_sub(plain_w) / 2;
    if pad > 0 {
        let mut centered = Vec::with_capacity(spans.len() + 1);
        centered.push(Span::raw(" ".repeat(pad)));
        centered.extend(spans);
        Line::from(centered)
    } else {
        Line::from(spans)
    }
}

/// Fit a plain footer string to `width` columns (grapheme-safe).
fn fit_footer_width(text: &str, width: u16) -> String {
    let max = width as usize;
    if UnicodeWidthStr::width(text) <= max {
        return text.to_string();
    }
    let mut used = 0usize;
    let mut out = String::new();
    for g in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let next = used + UnicodeWidthStr::width(g);
        if next > max {
            break;
        }
        out.push_str(g);
        used = next;
    }
    out
}

/// Paint the two-line footer.
///
/// Layout — per-tab keys on top, globals on bottom, both centered:
/// - Line 1 (top, adjacent to content) = per-tab keys for the active lane.
/// - Line 2 (bottom) = globals (always-available keybinds).
///
/// Both lines are centered in `footer_area.width` so the legend reads
/// as a single balanced block instead of left-anchored noise. Flash /
/// quitting messages still span the full `footer_area` as one Paragraph.
/// When `footer_area.height < 2`, fall back to globals only.
pub(super) fn render_footer(frame: &mut Frame, app: &App, view: &ViewState) {
    let area = view.footer_area;
    if area.width == 0 || area.height == 0 {
        return;
    }
    // M183 F-03: Paragraph only writes text cells; clear the full
    // footer_area first so short flash/globals never leave prior
    // chrome (dashboard borders, etc.) on unpainted cells.
    frame.render_widget(Clear, area);
    let dim = Style::default().fg(app.effective_palette().dim);

    if app.quitting {
        let msg = centered_plain(" Quitting... ", area.width);
        frame.render_widget(Paragraph::new(Span::styled(msg, dim)), area);
        return;
    }
    if let Some(ref msg) = app.flash_message {
        let footer_text = crate::tui::flash_message::format_flash_footer_with_details(
            msg,
            area.width,
            app.last_action_error.is_some(),
        );
        let centered = centered_plain(&footer_text, area.width);
        frame.render_widget(Paragraph::new(Span::styled(centered, dim)), area);
        return;
    }

    if area.height < 2 {
        let globals = footer_globals_line(app, area.width);
        frame.render_widget(Paragraph::new(globals), area);
        return;
    }

    // Per-tab on top, globals on bottom. Per-tab is the contextual line
    // for the active lane; globals is the always-available baseline.
    let per_tab_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let globals_area = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: 1,
    };

    let per_tab = centered_or_truncated(&footer_for(app), area.width);
    frame.render_widget(Paragraph::new(Span::styled(per_tab, dim)), per_tab_area);
    frame.render_widget(
        Paragraph::new(footer_globals_line(app, area.width)),
        globals_area,
    );
}

/// Center `text` in `width` columns. When the text overflows, fall back
/// to grapheme-safe truncation via [`fit_footer_width`].
fn centered_or_truncated(text: &str, width: u16) -> String {
    let max = width as usize;
    let text_w = UnicodeWidthStr::width(text);
    if text_w >= max {
        return fit_footer_width(text, width);
    }
    let pad = max.saturating_sub(text_w) / 2;
    format!("{}{}", " ".repeat(pad), text)
}

/// Same as [`centered_or_truncated`] but never truncates — used for
/// short modal-internal / flash lines that should always render in full,
/// centered.
fn centered_plain(text: &str, width: u16) -> String {
    let max = width as usize;
    let text_w = UnicodeWidthStr::width(text);
    if text_w >= max {
        return text.to_string();
    }
    let pad = max.saturating_sub(text_w) / 2;
    format!("{}{}", " ".repeat(pad), text)
}

pub(super) fn view_title(app: &App) -> &str {
    match (&app.active_lane, &app.content) {
        (_, ContentState::List) => app.active_lane.label(),
        (_, ContentState::MilestoneDetail) => {
            if matches!(app.active_mode, Mode::ReviewMenu(_)) {
                "Milestone Detail [Review Menu]"
            } else {
                "Milestone Detail"
            }
        }
        (_, ContentState::AnnotationThread) => "Annotations",
        (_, ContentState::CoApproval) => "Co-Approval",
        (_, ContentState::BacklogDetail) => "Backlog Detail",
    }
}

/// M185/M186: filter chip text for the header (without the leading " · ").
/// `None` when not on a list lane. Renamed from `milestones_filter_chip`
/// (M186 F-04) — now serves Milestones, Backlog, and Ideas.
pub(super) fn list_lane_filter_chip(app: &App) -> Option<String> {
    let on_list_lane = matches!(
        app.active_lane,
        Lane::Milestones | Lane::Backlog | Lane::Ideas
    ) && app.content == ContentState::List;
    if !on_list_lane {
        return None;
    }
    // Milestones-only: also carry lifecycle filter segment.
    let mut segments: Vec<String> = Vec::new();
    if app.active_lane == Lane::Milestones && !app.milestone_filter.is_empty() {
        let parts: Vec<&str> = app.milestone_filter.iter().map(String::as_str).collect();
        segments.push(parts.join(", "));
    }
    let term = app.lane_search_term();
    if !term.is_empty() {
        segments.push(format!("/{term}"));
    }
    let label = if segments.is_empty() {
        "All".to_string()
    } else {
        segments.join(" · ")
    };
    // M186 F-04: count is per-lane. visible_milestones is *not* lane-gated
    // (it returns the full milestone list even on Backlog/Ideas), so we
    // branch on the active lane rather than summing both helpers.
    let count = if app.active_lane == Lane::Milestones {
        app.visible_milestones().len()
    } else {
        app.visible_backlog().len()
    };
    Some(format!("{label} ({count})"))
}

// M135 F-03: `centered_rect` lives in `view_state` and is now called ONLY
// there (in `compute_view`). The overlay renderers below receive the
// pre-computed `view.overlay_rect` via `overlay_rect_or`, so this module
// no longer duplicates the overlay math.

/// M135 F-03: resolve the overlay rect to render at. `compute_view`
/// populates `view.overlay_rect` for the help / input / review-menu cases,
/// and that field is the single source of truth for overlay geometry.
/// Falls back to a fresh `centered_rect` computation only when the view
/// did not populate the field (defensive - e.g. a future overlay type
/// added to render without a matching `compute_view` branch).
pub(super) fn overlay_rect_or(view: &ViewState, area: Rect) -> Rect {
    view.overlay_rect
        .unwrap_or_else(|| crate::tui::view_state::centered_rect(60, 60, area))
}
