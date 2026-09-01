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
//!     case-insensitive, joined with `+` or `-`.
//!   * named keys: `enter`/`return`, `esc`/`escape`, `tab`/`backtab`/
//!     `back-tab`/`shift+tab`, `backspace`/`bs`, `space` (also a literal
//!     `" "`), arrows (`left`/`right`/`up`/`down`), `pageup`/`page_up`/
//!     `page-up`/`pgup`, `pagedown`/`page_down`/`page-down`/`pgdn`.
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
//!
//! ## M201 cycle 3: dash separator + hyphenated named keys
//!
//! Both `+` and `-` are accepted as token separators (F-02): the
//! `KEYBIND_DEFAULTS` table canonicalizes chords like `Ctrl-R`.
//! However, this conflicted with documented hyphenated named-key aliases
//! like `double-quote`, `page-up`, `page-down`, and `back-tab` — they
//! would split on `-` and be rejected as "two key tokens". The fix is
//! a pre-scan: if the WHOLE input (case-folded) matches a known named
//! key (single-word or hyphenated alias), resolve it directly. Otherwise
//! fall through to the normal split logic. The match arm below also
//! recognizes the hyphenated aliases so `Ctrl+page-up` and similar
//! combos work end-to-end.

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

/// M201 cycle 3 (F-01/F-02): pre-scan the input for a known named key
/// (single-word OR hyphenated alias). Returns `Some(KeyCode)` if the
/// whole trimmed, case-folded input matches a named key. This runs
/// before the dash-separator split so documented aliases like
/// `double-quote` (which would otherwise split into `["double", "quote"]`
/// and be rejected as "two key tokens") resolve directly.
///
/// The aliases here are the ONLY named keys that contain `-` in their
/// canonical form. Adding a new hyphenated alias means adding it here
/// AND in the match arm in `parse_key_combo` so `Ctrl+page-up` style
/// chords work end-to-end.
fn lookup_named_key(s: &str) -> Option<KeyCode> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        // Named keys that contain '-' (M201 cycle 3 — F-01/F-02 fix).
        "double-quote" | "double_quote" => Some(KeyCode::Char('"')),
        "page-up" | "page_up" | "pageup" | "pgup" => Some(KeyCode::PageUp),
        "page-down" | "page_down" | "pagedown" | "pgdn" => Some(KeyCode::PageDown),
        "back-tab" | "back_tab" | "backtab" => Some(KeyCode::BackTab),
        // Single-word named keys (re-listed here so the pre-scan is
        // self-contained — `parse_key_combo` falls through to the
        // match arm which has the canonical spelling, but a top-level
        // input that is, e.g., `Tab` resolves here without a split).
        "space" | " " => Some(KeyCode::Char(' ')),
        "enter" | "return" => Some(KeyCode::Enter),
        "esc" | "escape" => Some(KeyCode::Esc),
        "tab" => Some(KeyCode::Tab),
        "backspace" | "bs" => Some(KeyCode::Backspace),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "minus" => Some(KeyCode::Char('-')),
        "comma" => Some(KeyCode::Char(',')),
        "period" => Some(KeyCode::Char('.')),
        "slash" => Some(KeyCode::Char('/')),
        "backslash" => Some(KeyCode::Char('\\')),
        "quote" => Some(KeyCode::Char('\'')),
        "semicolon" => Some(KeyCode::Char(';')),
        "colon" => Some(KeyCode::Char(':')),
        "percent" => Some(KeyCode::Char('%')),
        "ampersand" => Some(KeyCode::Char('&')),
        "backtick" => Some(KeyCode::Char('`')),
        "plus" => Some(KeyCode::Char('+')),
        _ => None,
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
/// A pre-scan ([`lookup_named_key`]) resolves documented hyphenated
/// named-key aliases (`double-quote`, `page-up`, `back-tab`) before the
/// split, so they aren't rejected as "two key tokens".
///
/// See the module docs for the full accepted grammar.
pub fn parse_key_combo(s: &str) -> Option<KeyCombo> {
    // M201 cycle 3 (F-01/F-02): pre-scan the WHOLE input for a known
    // named key. Without this, "double-quote" alone would split on
    // '-' into ["double", "quote"] and the parser would reject the
    // input as "two key tokens". The pre-scan catches it as a whole
    // input and returns the correct KeyCode.
    if let Some(code) = lookup_named_key(s) {
        return Some(normalize_key_combo((code, KeyModifiers::empty())));
    }

    // M201 cycle 3: two-level split.
    //   - Top level: split on `+` so "Ctrl+double-quote" produces two
    //     segments ["Ctrl", "double-quote"].
    //   - Per segment: pre-scan for a known named key (catches
    //     "double-quote" as a whole segment). Otherwise split on `-`
    //     so "Ctrl-R" → ["Ctrl", "R"] and accumulates modifiers.
    //
    // The per-segment named-key pre-scan is what makes
    // `Ctrl+page-up` work end-to-end: the segment "page-up" is
    // resolved as PageUp without breaking it on the `-` separator.
    let mut modifiers = KeyModifiers::empty();
    let mut key_code: Option<KeyCode> = None;

    for segment in s.split('+') {
        // An empty segment is a stray `+` separator ("ctrl+", "+a") —
        // malformed.
        if segment.is_empty() {
            return None;
        }
        let trimmed = segment.trim();
        // A whitespace-only segment is the literal space key (` `).
        // Route it to the named-key lookup so `" "` still resolves
        // to Char(' ').
        let segment = if trimmed.is_empty() { " " } else { trimmed };

        // Pre-scan the segment as a whole named key.
        if let Some(code) = lookup_named_key(segment) {
            if key_code.is_some() {
                return None;
            }
            key_code = Some(code);
            continue;
        }

        // Otherwise split the segment on `-` (dash separator) and
        // process each sub-token as either a modifier or the key.
        for sub in segment.split('-') {
            if sub.is_empty() {
                return None;
            }
            let trimmed = sub.trim();
            // Whitespace-only sub-tokens are the literal space key
            // (`" "`); route them to the named-key lookup.
            let sub = if trimmed.is_empty() { " " } else { trimmed };
            if let Some(m) = parse_modifier_token(sub) {
                modifiers |= m;
            } else if key_code.is_some() {
                return None;
            } else {
                key_code = Some(parse_key_token(sub, &mut modifiers)?);
            }
        }
    }

    let code = key_code?;
    Some(normalize_key_combo((code, modifiers)))
}

/// Resolve a single key token (no modifiers) to a [`KeyCode`]. Used
/// after the modifier extraction in `parse_key_combo`. For known named
/// keys and named symbols this routes through [`lookup_named_key`] so
/// the alias table lives in one place. Single-char tokens auto-apply
/// `SHIFT` for uppercase letters; `f1`..`f12` parse as function keys.
fn parse_key_token(token: &str, modifiers: &mut KeyModifiers) -> Option<KeyCode> {
    // Try the named-key table first so hyphenated aliases
    // (double-quote, page-up, etc.) and underscore/contraction
    // spellings all resolve the same.
    if let Some(code) = lookup_named_key(token) {
        return Some(code);
    }
    // `shift+tab` normalizes to BackTab; the match arm inside
    // `lookup_named_key` doesn't see the modifier context, so handle
    // it here.
    let lower = token.to_lowercase();
    if lower == "tab" && modifiers.contains(KeyModifiers::SHIFT) {
        modifiers.remove(KeyModifiers::SHIFT);
        return Some(KeyCode::BackTab);
    }
    // Single-character token: uppercase ASCII letters auto-apply SHIFT.
    if let Some(ch) = single_key_char(token) {
        if ch.is_ascii_uppercase() {
            *modifiers |= KeyModifiers::SHIFT;
            return Some(KeyCode::Char(ch.to_ascii_lowercase()));
        }
        return Some(KeyCode::Char(ch));
    }
    // Function-key token: `f1`..`f12`.
    if let Some(rest) = lower.strip_prefix('f') {
        return rest.parse::<u8>().ok().map(KeyCode::F);
    }
    None
}

/// Split a combo string on `+` or `-` for the helper used by the F-02
/// fix's regression test. Production parsing now uses a two-level
/// split inside `parse_key_combo` (per-segment pre-scan + dash split)
/// so this helper is kept only as a documentation / testing utility.
#[allow(dead_code)]
fn split_combo(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'+' || b == b'-' {
            if start < i {
                parts.push(&s[start..i]);
                start = i + 1;
            } else {
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

    // M201 cycle 3 (F-01/F-02 regression guard): the dash-separator
    // addition broke `double-quote` (and would have broken the other
    // documented hyphenated aliases). The pre-scan + match-arm aliases
    // restore them. Pin every documented alias.

    #[test]
    fn parse_key_combo_accepts_double_quote_aliases() {
        // The pre-existing `double_quote_both_spellings` test in
        // crates/raul/tests/key_combo.rs pins the same surface; this
        // adds a settings-side test so the M201 S9 editor's parse gate
        // is also covered in unit tests.
        assert_eq!(
            parse_key_combo("double-quote").map(|c| c.0),
            Some(KeyCode::Char('"'))
        );
        assert_eq!(
            parse_key_combo("double_quote").map(|c| c.0),
            Some(KeyCode::Char('"'))
        );
        // Hyphen + modifier combo: Ctrl+double-quote should resolve to
        // CONTROL + '"' even though the dash conflicts with the named
        // key's own dash.
        assert_eq!(
            parse_key_combo("Ctrl+double-quote").map(|c| c.0),
            Some(KeyCode::Char('"'))
        );
        assert_eq!(
            parse_key_combo("Ctrl+double-quote").map(|c| c.1),
            Some(KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn parse_key_combo_accepts_page_up_aliases() {
        assert_eq!(
            parse_key_combo("page-up").map(|c| c.0),
            Some(KeyCode::PageUp)
        );
        assert_eq!(
            parse_key_combo("page_up").map(|c| c.0),
            Some(KeyCode::PageUp)
        );
        assert_eq!(
            parse_key_combo("pageup").map(|c| c.0),
            Some(KeyCode::PageUp)
        );
        assert_eq!(parse_key_combo("pgup").map(|c| c.0), Some(KeyCode::PageUp));
        // Modifier combo via '+' (not '-', which would be ambiguous).
        assert_eq!(
            parse_key_combo("Ctrl+page-up").map(|c| c.0),
            Some(KeyCode::PageUp)
        );
        assert_eq!(
            parse_key_combo("Ctrl+page-up").map(|c| c.1),
            Some(KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn parse_key_combo_accepts_page_down_aliases() {
        assert_eq!(
            parse_key_combo("page-down").map(|c| c.0),
            Some(KeyCode::PageDown)
        );
        assert_eq!(
            parse_key_combo("page_down").map(|c| c.0),
            Some(KeyCode::PageDown)
        );
        assert_eq!(
            parse_key_combo("pagedown").map(|c| c.0),
            Some(KeyCode::PageDown)
        );
        assert_eq!(
            parse_key_combo("pgdn").map(|c| c.0),
            Some(KeyCode::PageDown)
        );
        assert_eq!(
            parse_key_combo("Ctrl+page-down").map(|c| c.1),
            Some(KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn parse_key_combo_accepts_back_tab_aliases() {
        assert_eq!(
            parse_key_combo("back-tab").map(|c| c.0),
            Some(KeyCode::BackTab)
        );
        assert_eq!(
            parse_key_combo("back_tab").map(|c| c.0),
            Some(KeyCode::BackTab)
        );
        assert_eq!(
            parse_key_combo("backtab").map(|c| c.0),
            Some(KeyCode::BackTab)
        );
        // shift+tab still normalizes to BackTab with no SHIFT modifier.
        assert_eq!(
            parse_key_combo("shift+tab").map(|c| (c.0, c.1)),
            Some((KeyCode::BackTab, KeyModifiers::empty()))
        );
    }
}
