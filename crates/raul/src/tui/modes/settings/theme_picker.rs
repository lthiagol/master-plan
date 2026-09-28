//! Theme picker for the Settings lane's `ui.theme` row.
//!
//! Pure model — no rendering, no subprocess, no `App` access. The
//! renderer (`render::overlays::render_settings_list`) draws what this
//! exposes; `App::move_up` / `move_down` move the cursor; `s` saves.
//!
//! ## Row layout
//!
//! The expansion is **7 rows**:
//!
//! ```text
//!   ▾ latte      ██████  Catppuccin Latte — light
//!     frappe     ██████  Catppuccin Frappé — dim and soft
//!     macchiato  ██████  Catppuccin Macchiato — mid-dark
//!     mocha      ██████  Catppuccin Mocha — deep dark (default)
//!     dracula    ██████  Dracula — classic dark
//!     alucard    ██████  Alucard Classic — Dracula's light counterpart
//!     Default (mocha)    Reset to the default palette
//! ```
//!
//! One row per entry in [`crate::theme::ALL`] (alphabetical, so the
//! list reads in one direction), then a `Default` reset row.
//! `monochrome` is deliberately absent — it is the `ui.color=false`
//! no-color fallback, not a selectable `ui.theme` value.
//!
//! ## Swatch: 6 roles
//!
//! The swatch draws exactly six roles — `accent`, `success`, `warn`,
//! `danger`, `dim`, `foreground` — each in the palette it previews, so
//! the row is a self-portrait of what the rest of the TUI would look
//! like. It is deliberately **6, not 9**: the layering roles
//! (`focus_ring`, `surface_1`, `surface_2`) are background layers
//! rather than text colors and would read as mud in a one-line strip.
//! [`SWATCH_ROLES`] is the single place that decision is pinned.
//!
//! ## Save / cancel
//!
//! Highlighting is a *preview*: it sets `App::palette` immediately so
//! the next frame repaints in the highlighted palette. Nothing is
//! written until `s`, which routes through the existing
//! `Action::SettingsSave` path (`mp config set ui.theme`). Esc drops
//! the preview and restores the saved palette.

use ratatui::style::Color;

use crate::theme::{self, Palette};

/// The config key this picker owns.
pub const THEME_KEY: &str = "ui.theme";

/// Number of picker rows: one per named palette plus the reset row.
pub const ROW_COUNT: usize = 7;

/// The reset row's index — it always sorts after the palettes.
pub const RESET_ROW: usize = theme::ALL.len();

/// The six roles a swatch draws, in render order.
///
/// Pinned here (not in the renderer) so the count is a testable
/// contract rather than a `for` loop's accident. See the module docs
/// for why it is six and not nine.
pub const SWATCH_ROLES: [&str; 6] = ["accent", "success", "warn", "danger", "dim", "foreground"];

/// One-line description shown next to each palette's swatch.
pub const fn describe(name: &str) -> &'static str {
    match name.as_bytes() {
        b"latte" => "Catppuccin Latte — light",
        b"frappe" => "Catppuccin Frappé — dim and soft",
        b"macchiato" => "Catppuccin Macchiato — mid-dark",
        b"mocha" => "Catppuccin Mocha — deep dark (default)",
        b"dracula" => "Dracula — classic dark",
        b"alucard" => "Alucard Classic — Dracula's light counterpart",
        _ => "Unknown palette",
    }
}

/// The reset row's label, e.g. `Default (mocha)`.
pub fn reset_label() -> String {
    format!("Default ({})", Palette::DEFAULT_NAME)
}

/// The reset row's description.
pub const RESET_DESCRIPTION: &str = "Reset to the default palette";

/// What a picker row selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A named palette at this index into [`crate::theme::ALL`].
    Named(usize),
    /// The `Default (<mocha>)` reset row.
    Reset,
}

impl Row {
    /// The theme name this row writes when saved.
    ///
    /// The reset row writes [`Palette::DEFAULT_NAME`] — the reset is
    /// a concrete value, not a "clear this key" request, so `s` on
    /// the reset row lands `ui.theme=mocha` on disk.
    pub fn theme_name(self) -> &'static str {
        match self {
            Row::Named(i) => theme::ALL[i].name,
            Row::Reset => Palette::DEFAULT_NAME,
        }
    }

    /// The palette this row previews.
    pub fn palette(self) -> &'static Palette {
        match self {
            Row::Named(i) => &theme::ALL[i],
            Row::Reset => Palette::default_palette(),
        }
    }

    /// The row's label — the bare palette name, or `Default (mocha)`.
    pub fn label(self) -> String {
        match self {
            Row::Named(i) => theme::ALL[i].name.to_string(),
            Row::Reset => reset_label(),
        }
    }

    /// The row's one-line description.
    pub fn description(self) -> &'static str {
        match self {
            Row::Named(i) => describe(theme::ALL[i].name),
            Row::Reset => RESET_DESCRIPTION,
        }
    }
}

/// The 7 rows, in render order.
pub fn rows() -> Vec<Row> {
    (0..theme::ALL.len())
        .map(Row::Named)
        .chain(std::iter::once(Row::Reset))
        .collect()
}

/// The swatch colors for a palette, in [`SWATCH_ROLES`] order.
pub fn swatch(palette: &Palette) -> [Color; 6] {
    [
        palette.accent,
        palette.success,
        palette.warn,
        palette.danger,
        palette.dim,
        palette.foreground,
    ]
}

/// The status-preview chips: `(label, role)`. Rendered by the caller
/// with the *highlighted* palette so the operator sees the lifecycle
/// colors the TUI would use under the theme they are considering.
pub const STATUS_PREVIEW: [(&str, &str); 6] = [
    ("in-progress", "accent"),
    ("done", "success"),
    ("ready", "warn"),
    ("blocked", "danger"),
    ("accent", "accent"),
    ("dim", "dim"),
];

/// The picker's mutable state. Lives on
/// [`SettingsState`](crate::tui::mode::SettingsState).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePicker {
    /// Highlighted row index into [`rows()`], always `0..ROW_COUNT`.
    cursor: usize,
    /// `true` while the `ui.theme` row's expansion is visible.
    expanded: bool,
    /// The `ui.theme` value currently on disk (or the last saved one).
    /// Esc restores this.
    saved: String,
}

impl ThemePicker {
    /// Build a picker whose saved value is `saved` (an empty or
    /// unknown name falls back to the default palette). The cursor
    /// starts on the row matching the saved theme.
    pub fn new(saved: impl Into<String>) -> Self {
        let saved = saved.into();
        let cursor = row_for_name(&saved).unwrap_or(0);
        Self {
            cursor,
            expanded: false,
            saved,
        }
    }

    /// Is the expansion currently visible?
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Show the expansion with the cursor on the saved theme. Returns
    /// `true` when this changed anything.
    pub fn expand(&mut self) -> bool {
        if self.expanded {
            return false;
        }
        self.cursor = row_for_name(&self.saved).unwrap_or(0);
        self.expanded = true;
        true
    }

    /// Hide the expansion, leaving the cursor and saved value alone
    /// so re-expanding lands back where the operator left off.
    pub fn collapse(&mut self) -> bool {
        if !self.expanded {
            return false;
        }
        self.expanded = false;
        true
    }

    /// The highlighted row index into [`rows()`].
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The highlighted row.
    pub fn current(&self) -> Row {
        row_at(self.cursor)
    }

    /// Move the highlight by `delta` rows, clamped to the ends.
    /// Returns `true` when the cursor actually moved.
    pub fn move_cursor(&mut self, delta: i32) -> bool {
        let next = (self.cursor as i32 + delta).clamp(0, ROW_COUNT as i32 - 1) as usize;
        let moved = next != self.cursor;
        self.cursor = next;
        moved
    }

    /// Park the highlight on `idx`. Used by the mouse hit-test.
    /// Returns `true` when the cursor actually moved.
    pub fn set_cursor(&mut self, idx: usize) -> bool {
        if idx >= ROW_COUNT {
            return false;
        }
        let moved = idx != self.cursor;
        self.cursor = idx;
        moved
    }

    /// The palette the operator is currently looking at. This is what
    /// `App::palette` is set to on every highlight change.
    pub fn preview_palette(&self) -> &'static Palette {
        self.current().palette()
    }

    /// The theme name the operator is currently looking at.
    pub fn preview_name(&self) -> &'static str {
        self.current().theme_name()
    }

    /// The `ui.theme` value on disk.
    pub fn saved_theme(&self) -> &str {
        &self.saved
    }

    /// Is the highlight sitting on something other than the saved
    /// value? Drives the header chip's `preview: … (saved: …)` form.
    pub fn is_previewing(&self) -> bool {
        self.preview_name() != self.saved
    }

    /// Record `name` as saved and re-seat the cursor on it. Called
    /// after `Action::SettingsSave` commits the staged edit.
    pub fn mark_saved(&mut self, name: impl Into<String>) {
        self.saved = name.into();
        if let Some(idx) = row_for_name(&self.saved) {
            self.cursor = idx;
        }
    }

    /// The Settings block-title chip.
    ///
    /// `saved: <name>` when the highlight matches what is on disk;
    /// `preview: <name> (saved: <name>)` while the operator is looking
    /// at something else. The two-name form is the whole point — a
    /// live preview that silently differs from disk would otherwise
    /// look exactly like a saved change.
    pub fn state_label(&self) -> String {
        if self.is_previewing() {
            format!("preview: {} (saved: {})", self.preview_name(), self.saved)
        } else {
            format!("saved: {}", self.saved)
        }
    }

    /// Esc: drop the preview and put the highlight back on the saved
    /// theme. The caller restores `App::palette` from
    /// [`Self::saved_theme`].
    pub fn cancel(&mut self) {
        self.cursor = row_for_name(&self.saved).unwrap_or(0);
    }
}

impl Default for ThemePicker {
    fn default() -> Self {
        Self::new(Palette::DEFAULT_NAME)
    }
}

/// The row at `idx`, or the reset row for anything out of range —
/// callers hand us clamped indices, and a `None` here would panic a
/// render path for no good reason.
pub fn row_at(idx: usize) -> Row {
    if idx < theme::ALL.len() {
        Row::Named(idx)
    } else {
        Row::Reset
    }
}

/// The row that writes `name`, or `None` when `name` is not a
/// catalogued palette. Both named rows and the reset row resolve —
/// the reset row matches the default palette's name.
pub fn row_for_name(name: &str) -> Option<usize> {
    theme::ALL
        .iter()
        .position(|p| p.name == name)
        .or_else(|| (name == Palette::DEFAULT_NAME).then_some(RESET_ROW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_six_palettes_then_reset() {
        let rows = rows();
        assert_eq!(rows.len(), 7, "6 named palettes + 1 reset row");
        assert_eq!(rows.len(), ROW_COUNT);
        assert_eq!(rows[5], Row::Named(5));
        assert_eq!(rows[6], Row::Reset);
        // No `monochrome` anywhere in the picker.
        assert!(!rows
            .iter()
            .any(|r| r.theme_name() == theme::MONOCHROME.name));
    }

    #[test]
    fn reset_row_writes_the_default_theme() {
        let reset = Row::Reset;
        assert_eq!(reset.theme_name(), "mocha");
        assert_eq!(reset.label(), "Default (mocha)");
        // Compare by value, not by pointer: `Palette::by_name` hands
        // back a reference into the `ALL` array, not the `MOCHA`
        // static, so `std::ptr::eq` would be false for two identical
        // palettes.
        assert_eq!(reset.palette().name, theme::MOCHA.name);
        assert_eq!(reset.palette().accent, theme::MOCHA.accent);
    }

    #[test]
    fn swatch_is_six_roles_in_spec_order() {
        assert_eq!(SWATCH_ROLES.len(), 6);
        assert_eq!(
            SWATCH_ROLES,
            ["accent", "success", "warn", "danger", "dim", "foreground"]
        );
        let p = &theme::DRACULA;
        assert_eq!(
            swatch(p),
            [p.accent, p.success, p.warn, p.danger, p.dim, p.foreground]
        );
    }

    #[test]
    fn cursor_clamps_at_both_ends() {
        let mut t = ThemePicker::new("mocha");
        let start = t.cursor();
        assert_eq!(start, row_for_name("mocha").unwrap());
        assert!(t.move_cursor(-1), "mocha is row 3, so Up moves");

        // Park at the top, then prove Up is a no-op there.
        for _ in 0..10 {
            t.move_cursor(-1);
        }
        assert_eq!(t.cursor(), 0, "clamps at the first row");
        assert!(!t.move_cursor(-1), "cannot move above the first row");

        for _ in 0..20 {
            t.move_cursor(1);
        }
        assert_eq!(t.cursor(), ROW_COUNT - 1, "clamps at the reset row");
        assert!(!t.move_cursor(1), "cannot move below the reset row");
    }

    #[test]
    fn expand_collapse_roundtrip() {
        let mut t = ThemePicker::new("dracula");
        assert!(!t.is_expanded());
        assert!(t.expand());
        assert!(t.is_expanded());
        assert!(!t.expand(), "expanding twice is a no-op");
        assert!(t.collapse());
        assert!(!t.is_expanded());
    }

    #[test]
    fn cancel_returns_to_the_saved_theme() {
        let mut t = ThemePicker::new("mocha");
        t.expand();
        t.set_cursor(row_for_name("alucard").unwrap());
        assert!(t.is_previewing());
        t.cancel();
        assert_eq!(t.preview_name(), "mocha");
        assert!(!t.is_previewing());
    }

    #[test]
    fn mark_saved_makes_the_preview_clean() {
        let mut t = ThemePicker::new("mocha");
        t.expand();
        t.set_cursor(row_for_name("frappe").unwrap());
        assert!(t.is_previewing());
        t.mark_saved("frappe");
        assert_eq!(t.saved_theme(), "frappe");
        assert!(!t.is_previewing());
        assert_eq!(t.cursor(), row_for_name("frappe").unwrap());
    }

    #[test]
    fn every_catalog_name_resolves_to_a_row() {
        for name in mp_model::UI_THEMES {
            assert!(
                row_for_name(name).is_some(),
                "{name} must resolve to a picker row"
            );
        }
        assert_eq!(row_for_name("nope"), None);
    }

    #[test]
    fn state_label_reads_saved_then_preview() {
        let mut t = ThemePicker::new("mocha");
        assert_eq!(t.state_label(), "saved: mocha");
        t.expand();
        t.set_cursor(row_for_name("alucard").unwrap());
        assert_eq!(
            t.state_label(),
            "preview: alucard (saved: mocha)",
            "a preview must name BOTH the preview and what is on disk"
        );
        t.mark_saved("alucard");
        assert_eq!(t.state_label(), "saved: alucard");
    }

    #[test]
    fn descriptions_are_unique_and_non_empty() {
        let seen: Vec<&str> = rows().iter().map(|r| r.description()).collect();
        for (i, d) in seen.iter().enumerate() {
            assert!(!d.is_empty(), "row {i} has an empty description");
            assert_ne!(*d, "Unknown palette", "row {i} has no description");
        }
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "descriptions must be distinct");
    }
}
