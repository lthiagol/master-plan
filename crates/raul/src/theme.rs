//! Color theme palettes for raul's output (CLI + TUI).
//!
//! A [`Palette`] maps raul's semantic color roles to concrete colors. raul ships
//! with the Catppuccin palette (latte, frappe, macchiato, mocha), Dracula,
//! and Alucard (Dracula's light counterpart). `ui.theme` (see S2) selects
//! the active palette; renderers consume the semantic slots so a theme
//! switch recolors the whole surface.

use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A concrete set of colors for raul's semantic roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub name: &'static str,
    /// Headers, active items, in-progress (cyan-ish).
    pub accent: Color,
    /// Done / verified / passed (green-ish).
    pub success: Color,
    /// Ready / pending (yellow-ish).
    pub warn: Color,
    /// Blocked / failure (red-ish).
    pub danger: Color,
    /// Secondary text (gray-ish).
    pub dim: Color,
    /// Primary text.
    pub foreground: Color,
    /// Focused chip / selected-row border and marker.
    ///
    /// The "where is the cursor right now" color. Renderers use it
    /// for the selected row's border and its leading marker glyph,
    /// the focused tab, and the selected Path node. It is
    /// deliberately distinct from `accent` so focus does not read
    /// as a lifecycle color.
    pub focus_ring: Color,
    /// Overlay, modal, and picker background.
    ///
    /// The first layer above the terminal background. Overlays,
    /// modals, and the app's own chrome (header / footer) paint
    /// with this so a panel reads as floating above the lane
    /// content instead of blending into it.
    pub surface_1: Color,
    /// Selected-row / hovered background inside a surface.
    ///
    /// The second layer up: a selected or hovered row *within* a
    /// `surface_1` panel (a list row, a board box, a picker
    /// cursor). Paired with `foreground` as its text color.
    pub surface_2: Color,
}

impl Palette {
    /// Default theme when none is configured.
    pub const DEFAULT_NAME: &'static str = "mocha";

    pub fn by_name(name: &str) -> Option<&'static Palette> {
        ALL.iter().find(|p| p.name == name)
    }

    pub fn default_palette() -> &'static Palette {
        Self::by_name(Self::DEFAULT_NAME).expect("default theme must exist")
    }

    pub fn all() -> &'static [Palette] {
        &ALL
    }
}

pub static LATTE: Palette = Palette {
    name: "latte",
    accent: rgb(0x8839ef),
    success: rgb(0x40a02b),
    warn: rgb(0xdf8e1d),
    danger: rgb(0xd20f39),
    dim: rgb(0x6c6f85),
    foreground: rgb(0x4c4f69),
    focus_ring: rgb(0x7287fd),
    surface_1: rgb(0xe6e9ef),
    surface_2: rgb(0xccd0da),
};

pub static FRAPPE: Palette = Palette {
    name: "frappe",
    accent: rgb(0xca9ee6),
    success: rgb(0xa6d189),
    warn: rgb(0xe5c890),
    danger: rgb(0xe78284),
    // Frappé Subtext0 (`#a5adce`) — matches the secondary-text role
    // in the other three Catppuccin flavors (macchiato `0xa5adcb`,
    // mocha `0xa6adc8`). The old `0x949cbb` is Subtext1, a step
    // brighter than the role intended, which made Frappé's dim text
    // read louder than its siblings'.
    dim: rgb(0xa5adce),
    foreground: rgb(0xc6d0f5),
    focus_ring: rgb(0xbabbf1),
    surface_1: rgb(0x292c3c),
    surface_2: rgb(0x414559),
};

pub static MACCHIATO: Palette = Palette {
    name: "macchiato",
    accent: rgb(0xc6a0f6),
    success: rgb(0xa6da95),
    warn: rgb(0xeed49f),
    danger: rgb(0xed8796),
    dim: rgb(0xa5adcb),
    foreground: rgb(0xcad3f5),
    focus_ring: rgb(0xb7bdf8),
    surface_1: rgb(0x1e2030),
    surface_2: rgb(0x363a4f),
};

pub static MOCHA: Palette = Palette {
    name: "mocha",
    accent: rgb(0xcba6f7),
    success: rgb(0xa6e3a1),
    warn: rgb(0xf9e2af),
    danger: rgb(0xf38ba8),
    dim: rgb(0xa6adc8),
    foreground: rgb(0xcdd6f4),
    focus_ring: rgb(0xb4befe),
    surface_1: rgb(0x181825),
    surface_2: rgb(0x313244),
};

pub static DRACULA: Palette = Palette {
    name: "dracula",
    accent: rgb(0xbd93f9),
    success: rgb(0x50fa7b),
    warn: rgb(0xf1fa8c),
    danger: rgb(0xff5555),
    dim: rgb(0x6272a4),
    foreground: rgb(0xf8f8f2),
    focus_ring: rgb(0x8be9fd),
    surface_1: rgb(0x343746),
    surface_2: rgb(0x44475a),
};

/// Alucard Classic — Dracula's light counterpart.
///
/// Values are the official Alucard Classic theme, not taste:
/// accent `#644AC9` (Purple), success `#14710A` (Green),
/// warn `#846E15` (Yellow), danger `#CB3A2A` (Red), dim `#6C664B`
/// (Comment), foreground `#1F1F1F` (Foreground). The role mapping
/// mirrors [`DRACULA`]: raul's `accent` is the purple family,
/// `success` green, `warn` yellow, `danger` red, `dim` the comment
/// tone, `foreground` the light-theme text color.
pub static ALUCARD: Palette = Palette {
    name: "alucard",
    accent: rgb(0x644ac9),
    success: rgb(0x14710a),
    warn: rgb(0x846e15),
    danger: rgb(0xcb3a2a),
    dim: rgb(0x6c664b),
    foreground: rgb(0x1f1f1f),
    // Layering roles: official Alucard Cyan / Background Light /
    // Selection. Populating these is filling in fields on a new
    // palette — the roles themselves were added by M244.
    focus_ring: rgb(0x036a96),
    surface_1: rgb(0xdedccf),
    surface_2: rgb(0xcfcfde),
};

/// Neutral palette when `color_enabled()` is false — no theme accent RGB.
pub static MONOCHROME: Palette = Palette {
    name: "monochrome",
    accent: Color::Reset,
    success: Color::Reset,
    warn: Color::Reset,
    danger: Color::Reset,
    dim: Color::DarkGray,
    foreground: Color::Reset,
    // The layering roles collapse to Reset on purpose: with color
    // disabled the *only* remaining layer signal is the modifier
    // pair (BOLD for the selected row, REVERSED for an overlay
    // cursor). `Palette::all()` excludes this palette, so the
    // named-palette distinctness checks skip it too.
    focus_ring: Color::Reset,
    surface_1: Color::Reset,
    surface_2: Color::Reset,
};

pub static ALL: [Palette; 6] = [LATTE, FRAPPE, MACCHIATO, MOCHA, DRACULA, ALUCARD];

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the exact per-palette values of the three layering roles.
    /// These are official upstream theme colors, not taste:
    ///
    /// | palette    | focus_ring | surface_1 | surface_2 | source                        |
    /// |------------|------------|-----------|-----------|-------------------------------|
    /// | latte      | `0x7287fd` | `0xe6e9ef` | `0xccd0da` | Catppuccin Lavender/Base/Subtle0 |
    /// | frappe     | `0xbabbf1` | `0x292c3c` | `0x414559` | Catppuccin Lavender/Mantle/Surface0 |
    /// | macchiato  | `0xb7bdf8` | `0x1e2030` | `0x363a4f` | Catppuccin Lavender/Mantle/Surface0 |
    /// | mocha      | `0xb4befe` | `0x181825` | `0x313244` | Catppuccin Lavender/Mantle/Surface0 |
    /// | dracula    | `0x8be9fd` | `0x343746` | `0x44475a` | draculatheme.com/spec Cyan/Background Light/Selection |
    /// | alucard    | `0x036a96` | `0xdedccf` | `0xcfcfde` | Alucard Classic Cyan/Background Light/Selection |
    ///
    /// `monochrome` pins the `Color::Reset` collapse.
    #[test]
    fn palette_roles_match_spec() {
        let cases: &[(&'static Palette, Color, Color, Color)] = &[
            (&LATTE, rgb(0x7287fd), rgb(0xe6e9ef), rgb(0xccd0da)),
            (&FRAPPE, rgb(0xbabbf1), rgb(0x292c3c), rgb(0x414559)),
            (&MACCHIATO, rgb(0xb7bdf8), rgb(0x1e2030), rgb(0x363a4f)),
            (&MOCHA, rgb(0xb4befe), rgb(0x181825), rgb(0x313244)),
            (&DRACULA, rgb(0x8be9fd), rgb(0x343746), rgb(0x44475a)),
            (&ALUCARD, rgb(0x036a96), rgb(0xdedccf), rgb(0xcfcfde)),
        ];
        for (p, ring, s1, s2) in cases {
            assert_eq!(p.focus_ring, *ring, "{} focus_ring", p.name);
            assert_eq!(p.surface_1, *s1, "{} surface_1", p.name);
            assert_eq!(p.surface_2, *s2, "{} surface_2", p.name);
        }
    }

    /// Frappé's secondary text is Catppuccin Subtext0 (`#a5adce`).
    /// It was previously Subtext1 (`#949cbb`), which read a step
    /// brighter than the role and out-shouted macchiato / mocha.
    #[test]
    fn frappe_dim_matches_spec() {
        assert_eq!(FRAPPE.dim, rgb(0xa5adce), "frappe dim is Subtext0");
    }

    /// Alucard Classic ships as a first-class named palette. Pin the
    /// exact hexes so a future "taste tweak" to the light theme has
    /// to be a deliberate edit, not a drift.
    ///
    /// accent `#644AC9` / success `#14710A` / warn `#846E15` /
    /// danger `#CB3A2A` / dim `#6C664B` / foreground `#1F1F1F`.
    #[test]
    fn alucard_matches_spec() {
        assert_eq!(ALUCARD.name, "alucard");
        assert_eq!(ALUCARD.accent, rgb(0x644ac9), "alucard accent #644AC9");
        assert_eq!(ALUCARD.success, rgb(0x14710a), "alucard success #14710A");
        assert_eq!(ALUCARD.warn, rgb(0x846e15), "alucard warn #846E15");
        assert_eq!(ALUCARD.danger, rgb(0xcb3a2a), "alucard danger #CB3A2A");
        assert_eq!(ALUCARD.dim, rgb(0x6c664b), "alucard dim #6C664B");
        assert_eq!(
            ALUCARD.foreground,
            rgb(0x1f1f1f),
            "alucard foreground #1F1F1F"
        );
    }

    /// `alucard` must be reachable by name and listed in `ALL` —
    /// otherwise `ui.theme = "alucard"` validates in mp but
    /// silently falls back at runtime via `by_name` returning `None`.
    #[test]
    fn palette_by_name_alucard() {
        let found = Palette::by_name("alucard").expect("alucard must resolve by name");
        assert_eq!(found.name, "alucard");
        assert_eq!(found.accent, ALUCARD.accent);
        assert!(
            ALL.iter().any(|p| p.name == "alucard"),
            "alucard must be listed in ALL"
        );
        assert_eq!(ALL.len(), 6, "ALL carries the 6 named palettes");
    }

    /// The no-color palette collapses every layering role to `Reset`;
    /// layering is carried by modifiers instead of hue.
    #[test]
    fn monochrome_roles_collapse_to_reset() {
        assert_eq!(MONOCHROME.focus_ring, Color::Reset);
        assert_eq!(MONOCHROME.surface_1, Color::Reset);
        assert_eq!(MONOCHROME.surface_2, Color::Reset);
    }

    /// The layering roles are only useful if they actually separate
    /// the layers they name. A palette that reuses one color for
    /// `surface_1`, `surface_2`, and `foreground` would collapse
    /// every panel back onto the text color — the exact problem the
    /// layering roles were added to fix.
    ///
    /// Scoped to `ALL` (the named, user-selectable palettes).
    /// `MONOCHROME` is deliberately excluded: with color disabled it
    /// sets every role to `Reset` and leans on modifiers instead.
    #[test]
    fn palette_roles_non_degenerate() {
        for p in ALL.iter() {
            assert_ne!(
                p.surface_1, p.surface_2,
                "{}: surface_1 and surface_2 must differ (surface_2 is a layer above surface_1)",
                p.name
            );
            assert_ne!(
                p.surface_2, p.foreground,
                "{}: surface_2 must differ from foreground (a selected row needs readable text)",
                p.name
            );
            assert_ne!(
                p.surface_1, p.foreground,
                "{}: surface_1 must differ from foreground (an overlay needs readable text)",
                p.name
            );
            assert_ne!(
                p.focus_ring, p.accent,
                "{}: focus_ring must differ from accent (focus is not a lifecycle color)",
                p.name
            );
        }
    }
}
