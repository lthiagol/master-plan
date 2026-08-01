//! Color theme palettes for raul's output (CLI + TUI).
//!
//! A [`Palette`] maps raul's semantic color roles to concrete colors. raul ships
//! with the Catppuccin palette (latte, frappe, macchiato, mocha) and Dracula.
//! `ui.theme` (see S2) selects the active palette; renderers consume the
//! semantic slots so a theme switch recolors the whole surface.

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
};

pub static FRAPPE: Palette = Palette {
    name: "frappe",
    accent: rgb(0xca9ee6),
    success: rgb(0xa6d189),
    warn: rgb(0xe5c890),
    danger: rgb(0xe78284),
    dim: rgb(0x949cbb),
    foreground: rgb(0xc6d0f5),
};

pub static MACCHIATO: Palette = Palette {
    name: "macchiato",
    accent: rgb(0xc6a0f6),
    success: rgb(0xa6da95),
    warn: rgb(0xeed49f),
    danger: rgb(0xed8796),
    dim: rgb(0xa5adcb),
    foreground: rgb(0xcad3f5),
};

pub static MOCHA: Palette = Palette {
    name: "mocha",
    accent: rgb(0xcba6f7),
    success: rgb(0xa6e3a1),
    warn: rgb(0xf9e2af),
    danger: rgb(0xf38ba8),
    dim: rgb(0xa6adc8),
    foreground: rgb(0xcdd6f4),
};

pub static DRACULA: Palette = Palette {
    name: "dracula",
    accent: rgb(0xbd93f9),
    success: rgb(0x50fa7b),
    warn: rgb(0xf1fa8c),
    danger: rgb(0xff5555),
    dim: rgb(0x6272a4),
    foreground: rgb(0xf8f8f2),
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
};

pub static ALL: [Palette; 5] = [LATTE, FRAPPE, MACCHIATO, MOCHA, DRACULA];
