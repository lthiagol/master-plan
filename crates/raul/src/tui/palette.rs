//! Semantic palette helpers keep status and selection colors consistent
//! across every renderer.
//!
//! Contract:
//! - `status_color(role)` → palette accent (in-progress), success (complete),
//!   danger (blocked), warn (approved/ready), dim (other).
//! - `selection_fg()` / `selection_bg()` → high-contrast selection row pair.
//! - `branch_marker()` / `branch_continuation()` → tree branch glyphs in dim.

use ratatui::style::Color;

use crate::theme::Palette;

/// Map a status role (lifecycle / inbox / etc.) to its semantic
/// palette color. The mapping matches the M167 contract:
/// in-progress → accent, complete → success, blocked → danger,
/// approved/ready → warn, anything else → dim.
pub fn status_color(lifecycle: &str, palette: &Palette) -> Color {
    match lifecycle {
        "in-progress" => palette.accent,
        "complete" => palette.success,
        "blocked" => palette.danger,
        "approved" | "ready" => palette.warn,
        "cancelled" => palette.dim,
        _ => palette.dim,
    }
}

/// Foreground color for a row that lives on a colored background
/// (e.g. Board view's status-color fill, the legacy selected row in
/// Backlog). The selected row needs a high-contrast fg that reads on
/// any accent color — pure black is the standard choice.
pub fn on_accent_fg(palette: &Palette) -> Color {
    // Foreground on the accent background: prefer pure black for
    // contrast; fall back to foreground on a monochrome palette.
    if palette.accent == Color::Reset {
        palette.foreground
    } else {
        Color::Black
    }
}

/// Cursor/caret block style for the input overlay. Convention is a
/// white-bg / black-fg block with BOLD — readable on every palette.
/// Returns `(background, foreground)` so callers can build their own
/// `Style` without re-declaring the colors.
pub fn caret_block(_palette: &Palette) -> (Color, Color) {
    (Color::White, Color::Black)
}

/// M172 S4 (F-04): the selected-row highlight color in the Board view.
/// White has the highest contrast against every palette so a
/// selected box stands out regardless of theme. Centralized here so
/// the audit grep stays at zero hits outside this module — every
/// other render path uses palette helpers.
pub fn selection_border(_palette: &Palette) -> Color {
    Color::White
}

/// Background color for the selected row in a list (Backlog /
/// Milestones). Mirrors `effective_palette().accent`.
pub fn selection_bg(palette: &Palette) -> Color {
    palette.accent
}

/// Foreground color for the selected row (high contrast against the
/// selection bg).
pub fn selection_fg(_palette: &Palette) -> Color {
    Color::Black
}

/// Tree-branch glyph used as the "last sibling" marker (`└─`) in dim
/// (so the marker recedes; the milestone id carries the lifecycle
/// color).
pub fn branch_marker(palette: &Palette) -> (Color, &'static str) {
    (palette.dim, "└─")
}

/// Tree-branch glyph used as the "more siblings follow" marker
/// (`├─`) in dim.
pub fn branch_continuation(palette: &Palette) -> (Color, &'static str) {
    (palette.dim, "├─")
}

/// Tree-branch vertical continuation (`│`) in dim.
pub fn branch_vertical(palette: &Palette) -> (Color, &'static str) {
    (palette.dim, "│")
}

/// Header color (Path trunk label "EXECUTION", lane section headers).
pub fn header_color(palette: &Palette) -> Color {
    palette.accent
}

/// Error / warning message color (rendered in lane empty-state blocks).
pub fn warn_color(palette: &Palette) -> Color {
    palette.warn
}

/// Neutral / dim message color (the "no path data — press r" hint).
pub fn dim_color(palette: &Palette) -> Color {
    palette.dim
}

/// Default body foreground (text reads on the lane background).
pub fn body_color(palette: &Palette) -> Color {
    palette.foreground
}

/// "Row is empty / no item" placeholder color.
pub fn placeholder_color(palette: &Palette) -> Color {
    palette.dim
}

/// Review-menu overlay backdrop. Distinct from the lane content so
/// the modal visually floats above the lane chrome.
pub fn overlay_backdrop(_palette: &Palette) -> Color {
    Color::DarkGray
}

/// Review-menu selected-item foreground (white on the accent bg).
pub fn overlay_selected_fg(_palette: &Palette) -> Color {
    Color::Black
}

/// Review-menu selected-item background (the accent — same color as
/// row selection so a reviewer switches between modes without
/// re-learning colors).
pub fn overlay_selected_bg(palette: &Palette) -> Color {
    palette.accent
}

/// Map a milestone lifecycle to a tree-view id color. Mirrors
/// `milestone_tree::lifecycle_color` — kept as a separate helper so
/// the tree module doesn't need to depend on this palette module.
pub fn tree_id_color(lifecycle: &str, palette: &Palette) -> Color {
    status_color(lifecycle, palette)
}

/// Convert a crossterm `Color` to the ratatui `Color` equivalent.
/// Centralized here so the audit grep for direct color literals
/// returns zero hits outside this module — the conversion is the
/// ONE place a direct `Color::*` lookup is allowed.
///
/// Note: crossterm uses British spelling (`Grey`) while ratatui uses
/// American spelling (`Gray`). The conversion maps across.
pub fn crossterm_to_ratatui(c: crossterm::style::Color) -> Color {
    use crossterm::style::Color as Ct;
    match c {
        Ct::Reset => Color::Reset,
        Ct::Black => Color::Black,
        Ct::DarkGrey => Color::DarkGray,
        Ct::DarkRed | Ct::Red => Color::LightRed,
        Ct::DarkGreen | Ct::Green => Color::LightGreen,
        Ct::DarkYellow | Ct::Yellow => Color::LightYellow,
        Ct::DarkBlue | Ct::Blue => Color::LightBlue,
        Ct::DarkMagenta | Ct::Magenta => Color::LightMagenta,
        Ct::DarkCyan | Ct::Cyan => Color::LightCyan,
        Ct::Grey => Color::Gray,
        Ct::White => Color::White,
        Ct::AnsiValue(v) => Color::Indexed(v),
        Ct::Rgb { r, g, b } => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::MOCHA;

    #[test]
    fn status_color_canonical_lifecycle_map() {
        // M172 S4 contract: the same status uses the same color
        // regardless of which lane renders it. The map below is
        // the canonical lifecycle→color mapping.
        assert_eq!(status_color("in-progress", &MOCHA), MOCHA.accent);
        assert_eq!(status_color("complete", &MOCHA), MOCHA.success);
        assert_eq!(status_color("blocked", &MOCHA), MOCHA.danger);
        assert_eq!(status_color("approved", &MOCHA), MOCHA.warn);
        assert_eq!(status_color("ready", &MOCHA), MOCHA.warn);
    }

    #[test]
    fn status_color_unknown_lifecycle_falls_back_to_dim() {
        assert_eq!(status_color("draft", &MOCHA), MOCHA.dim);
        assert_eq!(status_color("", &MOCHA), MOCHA.dim);
    }

    #[test]
    fn selection_pair_is_high_contrast() {
        let fg = selection_fg(&MOCHA);
        let bg = selection_bg(&MOCHA);
        // The fg must NOT equal the bg — that would be invisible.
        assert_ne!(fg, bg);
    }

    #[test]
    fn branch_marker_returns_glyph_and_dim_color() {
        let (color, glyph) = branch_marker(&MOCHA);
        assert_eq!(glyph, "└─");
        assert_eq!(color, MOCHA.dim);
    }
}
