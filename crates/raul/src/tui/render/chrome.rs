use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::scrollbar::Set as ScrollbarSet;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::tui::app::{App, ContentState, Lane};
use crate::tui::key_combo::format_key_combo;
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
    // automatically. M199: the per-(lane, content_state) table lives
    // in `Keybinds::footer_per_tab`. Modal footers (LifecycleFilter,
    // SearchInput, SortRebind) keep their dedicated single-line text
    // per design decision D-07 — `render_footer` early-returns to
    // the per-tab area only, never to the globals area, when one of
    // these is open (preserves the existing modal UX).
    if matches!(app.active_mode, Mode::LifecycleFilter(_)) {
        return " ↑↓ navigate · Space toggle · ⏎ commit · Esc revert ".to_string();
    }
    if matches!(app.active_mode, Mode::SearchInput(_)) {
        return " type to filter · ⏎ commit · Esc cancel ".to_string();
    }
    if app.sort_rebind_open() {
        return " ↑↓ cycle · ⏎ bind · Esc cancel ".to_string();
    }
    // ReviewMenu (m:open) and AnnotationThread mode are routed
    // through the per-(lane, content_state) table — both have
    // their own entries (`_:annotate · _:resolve · _:reopen` and
    // `_:approve · _:menu` respectively).
    let settings_staged = app.settings.as_ref().is_some_and(|s| s.has_staged_edits());
    let mut text =
        app.keybinds
            .footer_per_tab(app.active_lane, app.content, app.open_only, settings_staged);
    // M205 AC-06: the per-tab footer carries a trailing
    // `sort: <key> ▼` indicator on the three sort-bearing lanes
    // (Milestones / Backlog / Ideas), showing the active sort
    // key. The arrow matches the column-header arrow glyph used
    // in `header_cell`, so the operator sees the same visual
    // affordance on the column header and the footer.
    if matches!(
        app.active_lane,
        Lane::Milestones | Lane::Backlog | Lane::Ideas
    ) && app.content == ContentState::List
    {
        let key = app.lane_sort_key(app.active_lane);
        let indicator = format!("sort: {} ▼", key.label());
        if text.is_empty() {
            text = format!(" {indicator} ");
        } else {
            // Append after the existing per-tab text — separator
            // matches the existing `·` between affordances.
            text.push_str(&format!("  ·  {indicator}"));
        }
    }
    text
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
/// M199: globals line — the six keys that fire on every
/// (lane, content_state, mode) combination. Lane-conditional items
/// (`F:filter`, `/:search`, `h:hide-done`, `S:sort`, `o:cycle`) move
/// to the per-tab line; the globals row now only carries the truly
/// universal bindings (per design decision D-02).
///
/// Truncates to `width` columns when the full list would overflow
/// (collapse-to-prefix behavior — the load-bearing globals keep
/// their leftmost tokens; per design decision D-06).
pub(super) fn footer_globals_line(app: &App, width: u16) -> Line<'static> {
    let kb = &app.keybinds;
    let palette = app.effective_palette();
    let live = Style::default().fg(palette.dim);

    // M199 D-05: globals uses the same `·` separator as the per-tab
    // row so the two rows read as one balanced block. Each entry
    // is a standalone "glyph:label" string; the join with `·` happens
    // here rather than inside the per-span leading-space format.
    let mut entries: Vec<String> = Vec::with_capacity(7);
    if !primary_key(&kb.quit).is_empty() {
        entries.push(format!(" {}:quit", primary_key(&kb.quit)));
    }
    if !primary_key(&kb.help).is_empty() {
        entries.push(format!(" {}:help", primary_key(&kb.help)));
    }
    // Spec surfaces Tab / Shift+Tab for lane switch even though the
    // primary bindings are ←/→ (multi-binding aliases stay first for
    // resolve order). Prefer those display glyphs when present.
    let prev = preferred_key(&kb.previous_lane, |c| format_key_combo(c) == "Shift+Tab");
    let next = preferred_key(&kb.next_lane, |c| format_key_combo(c) == "Tab");
    if !prev.is_empty() || !next.is_empty() {
        let lane_glyph = match (prev.is_empty(), next.is_empty()) {
            (false, false) => format!("{prev}/{next}"),
            (false, true) => prev,
            (true, false) => next,
            (true, true) => String::new(),
        };
        entries.push(format!(" {lane_glyph}:lanes"));
    }
    // ↑↓/jk:move — both keys live in `kb.up` / `kb.down` so we show
    // a compact "↑↓/<glyph>:move" label using whichever glyph the up
    // binding surfaces first. Footer is width-constrained so the
    // explicit two-glyph label is the right tradeoff.
    let move_glyph = primary_key(&kb.up);
    if !move_glyph.is_empty() {
        entries.push(format!(" ↑↓/{move_glyph}:move"));
    }
    let enter_glyph = primary_key(&kb.enter);
    if !enter_glyph.is_empty() {
        // Display the Enter binding as "<Enter>:go" per design
        // decision D-02 (verb "go" over "drill" / "select" / "open").
        entries.push(format!(" {enter_glyph}:go"));
    }
    if !primary_key(&kb.refresh).is_empty() {
        entries.push(format!(" {}:refresh", primary_key(&kb.refresh)));
    }
    let plain = entries.join(" · ");
    let plain_w = UnicodeWidthStr::width(plain.as_str());
    let max = width as usize;
    if plain_w > max {
        // Overflow: collapse to a single truncated span (keeps
        // quit..prefix). M199 D-06: this is the *globals* path; the
        // per-tab line uses right-truncate-with-… instead.
        return Line::from(Span::styled(fit_footer_width(&plain, width), live));
    }
    // Center the globals line in the available width.
    let pad = max.saturating_sub(plain_w) / 2;
    if pad > 0 {
        Line::from(vec![Span::raw(" ".repeat(pad)), Span::styled(plain, live)])
    } else {
        Line::from(Span::styled(plain, live))
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
/// M199 layout — globals on top, per-tab on bottom, both centered:
/// - Row h-2 (top, closer to the title bar) = globals (the six
///   universal keys; identical on every tab so the eye learns it
///   once and stops re-reading it on every tab switch).
/// - Row h-1 (bottom, adjacent to the terminal edge) = per-tab keys
///   for the active lane (the contextual delta from the global
///   baseline; lane-conditional only).
///
/// Both lines are centered in `footer_area.width` and use the same
/// dim color + ` · ` separator so the legend reads as a single
/// balanced block. Flash / quitting messages still span the full
/// `footer_area` as one Paragraph. When the per-tab string is empty
/// (Path, Watch) the renderer drops the per-tab row and the footer
/// is 1 row tall (`compute_view` is the single source of truth for
/// `footer_area.height`). When `footer_area.height < 2`, fall back
/// to globals only.
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

    // M199: globals on top (h-2), per-tab on bottom (h-1).
    let globals_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let per_tab_area = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: 1,
    };

    // M199: per-tab is the contextual line for the active
    // (lane, content_state) pair, sourced from `Keybinds::footer_per_tab`
    // so the help overlay and the footer share a single source of
    // truth. Right-truncate with `…` on overflow (per design
    // decision D-07) — the contextual line is forgiving when the
    // rightmost (least-discoverable) entry clips.
    let per_tab_text = footer_for(app);
    let per_tab = right_truncate_with_ellipsis(&per_tab_text, area.width);
    let per_tab = centered_plain(&per_tab, area.width);
    frame.render_widget(
        Paragraph::new(footer_globals_line(app, area.width)),
        globals_area,
    );
    frame.render_widget(Paragraph::new(Span::styled(per_tab, dim)), per_tab_area);
}

/// M199: right-truncate `text` to fit within `width` columns, appending
/// a single `…` (U+2026) when truncation occurs. Grapheme-safe via
/// `unicode_segmentation` (matches `fit_footer_width`'s behavior) and
/// uses `unicode_width` for column accounting. The reserved 1 column
/// for the `…` keeps the result within `width` exactly — when the
/// input is already ≤ width, the helper is a no-op.
pub(super) fn right_truncate_with_ellipsis(text: &str, width: u16) -> String {
    let max = width as usize;
    if max == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max {
        return text.to_string();
    }
    // Reserve one column for the ellipsis so the result fits exactly.
    let budget = max.saturating_sub(1);
    let mut used = 0usize;
    let mut out = String::new();
    for g in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let next = used + UnicodeWidthStr::width(g);
        if next > budget {
            break;
        }
        out.push_str(g);
        used = next;
    }
    out.push('\u{2026}');
    out
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
    if app.active_lane == Lane::Milestones {
        let lf = app.lifecycle_filter_set();
        if !lf.is_empty() {
            let parts: Vec<&str> = lf.iter().map(String::as_str).collect();
            segments.push(parts.join(", "));
        }
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

// M199 S3: unit tests for `right_truncate_with_ellipsis`. The
// spec calls for a test that truncates at multiple widths and
// asserts the result ends with `…` and the leftmost tokens are
// preserved. Lives inside the module so we don't have to make
// the helper `pub` for the integration test alone.
#[cfg(test)]
mod tests {
    use super::right_truncate_with_ellipsis;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn right_truncate_no_op_when_text_fits() {
        let s = " F:filter  ·  /:search ";
        // Width >= natural width returns the input unchanged.
        assert_eq!(right_truncate_with_ellipsis(s, s.len() as u16 + 5), s);
    }

    #[test]
    fn right_truncate_appends_ellipsis_and_keeps_leftmost_tokens() {
        // 60 cols (the spec's "narrow" width). Milestones's
        // natural per-tab string is ~95 cols; truncated to 60
        // should end with `…` and keep `F:filter` (leftmost
        // token).
        let s = " F:filter  ·  /:search  ·  h:hide-done  ·  Shift+s:sort  ·  o:cycle  ·  Shift+a:annotate ";
        let r = right_truncate_with_ellipsis(s, 60);
        assert!(r.ends_with('\u{2026}'), "must end with `…`; got: {r:?}");
        assert!(
            r.contains("F:filter"),
            "leftmost token must survive; got: {r:?}"
        );
        assert!(
            UnicodeWidthStr::width(r.as_str()) <= 60,
            "truncated width must fit in 60 cols; got: {}",
            UnicodeWidthStr::width(r.as_str())
        );
    }

    #[test]
    fn right_truncate_at_80_and_120() {
        let s = " F:filter  ·  /:search  ·  h:hide-done  ·  Shift+s:sort  ·  o:cycle  ·  Shift+a:annotate ";
        let natural = UnicodeWidthStr::width(s);
        for width in [80u16, 120] {
            let r = right_truncate_with_ellipsis(s, width);
            if natural > width as usize {
                assert!(
                    r.ends_with('\u{2026}'),
                    "width {width} overflowing: must end with `…`; got: {r:?}"
                );
            } else {
                assert_eq!(r, s, "width {width} fits natural; got: {r:?}");
            }
        }
    }

    #[test]
    fn right_truncate_with_zero_width_returns_empty() {
        // Defensive: width 0 is not a realistic terminal width,
        // but the helper should still return an empty string
        // rather than panic.
        assert_eq!(right_truncate_with_ellipsis("anything", 0), "");
    }

    #[test]
    fn right_truncate_preserves_wide_graphemes_atomically() {
        // CJK characters are 2 columns each. Truncation must
        // not split a wide character; either include it whole
        // or drop it.
        let s = "中文中文中文"; // 6 columns
        let r = right_truncate_with_ellipsis(s, 4);
        // Reserve 1 col for `…`, so budget is 3 cols. 3 cols is
        // not a whole number of CJK characters, so we keep
        // just one (2 cols) and append `…`. Total 3 cols.
        assert!(r.ends_with('\u{2026}'), "must end with `…`; got: {r:?}");
        assert!(
            UnicodeWidthStr::width(r.as_str()) <= 4,
            "truncated CJK must fit in 4 cols; got: {}",
            UnicodeWidthStr::width(r.as_str())
        );
    }
}
