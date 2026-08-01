//! M137 + BF-01: scrollbar math + geometry.
//!
//! Reserves a 1-column gutter on the right edge of every scrollable
//! region. The render itself uses `ratatui::widgets::Scrollbar` +
//! `ScrollbarState` (BF-01 migration) and is dispatched from
//! `chrome.rs::render_scrollbars`. This module owns:
//!
//! * `SCROLLBAR_GUTTER = 1` — the constant other modules use to size
//!   their panel guts.
//! * `scrollbar_rect(panel, gutter)` — pure geometry, returns the
//!   right-edge gutter rect given a panel.
//! * `thumb_range` / `thumb_range_visible` — pure thumb math (the
//!   `_visible` variant takes an explicit content-units viewport size
//!   so board columns can count card-slots instead of cells). Kept as
//!   helpers so future tests / click handlers can reason about the
//!   same shapes the framework uses.
//! * `track_click_to_scroll` — pure linear interpolation over the
//!   full track for AC-04 track-click jumps (click y → scroll index).
//!   Intentionally NOT the mathematical inverse of `thumb_range`: the
//!   click maps across the whole track height so every row is reachable
//!   with no dead zones, whereas thumb position uses a reduced track
//!   span. Same intent, different denominators.
//! * `measure_paragraph_height` — M167 helper for the milestone detail
//!   scrollbar fix (rendered Paragraph into a buffer; walks rows for
//!   the last non-blank). Unrelated to widget identity; stayed put.
//!
//! The hand-rolled buffer writer (`render_scrollbar_gutter` /
//! `render_scrollbar_gutter_visible`) was retired by BF-01 in favor of
//! `ratatui::widgets::Scrollbar`. The remaining helpers stay as pure
//! functions for callers that want them (mouse hit-testing, ad-hoc
//! buffer inspection in tests).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// M167 + M169-rev scrollbar: measure the post-render row count of a
/// `Paragraph`.
///
/// **Important — measurement area must be tall enough to fit the full
/// wrapped content.** The previous version rendered into a buffer the
/// size of the visible panel, which made the bottom border the last
/// non-blank row and returned the *panel height* (capping the scrollbar
/// at `panel.height - visible ≈ 2` for any milestone that overflowed).
/// The dogfood-log entry 32 / repro in
/// `crates/raul/tests/m169_scroll_repro.rs` pins this fix.
///
/// Algorithm:
///   1. Render into a buffer 8× the panel height (capped to keep the
///      allocation reasonable). 8× is plenty for any realistic detail
///      panel — milestone detail wraps to ~50–80 rows from ~80 logical
///      lines; a 20-row panel needs ~160 rows to cover that.
///   2. Walk bottom-up to find the bottom-border row (`─` row just
///      after the content).
///   3. Subtract `top_border + bottom_border = 2` rows from the result
///      to get the inner content row count.
///
/// Edge cases:
///   * `area.width == 0` or `area.height == 0` → `0`.
///   * Paragraph renders nothing visible → `0`.
///   * Otherwise: `(bottom_border_row) - 1 - top_border` rows of
///     content (i.e. `bottom_border_row - 1`).
pub fn measure_paragraph_height(paragraph: ratatui::widgets::Paragraph<'_>, area: Rect) -> u16 {
    use ratatui::widgets::Widget;
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    // Measure absolute content extent; the user's scroll offset is applied
    // separately by the renderer.
    let measure_paragraph = paragraph.scroll((0, 0));
    // Allocate a tall enough buffer that the bottom border does NOT
    // hit the buffer edge unless content truly exceeds 8× panel
    // height. The Paragraph's Block paints top + bottom borders;
    // walking bottom-up we want the *bottom border* row (the `─...─`
    // line just after the last content row), not the buffer edge.
    let measure_height = area.height.saturating_mul(8).clamp(64, 8 * 1024);
    let measure_area = Rect {
        x: 0,
        y: 0,
        width: area.width,
        height: measure_height,
    };
    let mut buf = Buffer::empty(measure_area);
    measure_paragraph.render(measure_area, &mut buf);

    // Detect whether the Paragraph has a block by inspecting the
    // top-left cell. With block, the top-left corner is one of the
    // box-drawing corners (`┌` U+250C, `┏` U+250F, `╔` U+2554,
    // `╭` U+256D). Without block, row 0 col 0 is content (or blank).
    // This decides whether rows 0 and H-1 are reserved for borders
    // (with-block) or are content rows (without-block).
    let has_block = measure_area.width > 0
        && measure_area.height > 0
        && matches!(
            buf[(0, 0)].symbol().chars().next(),
            Some('\u{250C}') | Some('\u{250F}') | Some('\u{2554}') | Some('\u{256D}')
        );

    // Measure the absolute extent from the first content row through the
    // last non-blank, non-border row. This includes blank separators between
    // sections while excluding unreachable trailing blank rows. Side borders
    // are ignored because a Block paints them below the content as well.
    //
    // Box-drawing chars live in U+2500..=U+257F and include the
    // horizontal rules (`─` U+2500, `━` U+2501, `═` U+2550), vertical
    // rules (`│` U+2502, `┃` U+2503, `║` U+2551), and corners/joints
    // (`┌┐└┘`, `┏┓┗┛`, `╔╗╚╝`, `╭╮╰╯`, `├┤┬┴┼`, etc.).
    if measure_area.width < 2 || measure_area.height < 2 {
        return 0;
    }
    let last_row = if has_block {
        measure_area.height.saturating_sub(2)
    } else {
        measure_area.height.saturating_sub(1)
    };
    let first_row: u16 = if has_block { 1 } else { 0 };

    let mut last_content_row: Option<u16> = None;
    for y in (first_row..=last_row).rev() {
        let mut has_non_border_content = false;
        for x in 1..measure_area.width.saturating_sub(1) {
            let sym = buf[(x, y)].symbol();
            if sym.is_empty() || sym == " " {
                continue;
            }
            let Some(c) = sym.chars().next() else {
                continue;
            };
            let cp = c as u32;
            if (0x2500..=0x257F).contains(&cp) {
                // Box-drawing glyph — border, not content.
                continue;
            }
            has_non_border_content = true;
            break;
        }
        if has_non_border_content {
            last_content_row = Some(y);
            break;
        }
    }
    // The last content row's index (1-based with block, 0-based without)
    // is also the content row count, since rows are 0..=last_row with
    // the top border at row 0 and the bottom border at row H-1 (or rows
    // 0..H for no-block).
    last_content_row
        .map(|r| r.saturating_sub(first_row) + 1)
        .unwrap_or(0)
}

/// M137: width of the right-edge scrollbar gutter. Always 1 — keeping
/// it constant prevents the terminal grid from reflowing when the
/// overflow state flips (otherwise content would widen by `1` the
/// moment a track appears).
pub const SCROLLBAR_GUTTER: u16 = 1;

/// Pure geometry: where does the scrollbar track live, given the
/// panel's full rect (borders included) and the gutter width?
///
/// The last `gutter` columns of `panel` are reserved. The returned
/// rect is non-empty (height ≥ 1, width = `gutter`) whenever the panel
/// is wide enough — i.e. `panel.width ≥ gutter`. Below that threshold
/// the caller skips rendering.
pub fn scrollbar_rect(panel: Rect, gutter: u16) -> Rect {
    let g = gutter.min(panel.width);
    Rect {
        x: panel.x.saturating_add(panel.width.saturating_sub(g)),
        y: panel.y,
        width: g,
        height: panel.height,
    }
}

/// Pure thumb math. Returns `Some((thumb_height, thumb_y))` only when
/// the content actually overflows.
///
/// * `area_height` — height of the scrollbar track in cells (≥ 1).
/// * `scroll` — current top row index (0-based).
/// * `total` — total number of rows in the scrollable content.
///
/// Edge cases:
///
/// * `area_height == 0` or `total == 0` → `None` (no track / empty).
/// * `total ≤ area_height` → `None` (content fits, no thumb).
/// * `thumb_height ≥ 1` always (even when `visible` is huge and
///   `total` is enormous, the thumb cannot collapse below 1 cell).
/// * `thumb_y` is clamped to `0..=area_height - thumb_height` so a
///   slightly-too-large scroll value doesn't render the thumb past the
///   bottom of the track.
pub fn thumb_range(area_height: u16, scroll: usize, total: usize) -> Option<(u16, u16)> {
    thumb_range_visible(area_height, scroll, total, area_height as usize)
}

/// Like [`thumb_range`], but `visible` is the number of content units
/// that fit in the viewport (card-slots for the board, rows for lists).
/// `track_h` is still the paint height of the gutter in cells.
pub fn thumb_range_visible(
    track_h: u16,
    scroll: usize,
    total: usize,
    visible: usize,
) -> Option<(u16, u16)> {
    if track_h == 0 || total == 0 || visible == 0 {
        return None;
    }
    if total <= visible {
        return None;
    }
    let thumb_height = std::cmp::max(1, (track_h as usize * visible) / total) as u16;
    let span = total.saturating_sub(visible).max(1);
    let track_room = (track_h as usize).saturating_sub(thumb_height as usize);
    let max_scroll = total.saturating_sub(visible);
    let clamped_scroll = scroll.min(max_scroll);
    let raw_y = (clamped_scroll * track_room) / span;
    let thumb_y = (raw_y as u16).min((track_h - thumb_height) as u16);
    Some((thumb_height, thumb_y))
}

/// Pure helper: clicking at relative `y_in_track` (0..track_h) inside
/// a scrollable region whose total content is `total` units should
/// resolve to a new `scroll` value.
///
/// This is a linear interpolation over the *full* track height
/// (`new_scroll = round(y / (track_h - 1) * (total - 1))`), chosen so every
/// content row is reachable from some click position. It is deliberately
/// NOT the mathematical inverse of [`thumb_range`]'s positioning math,
/// which maps scroll across the reduced span `track_h - thumb_height`; a
/// true inverse would create dead zones at the ends of the track. The
/// behaviors agree at the extremes (top → 0, bottom → max scroll).
pub fn track_click_to_scroll(track_h: u16, y_in_track: u16, total: usize) -> usize {
    if track_h <= 1 || total <= 1 {
        return 0;
    }
    let max_scroll = total.saturating_sub(1);
    let y = (y_in_track as usize).min((track_h - 1) as usize);
    let denom = (track_h - 1) as usize;
    (y * max_scroll + denom / 2) / denom
}

/// Render the track + optional thumb into `buf` at `track_rect`. Caller
/// owns the buffer; nothing is allocated.
///
/// Style choices (locked in to keep tests stable):
///
/// * Track: `│` (U+2502) in dim foreground — looks like a light
///   vertical rail that's always visible.
/// * Thumb: `█` (U+2588) in accent foreground, black background,
///   bold — a filled block that pops against the track.
///
/// Both glyphs are exactly 1 cell wide so they line up with the
/// single-column gutter perfectly.
///
#[cfg(test)]
mod tests {
    use super::*;

    /// AC-02: thumb size = max(1, area_height * visible / total).
    /// For an area of 10 with total 100 (10x), thumb ≈ 1 cell
    /// (10*10/100=1).
    #[test]
    fn thumb_size_thousandth() {
        assert_eq!(thumb_range(10, 0, 100), Some((1, 0)));
    }

    /// 5-row track with 20 rows of content. thumb = 5*5/20 = 1.
    /// thumb_y = (scroll * (5-1)) / max(1, 20-5) = scroll * 4 / 15.
    #[test]
    fn thumb_size_fits_one() {
        let r = thumb_range(5, 0, 20);
        assert_eq!(r, Some((1, 0)));
    }

    /// AC-02: 10-row track with 20 rows: thumb = 10*10/20 = 5.
    /// scroll=0 → thumb_y=0; scroll=10 → thumb_y=(10*5)/10=5.
    #[test]
    fn thumb_half_size_fits() {
        assert_eq!(thumb_range(10, 0, 20), Some((5, 0)));
        assert_eq!(thumb_range(10, 5, 20), Some((5, 2)));
        assert_eq!(thumb_range(10, 10, 20), Some((5, 5)));
    }

    /// AC-03: visible >= total → no thumb.
    #[test]
    fn no_thumb_when_fits() {
        assert_eq!(thumb_range(20, 0, 5), None);
        assert_eq!(thumb_range(20, 0, 20), None);
    }

    /// Edge: total = 0.
    #[test]
    fn no_thumb_when_empty() {
        assert_eq!(thumb_range(10, 0, 0), None);
    }

    /// Edge: area_height = 0 → no track, no thumb.
    #[test]
    fn no_track_when_height_zero() {
        assert_eq!(thumb_range(0, 0, 100), None);
    }

    /// thumb_y never exceeds (area_height - thumb_height), even when
    /// scroll saturates past total - visible.
    #[test]
    fn thumb_y_clamped_to_track_bottom() {
        let (h, y) = thumb_range(3, 999, 100).unwrap();
        assert_eq!(h, 1);
        assert_eq!(y, 2, "thumb_y must clamp to 3-1=2");
    }

    /// AC-01: gutter width is exactly 1.
    #[test]
    fn gutter_width_is_one() {
        assert_eq!(SCROLLBAR_GUTTER, 1);
    }

    /// AC-01: gutter lives at the right edge of the panel.
    #[test]
    fn gutter_at_right_edge() {
        let panel = Rect::new(10, 5, 30, 12);
        let track = scrollbar_rect(panel, SCROLLBAR_GUTTER);
        assert_eq!(track.x, 39);
        assert_eq!(track.width, 1);
        assert_eq!(track.y, 5);
        assert_eq!(track.height, 12);
    }

    /// AC-03: gutter is reserved even when content fits (no overflow
    /// → thumb is None, but `scrollbar_rect` still returns the gutter).
    #[test]
    fn gutter_reserved_when_no_overflow() {
        let panel = Rect::new(0, 0, 20, 10);
        let track = scrollbar_rect(panel, SCROLLBAR_GUTTER);
        assert_eq!(track, Rect::new(19, 0, 1, 10));
        assert_eq!(thumb_range(track.height, 0, 5), None);
    }

    /// Scroll past end clamps: scroll = total should put thumb at bottom.
    #[test]
    fn scroll_past_end_clamps() {
        // 10 rows visible, 20 total. thumb=5. track_room=5.
        // scroll = 20 → clamped to 10 → raw_y = (10*5)/10=5 → thumb_y=5.
        assert_eq!(thumb_range(10, 20, 20), Some((5, 5)));
        assert_eq!(thumb_range(10, 11, 20), Some((5, 5)));
    }

    /// Empty panel (0-width) → no track.
    #[test]
    fn empty_panel() {
        let panel = Rect::new(0, 0, 0, 0);
        let track = scrollbar_rect(panel, SCROLLBAR_GUTTER);
        assert_eq!(track.width, 0);
        assert_eq!(track.height, 0);
    }
}
