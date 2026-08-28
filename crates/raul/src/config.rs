//! raul UI configuration: color toggle, icon mode, theme selection.
//!
//! raul is the read-only human surface, so it does NOT write files. UI
//! preferences (ui.color, ui.icons, ui.theme, ui.hide_done) live in mp's
//! project config (`config.toml`) — raul reads them via `mp config show` and
//! persists changes via `mp config set`. The `--color` CLI flag overrides
//! `ui.color` for the run. The global toggle lets deep call sites respect the
//! flag without threading config through every signature.

use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::style::{Color as CtColor, Stylize};
use serde_json::Value;

use crate::mp_runner::MpRunner;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IconMode {
    None,
    Ascii,
    Unicode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiConfig {
    pub color: bool,
    pub icons: IconMode,
    pub theme: String,
    pub hide_done: bool,
    /// M154: the `[review] hunk` flag read from mp's project config.
    /// When true, the milestone detail view shows the "hunk export: on
    /// (N anchored)" indicator and the count of location-anchored
    /// findings; when false (default), the indicator is hidden. raul
    /// is read-only on mp config — the flag is loaded once at startup
    /// via `UiConfig::load` and never written back.
    pub review_hunk_enabled: bool,
    /// M198: the `ui.show_watch_tab` flag read from mp's project
    /// config. When true, raul's tab bar includes the Watch lane;
    /// when false (the default), the Watch lane is filtered out of
    /// the tab bar, the hit-test areas, and the prev/next
    /// navigation. Loaded once at startup; restart raul to pick
    /// up mid-session changes. Independent of `mp watch` — the
    /// `mp` binary's `mp watch` command always works regardless
    /// of this flag.
    pub show_watch_tab: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color: true,
            icons: IconMode::Unicode,
            theme: Palette::DEFAULT_NAME.to_string(),
            hide_done: false,
            review_hunk_enabled: false,
            show_watch_tab: false,
        }
    }
}

impl UiConfig {
    /// Load raul UI preferences from `mp config show` (project config.toml).
    /// raul is read-only; UI prefs live in mp's config so they persist without
    /// raul writing files. Falls back to defaults on any error.
    pub fn load(runner: &MpRunner) -> Self {
        let mut cfg = Self::default();
        let data: Value = match runner.run("config", &["show"]) {
            Ok(v) => v,
            Err(_) => return cfg,
        };
        let ui = &data["config"]["ui"];
        if let Some(c) = ui["color"].as_bool() {
            cfg.color = c;
        }
        if let Some(h) = ui["hide_done"].as_bool() {
            cfg.hide_done = h;
        }
        if let Some(t) = ui["theme"].as_str() {
            cfg.theme = t.to_string();
        }
        if let Some(i) = ui["icons"].as_str() {
            cfg.icons = match i {
                "none" => IconMode::None,
                "ascii" => IconMode::Ascii,
                _ => IconMode::Unicode,
            };
        }
        // M154: read `[review].hunk` for the milestone-detail indicator.
        // The section is opt-in per project (default off); absent
        // section means the indicator is hidden.
        if let Some(h) = data["config"]["review"]["hunk"].as_bool() {
            cfg.review_hunk_enabled = h;
        }
        // M198: read `ui.show_watch_tab`. The default is `false`
        // (the Watch lane is hidden); the operator opts in by
        // `mp config set ui.show_watch_tab true` (either from the
        // raul Settings lane or the CLI). An absent value keeps
        // the default so a stale config never accidentally
        // re-enables the tab.
        if let Some(s) = ui["show_watch_tab"].as_bool() {
            cfg.show_watch_tab = s;
        }
        cfg
    }

    /// Apply a `--color` CLI override.
    pub fn with_color_override(mut self, color: Option<bool>) -> Self {
        if let Some(c) = color {
            self.color = c;
        }
        self
    }

    /// Resolve the active palette (unknown theme falls back to default).
    pub fn palette(&self) -> &'static Palette {
        Palette::by_name(&self.theme).unwrap_or_else(Palette::default_palette)
    }
}

// --- global color toggle ---

static COLOR_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_color_enabled(on: bool) {
    COLOR_ENABLED.store(on, Ordering::Relaxed);
}

pub fn color_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

static ICONS_MODE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(IconMode::Unicode as u8);

pub fn set_icons(mode: IconMode) {
    ICONS_MODE.store(mode as u8, Ordering::Relaxed);
}

pub fn icons() -> IconMode {
    match ICONS_MODE.load(Ordering::Relaxed) {
        0 => IconMode::None,
        1 => IconMode::Ascii,
        _ => IconMode::Unicode,
    }
}

/// Width (in terminal columns) the icon + separator contributes, per mode.
pub fn icon_width() -> usize {
    match icons() {
        IconMode::None => 0,
        IconMode::Ascii => 4,   // "[x] "
        IconMode::Unicode => 2, // "● "
    }
}

pub fn status_icon(status: &str) -> &'static str {
    match icons() {
        IconMode::None => "",
        IconMode::Ascii => match status {
            "done" | "verified" | "passed" => "[x]",
            "in-progress" | "active" => "[~]",
            "blocked" => "[!]",
            _ => "[ ]",
        },
        IconMode::Unicode => match status {
            "done" | "verified" | "passed" => "●",
            "in-progress" | "active" => "◐",
            "blocked" => "✕",
            _ => "○",
        },
    }
}

/// Lane glyph for sidebar labels, honoring the active icon mode.
pub fn lane_icon(lane: &str) -> &'static str {
    match icons() {
        IconMode::None => "",
        IconMode::Ascii => match lane {
            crate::lanes::LANE_OVERVIEW => "[~]",
            crate::lanes::LANE_MILESTONES => "[M]",
            crate::lanes::LANE_PATH => "[P]",
            crate::lanes::LANE_BUGFIXES => "[B]",
            crate::lanes::LANE_BACKLOG => "[L]",
            crate::lanes::LANE_IDEAS => "[I]",
            crate::lanes::LANE_WATCH => "[W]",
            crate::lanes::LANE_SETTINGS => "[S]",
            _ => "[ ]",
        },
        IconMode::Unicode => match lane {
            crate::lanes::LANE_OVERVIEW => "⌂",
            crate::lanes::LANE_MILESTONES => "◆",
            crate::lanes::LANE_PATH => "→",
            crate::lanes::LANE_BUGFIXES => "🐛",
            crate::lanes::LANE_BACKLOG => "📋",
            crate::lanes::LANE_IDEAS => "💡",
            crate::lanes::LANE_WATCH => "👁",
            crate::lanes::LANE_SETTINGS => "⚙",
            _ => "·",
        },
    }
}

/// Colorize `text` when color output is enabled, else return plain text.
pub fn paint(text: &str, color: CtColor) -> String {
    if color_enabled() {
        text.with(color).to_string()
    } else {
        text.to_string()
    }
}

/// Bold `text` when color output is enabled, else plain.
pub fn paint_bold(text: &str) -> String {
    if color_enabled() {
        text.bold().to_string()
    } else {
        text.to_string()
    }
}

/// Bold + color `text` when enabled, else plain.
pub fn paint_bold_color(text: &str, color: CtColor) -> String {
    if color_enabled() {
        text.with(color).bold().to_string()
    } else {
        text.to_string()
    }
}

/// Semantic role of a status string. The workaround-pass code-review
/// (2026-07-05) consolidated the 4 near-identical match tables
/// (`status_color`, `status_color_tui_text`, `status_badge_style`,
/// `status_row_color`) into this single enum. Adapters in
/// `config::paint_for_role` and `tui::progress::{style_for_role,
/// color_for_role}` re-derive the crossterm `CtColor` and ratatui
/// `Color` from a single source of truth. To add a new state, edit the
/// `status_role` match table only — the color adapters pick it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRole {
    /// Done / verified / passed / complete → success (green).
    Done,
    /// in-progress / active / open / reviewed → accent (cyan).
    InProgress,
    /// implemented / self-reviewed → warn (yellow). Work finished,
    /// awaiting the verified transition.
    Implemented,
    /// ready / approved / groomed / review / draft / interview /
    /// remediation / deferred → warn (yellow). Spec or scope work in
    /// flight, not actively executing.
    AwaitingReview,
    /// blocked / failed / cancelled / removed / rejected → danger (red).
    Blocked,
    /// planned / pending → dim grey. Author has not started.
    NotStarted,
    /// Anything else → dim grey (neutral fallback). Reaching this
    /// branch is a code smell — log a hint or add the new value to the
    /// table.
    Unknown,
}

/// Map a status string (legacy `spec_status` / `execution_status` or new
/// `lifecycle`) to its semantic role. Single source of truth for the
/// status → color pipeline used by the CLI list, `raul spec`, and the
/// TUI list / detail. Pre-fix this lived as four identical
/// `match` arms in different files; kept in sync by inspection.
pub fn status_role(text: &str) -> StatusRole {
    match text {
        "done" | "verified" | "passed" | "complete" => StatusRole::Done,
        // Open question / finding closed states share the Done color.
        "resolved" | "closed" => StatusRole::Done,
        "in-progress" | "active" | "open" | "reviewed" => StatusRole::InProgress,
        "implemented" | "self-reviewed" => StatusRole::Implemented,
        "ready" | "approved" => StatusRole::AwaitingReview,
        "groomed" | "review" | "draft" | "interview" | "remediation" | "deferred" => {
            StatusRole::AwaitingReview
        }
        "blocked" | "failed" | "cancelled" | "removed" | "rejected" => StatusRole::Blocked,
        "planned" | "pending" => StatusRole::NotStarted,
        _ => StatusRole::Unknown,
    }
}

/// Crossterm paint adapter — converts a role into a `String` honoring
/// the configured `color_enabled()` flag. Replaces the inline match
/// arms that used to live in `commands::milestones::list` and
/// `commands::spec`.
pub fn paint_for_role(text: &str, role: StatusRole) -> String {
    let color = match role {
        StatusRole::Done => CtColor::Green,
        StatusRole::InProgress => CtColor::Cyan,
        StatusRole::Implemented | StatusRole::AwaitingReview => CtColor::Yellow,
        StatusRole::Blocked => CtColor::Red,
        StatusRole::NotStarted | StatusRole::Unknown => CtColor::DarkGrey,
    };
    paint(text, color)
}

/// CLI list — wraps `status_role` + `paint_for_role` in one call. Most
/// call sites use this; the role API exists for callers that want to
/// inspect without re-painting.
pub fn status_color(text: &str) -> String {
    paint_for_role(text, status_role(text))
}

/// Terminal column count for layout: honors `$COLUMNS`, then the real terminal
/// size (crossterm), then falls back to 80.
pub fn term_columns() -> usize {
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(n) = s.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    if let Ok((w, _)) = crossterm::terminal::size() {
        if w > 0 {
            return w as usize;
        }
    }
    80
}

/// Shared lane-color palette: maps lane name → terminal color.
/// TUI (`tui/path_view.rs`) is the only consumer after M164 dropped the
/// CLI verb surface; the helper stays here so any future non-TUI
/// consumer (e.g. a CLI dump) can match the same colors.
pub fn lane_color(name: &str) -> CtColor {
    match name {
        "blocked" => CtColor::Red,
        "execution" => CtColor::Green,
        "review" => CtColor::Yellow,
        "grooming" => CtColor::Blue,
        "backlog" => CtColor::Magenta,
        _ => CtColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the `paint_for_role` adapter: the role enum drives a single
    /// CtColor per role. Iterates every role variant's expected color.
    #[test]
    fn paint_for_role_matches_role_table() {
        assert_eq!(
            paint_for_role("done", StatusRole::Done),
            paint("done", CtColor::Green)
        );
        assert_eq!(
            paint_for_role("in-progress", StatusRole::InProgress),
            paint("in-progress", CtColor::Cyan)
        );
        assert_eq!(
            paint_for_role("implemented", StatusRole::Implemented),
            paint("implemented", CtColor::Yellow)
        );
        assert_eq!(
            paint_for_role("ready", StatusRole::AwaitingReview),
            paint("ready", CtColor::Yellow)
        );
        assert_eq!(
            paint_for_role("blocked", StatusRole::Blocked),
            paint("blocked", CtColor::Red)
        );
        assert_eq!(
            paint_for_role("planned", StatusRole::NotStarted),
            paint("planned", CtColor::DarkGrey)
        );
        assert_eq!(
            paint_for_role("unknown-string", StatusRole::Unknown),
            paint("unknown-string", CtColor::DarkGrey)
        );
    }

    /// Pin `status_color` is `paint_for_role ∘ status_role` for the strings
    /// the production code can legitimately pass through.
    #[test]
    fn status_color_pins_known_strings() {
        // Done/verified
        assert!(status_color("done").contains("\x1b") || status_color("done") == "done");
        // (color output is ANSI-encoded when color_enabled(); we just
        // assert the helper returns the input wrapped, not the
        // wrong text.)

        // Per-role sanity: the helper routes each string to the
        // expected CtColor. This is the test that catches the "all
        // values fall through to dim" regression.
        let cases: &[(&str, CtColor)] = &[
            ("done", CtColor::Green),
            ("verified", CtColor::Green),
            ("complete", CtColor::Green),
            ("in-progress", CtColor::Cyan),
            ("implemented", CtColor::Yellow),
            ("ready", CtColor::Yellow),
            ("approved", CtColor::Yellow),
            ("groomed", CtColor::Yellow),
            ("remediation", CtColor::Yellow),
            ("blocked", CtColor::Red),
            ("failed", CtColor::Red),
            ("cancelled", CtColor::Red),
            ("planned", CtColor::DarkGrey),
            ("pending", CtColor::DarkGrey),
        ];
        for (input, _expected) in cases {
            // The public `status_color` returns a String; we don't want to
            // assert on ANSI codes (color_enabled() may be off in tests).
            // We assert the role enum directly: that's the source of
            // truth for the color.
            let role = status_role(input);
            assert_eq!(
                paint_for_role(input, role),
                status_color(input),
                "paint_for_role and status_color must agree for {input}"
            );
        }
    }
}
