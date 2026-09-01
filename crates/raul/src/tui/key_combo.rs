//! M138: a human-friendly key-combo parser + formatter.
//!
//! ## Why this module exists
//!
//! Pre-M138 every binding was a hand-written `match` arm in `modes/normal.rs`
//! (`KeyCode::Char('q') | KeyCode::Char('Q') if key.modifiers.is_empty()`),
//! duplicated across the help overlay and footer text. A user who wanted to
//! rebind a key had to fork raul. This module is the first half of the fix:
//! it turns a human string like `"ctrl+shift+up"` or `"minus"` into a typed
//! [`KeyCombo`] that [`super::keybinds`] stores per action.
//!
//! The grammar is lifted from herdr's `src/config/keybinds.rs` (the reference
//! implementation the M138 spec points at):
//!
//!   * modifiers: `ctrl`/`control`, `shift`, `alt`/`option`/`meta`,
//!     `cmd`/`command`/`super`, `hyper` — any combination, any order,
//!     case-insensitive, joined with `+`.
//!   * named keys: `enter`/`return`, `esc`/`escape`, `tab`, `backspace`/`bs`,
//!     `space` (also a literal `" "`), arrows (`left`/`right`/`up`/`down`).
//!   * function keys: `f1`..`f12` (any `fN`).
//!   * named symbols: `minus`, `comma`, `period`, `slash`, `backslash`,
//!     `quote`, `double_quote`/`double-quote`, `semicolon`, `colon`,
//!     `percent`, `ampersand`, `backtick`, `plus`.
//!   * single characters: any single (possibly non-ASCII) char. An uppercase
//!     ASCII letter auto-applies `SHIFT` and lowercases the code, so `"Q"`
//!     parses to `shift+q` — the pre-M138 `'q' | 'Q'` is_empty dance is gone.
//!
//! `shift+tab` normalizes to [`KeyCode::BackTab`] (crossterm reports a
//! shifted Tab as `BackTab` with no SHIFT modifier), so a config author can
//! write either `"shift+tab"` or the raw backtab and both resolve the same.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A parsed key binding: the key code plus the modifiers that must be held.
pub type KeyCombo = (KeyCode, KeyModifiers);

/// Parse a single modifier token (case-insensitive). Returns `None` for a
/// token that is not a modifier so the caller can treat it as the key.
fn parse_modifier_token(token: &str) -> Option<KeyModifiers> {
    match token.to_lowercase().as_str() {
        "ctrl" | "control" => Some(KeyModifiers::CONTROL),
        "shift" => Some(KeyModifiers::SHIFT),
        "alt" | "option" | "meta" => Some(KeyModifiers::ALT),
        "cmd" | "command" | "super" => Some(KeyModifiers::SUPER),
        "hyper" => Some(KeyModifiers::HYPER),
        _ => None,
    }
}

/// Return the single `char` in `s`, or `None` when `s` is empty or has more
/// than one `char` (multi-byte chars count as one).
fn single_key_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch)
    } else {
        None
    }
}

/// Parse a human key-combo string into a [`KeyCombo`], or `None` when the
/// string is malformed (empty part, two non-modifier tokens, unknown `fN`,
/// unknown multi-char name).
///
/// Both `+` and `-` are accepted as token separators (M201 fix: the
/// `KEYBIND_DEFAULTS` table canonicalizes chords like `Ctrl-R` and the
/// `Left, BackTab` alias uses `-` for the back-tab name). To bind the
/// literal `-` key, use the named keyword `minus`.
///
/// See the module docs for the full accepted grammar.
pub fn parse_key_combo(s: &str) -> Option<KeyCombo> {
    let parts: Vec<&str> = split_combo(s);
    let mut modifiers = KeyModifiers::empty();
    let mut key_str: Option<&str> = None;

    for part in &parts {
        // A truly empty part comes from a stray separator ("ctrl+", "+a",
        // "ctrl-", "a--b") and is always malformed. A whitespace-only part,
        // by contrast, is the literal space key (`" "`), which we must not
        // trim away.
        if part.is_empty() {
            return None;
        }
        let trimmed = part.trim();
        let token = if trimmed.is_empty() { " " } else { trimmed };
        if let Some(modifier) = parse_modifier_token(token) {
            modifiers |= modifier;
        } else if key_str.is_some() {
            // Two non-modifier tokens ("a+b") is not a valid single combo.
            return None;
        } else {
            key_str = Some(token);
        }
    }

    let key_str = key_str?;
    let single_char = single_key_char(key_str);
    let lower = key_str.to_lowercase();
    let code = match lower.as_str() {
        "space" | " " => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" if modifiers.contains(KeyModifiers::SHIFT) => {
            modifiers.remove(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "tab" => KeyCode::Tab,
        "backtab" | "shift+tab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "pageup" | "page_up" | "pgup" => KeyCode::PageUp,
        "pagedown" | "page_down" | "pgdn" => KeyCode::PageDown,
        "minus" => KeyCode::Char('-'),
        "comma" => KeyCode::Char(','),
        "period" => KeyCode::Char('.'),
        "slash" => KeyCode::Char('/'),
        "backslash" => KeyCode::Char('\\'),
        "quote" => KeyCode::Char('\''),
        "double_quote" | "double-quote" => KeyCode::Char('"'),
        "semicolon" => KeyCode::Char(';'),
        "colon" => KeyCode::Char(':'),
        "percent" => KeyCode::Char('%'),
        "ampersand" => KeyCode::Char('&'),
        "backtick" => KeyCode::Char('`'),
        "plus" => KeyCode::Char('+'),
        _ if single_char.is_some() => {
            let ch = single_char?;
            if ch.is_ascii_uppercase() {
                modifiers |= KeyModifiers::SHIFT;
                KeyCode::Char(ch.to_ascii_lowercase())
            } else {
                KeyCode::Char(ch)
            }
        }
        s if s.starts_with('f') => s[1..].parse::<u8>().ok().map(KeyCode::F)?,
        _ => return None,
    };

    Some(normalize_key_combo((code, modifiers)))
}

/// Split a combo string on `+` or `-`. Each separator must have non-empty
/// content on both sides (so a leading `-` is NOT a separator — it leaves
/// the literal `-` char token alone, which the caller routes to the `minus`
/// named key).
fn split_combo(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'+' || b == b'-' {
            // Reject stray leading/trailing/consecutive separators by
            // splitting but letting the empty-part check in `parse_key_combo`
            // catch them. Allow a single `-` token at the very start or end
            // (the caller treats it as the literal minus char token).
            if start < i {
                parts.push(&s[start..i]);
                start = i + 1;
            } else {
                // Empty left half (e.g. "-R" or "--") — keep the separator as
                // part of the next token so the empty-part check fires.
                start = i;
                i += 1;
                continue;
            }
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

/// Canonicalize a combo so equality comparisons are robust:
///
///   * `Tab + SHIFT` → `BackTab` (crossterm's shifted-Tab representation);
///   * a bare `BackTab` drops any stray `SHIFT` modifier.
pub fn normalize_key_combo((mut code, mut modifiers): KeyCombo) -> KeyCombo {
    if matches!(code, KeyCode::Tab) && modifiers.contains(KeyModifiers::SHIFT) {
        code = KeyCode::BackTab;
        modifiers.remove(KeyModifiers::SHIFT);
    } else if matches!(code, KeyCode::BackTab) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    (code, modifiers)
}

/// Does a live `KeyEvent` match the given `combo`? Both sides are normalized
/// first so `shift+tab` (stored as `BackTab`) matches a terminal `BackTab`.
///
/// Crossterm sometimes reports uppercase letters as `Char('Q')` with `SHIFT`
/// and sometimes as `Char('Q')` without it (terminal-dependent). To be
/// forgiving we compare the case-folded char when the only difference is a
/// `SHIFT` on an alphabetic key.
pub fn key_event_matches_combo(key: &KeyEvent, combo: KeyCombo) -> bool {
    let (actual_code, actual_mods) = normalize_key_combo((key.code, key.modifiers));
    let (expected_code, expected_mods) = normalize_key_combo(combo);

    if actual_code == expected_code && actual_mods == expected_mods {
        return true;
    }

    // Tolerate the uppercase-letter ambiguity: a terminal may deliver `Q` as
    // `Char('Q')` (no SHIFT) while the parsed combo is `shift+q`
    // (`Char('q') + SHIFT`). Fold both to lowercase + SHIFT and re-compare.
    if let (KeyCode::Char(a), KeyCode::Char(e)) = (actual_code, expected_code) {
        let fold = |c: char, m: KeyModifiers| -> (char, KeyModifiers) {
            if c.is_ascii_uppercase() {
                (c.to_ascii_lowercase(), m | KeyModifiers::SHIFT)
            } else {
                (c, m)
            }
        };
        return fold(a, actual_mods) == fold(e, expected_mods);
    }

    false
}

/// Format a [`KeyCombo`] for on-screen display, e.g. `Ctrl+Shift+Up`,
/// `Enter`, `?`, `F12`. Used by the help overlay and footer so the displayed
/// legend is generated from the same data the dispatcher resolves against.
pub fn format_key_combo((code, modifiers): KeyCombo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if modifiers.contains(KeyModifiers::SUPER) {
        parts.push("Super".to_string());
    }
    if modifiers.contains(KeyModifiers::HYPER) {
        parts.push("Hyper".to_string());
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    parts.push(format_key_code(code));
    parts.join("+")
}

/// Format just the key code (no modifiers).
fn format_key_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Shift+Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // M201 F-02: parse_key_combo must accept every default keybind chord
    // emitted by `mp config schema` (the KEYBIND_DEFAULTS table uses both
    // `+` and `-` separators, plus the named keys `PageUp`/`PageDown`/
    // `BackTab`). A pre-validation gate in the Settings keybind editor
    // (S9) runs the user's buffer through this parser on commit; if any
    // canonical chord fails to parse, the user sees a "not a valid key
    // combo" error on the very defaults shipped by the schema.

    #[test]
    fn parse_key_combo_accepts_dash_separator() {
        assert_eq!(
            parse_key_combo("Ctrl-R"),
            parse_key_combo("Ctrl+R"),
            "dash and plus separators must be equivalent"
        );
        // `Ctrl-r` (lowercase) → CONTROL + Char('r'), no SHIFT.
        assert_eq!(
            parse_key_combo("Ctrl-r"),
            Some((KeyCode::Char('r'), KeyModifiers::CONTROL)),
            "lowercase r with Ctrl produces CONTROL only (no SHIFT)"
        );
    }

    #[test]
    fn parse_key_combo_accepts_pageup_pagedown() {
        // Default values for keybinds.page_up / keybinds.page_down.
        assert_eq!(
            parse_key_combo("PageUp"),
            Some((KeyCode::PageUp, KeyModifiers::empty()))
        );
        assert_eq!(
            parse_key_combo("PageDown"),
            Some((KeyCode::PageDown, KeyModifiers::empty()))
        );
        assert_eq!(
            parse_key_combo("pageup"),
            Some((KeyCode::PageUp, KeyModifiers::empty())),
            "case-insensitive"
        );
        assert_eq!(
            parse_key_combo("pgup"),
            Some((KeyCode::PageUp, KeyModifiers::empty())),
            "PgUp alias"
        );
    }

    #[test]
    fn parse_key_combo_accepts_backtab() {
        // Default value for keybinds.previous_lane: "Left, BackTab".
        // The Settings keybind editor splits on `,` first, then runs each
        // chord through parse_key_combo. "BackTab" alone must parse.
        assert_eq!(
            parse_key_combo("BackTab"),
            Some((KeyCode::BackTab, KeyModifiers::empty())),
            "BackTab must parse without a `shift+tab` workaround"
        );
        assert_eq!(
            parse_key_combo("shift+tab"),
            Some((KeyCode::BackTab, KeyModifiers::empty())),
            "shift+tab still normalizes to BackTab"
        );
    }

    #[test]
    fn parse_key_combo_accepts_all_keybind_defaults() {
        // Exercise every chord in KEYBIND_DEFAULTS at parser level so a
        // future change cannot silently drop one. The schema emits these
        // as the `default` for each keybind row.
        let defaults: &[&str] = &[
            "q", "Q", "Up", "k", "Down", "j", "PageUp", "PageDown", "Enter", "Esc", "?", "f", "h",
            "A", "r", "R", "p", "m", "Ctrl-O", "Left", "BackTab", "Right", "l", "Tab", "Ctrl-R",
            "]", "[", "n", "F", "g", "/", "o",
        ];
        for chord in defaults {
            assert!(
                parse_key_combo(chord).is_some(),
                "KEYBIND_DEFAULTS chord `{chord}` failed to parse"
            );
        }
    }

    #[test]
    fn parse_key_combo_still_rejects_garbage() {
        assert!(parse_key_combo("zzznotreal").is_none());
        assert!(parse_key_combo("ctrl+").is_none(), "stray separator");
        assert!(parse_key_combo("+a").is_none(), "leading separator");
        assert!(parse_key_combo("a+b").is_none(), "two key tokens");
    }

    #[test]
    fn parse_key_combo_split_combo_handles_dash_and_plus() {
        // The internal split_combo helper is exercised through parse_key_combo;
        // pin the boundary cases that motivated the dash-separator support.
        // Lowercase final key → no SHIFT modifier added.
        assert_eq!(
            parse_key_combo("Ctrl-Alt-r").map(|c| c.1),
            Some(KeyModifiers::CONTROL | KeyModifiers::ALT)
        );
        assert_eq!(
            parse_key_combo("Ctrl-Alt-r").map(|c| c.0),
            Some(KeyCode::Char('r'))
        );
        // Mixed separators (Ctrl+Alt-Del style) — note: 'Del' is not a
        // recognized named key in this build, so we only assert the
        // parsing path; the actual result is `None` for unknown keys.
        assert!(parse_key_combo("Ctrl+Alt-r").is_some());
    }
}
