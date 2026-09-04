//! M222 S6: full key-combo parser alias surface, exercised
//! end-to-end through `Keybinds::load_from_keybinds_toml`.
//!
//! AC-07: parser regressions cover the documented key surface:
//! underscore and dash aliases, `double_quote`/`double-quote`,
//! `page_up`/`page-down`, `BackTab`/`Backtab`/`back-tab`, mixed
//! case, modifiers, and combinations such as `Ctrl+double-quote`
//! and `Ctrl+page-up`.

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::keybinds::Keybinds;

fn pair(c: KeyCode, m: KeyModifiers) -> (KeyCode, KeyModifiers) {
    (c, m)
}

/// AC-07: underscore-vs-dash aliases — `page_up`, `page-up`,
/// `pageup`, `pgup` all parse to `KeyCode::PageUp`. The
/// documented surface is exercised through the TOML loader so
/// it ships the same shape the spec calls for.
#[test]
fn parser_accepts_all_page_up_aliases_via_toml() {
    for spelling in ["page_up", "page-up", "pageup", "pgup"] {
        let text = format!(
            r#"
[global]
page_up = "{}"
"#,
            spelling
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.is_empty(),
            "spelling `{spelling}` must not emit diagnostics; got: {diags:?}"
        );
        assert_eq!(
            kb.page_up,
            vec![pair(KeyCode::PageUp, KeyModifiers::empty())],
            "spelling `{spelling}` must bind to KeyCode::PageUp"
        );
    }
}

/// AC-07: `page_down`, `page-down`, `pagedown`, `pgdn` — same
/// expectation as the page_up surface.
#[test]
fn parser_accepts_all_page_down_aliases_via_toml() {
    for spelling in ["page_down", "page-down", "pagedown", "pgdn"] {
        let text = format!(
            r#"
[global]
page_down = "{}"
"#,
            spelling
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.is_empty(),
            "spelling `{spelling}` must not emit diagnostics; got: {diags:?}"
        );
        assert_eq!(
            kb.page_down,
            vec![pair(KeyCode::PageDown, KeyModifiers::empty())],
            "spelling `{spelling}` must bind to KeyCode::PageDown"
        );
    }
}

/// AC-07: `BackTab`, `Backtab`, `back-tab`, `shift+tab` all
/// normalize to `KeyCode::BackTab` (the SHIFT modifier is
/// dropped — crossterm reports shifted-Tab as `BackTab` with
/// no SHIFT).
#[test]
fn parser_accepts_all_back_tab_aliases_via_toml() {
    for spelling in ["BackTab", "Backtab", "back-tab", "shift+tab"] {
        let text = format!(
            r#"
[global]
previous_lane = "{}"
"#,
            spelling
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.is_empty(),
            "spelling `{spelling}` must not emit diagnostics; got: {diags:?}"
        );
        assert_eq!(
            kb.previous_lane,
            vec![pair(KeyCode::BackTab, KeyModifiers::empty())],
            "spelling `{spelling}` must normalize to KeyCode::BackTab with no SHIFT modifier"
        );
    }
}

/// AC-07: `double_quote` vs `double-quote` aliases. Both
/// resolve to `Char('"')`.
#[test]
fn parser_accepts_double_quote_aliases_via_toml() {
    for spelling in ["\"double_quote\"", "\"double-quote\""] {
        let text = format!(
            r#"
[autopilot]
select = {}
"#,
            spelling
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.is_empty(),
            "spelling `{spelling}` must not emit diagnostics; got: {diags:?}"
        );
        assert_eq!(
            kb.lane_autopilot.select,
            vec![pair(KeyCode::Char('"'), KeyModifiers::empty())],
            "spelling `{spelling}` must bind to Char('\"')"
        );
    }
}

/// AC-07: modifiers stack with the documented named keys.
/// `Ctrl+page-up`, `Ctrl+page-down`, `Ctrl+double-quote`, and
/// `Ctrl+back-tab` all parse to the SHIFT-less back-tab with
/// CONTROL.
#[test]
fn modifier_plus_dash_alias_combos_parse_via_toml() {
    let cases = [
        ("ctrl+page-up", KeyCode::PageUp, KeyModifiers::CONTROL),
        ("Ctrl+page-up", KeyCode::PageUp, KeyModifiers::CONTROL),
        ("ctrl+page-down", KeyCode::PageDown, KeyModifiers::CONTROL),
        // Match the docs: the `+page_down` spelling is also
        // underscore-compatible (the spec calls them aliases).
        ("ctrl+page_down", KeyCode::PageDown, KeyModifiers::CONTROL),
        ("ctrl+double-quote", KeyCode::Char('"'), KeyModifiers::CONTROL),
        ("ctrl+double_quote", KeyCode::Char('"'), KeyModifiers::CONTROL),
        // back-tab: SHIFT is consumed by `shift+tab` -> BackTab
        // normalization, so the final modifiers are CONTROL
        // only.
        ("ctrl+shift+tab", KeyCode::BackTab, KeyModifiers::CONTROL),
    ];
    for (spelling, expected_code, expected_mods) in cases {
        // Use a section-appropriate slot per spelling so the
        // assertion stays on the documented surface.
        let section = if spelling.contains("page-down") || spelling.contains("page_down") {
            "page_down"
        } else {
            "page_up"
        };
        let text = format!(
            r#"
[global]
{section} = "{spelling}"
"#,
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.is_empty(),
            "spelling `{spelling}` must not emit diagnostics; got: {diags:?}"
        );
        let combo = if section == "page_up" {
            kb.page_up.first().copied().expect("page_up must bind")
        } else {
            kb.page_down.first().copied().expect("page_down must bind")
        };
        assert_eq!(
            combo.0, expected_code,
            "spelling `{spelling}` must bind to expected KeyCode"
        );
        assert_eq!(
            combo.1, expected_mods,
            "spelling `{spelling}` must have expected modifiers"
        );
    }
}

/// AC-07: mixed case is allowed at the parser level. Bare
/// `PAGEUP`, `Shift+tab`, `Control-X` (uppercase final char
/// auto-applies SHIFT), and `shift+TAB` all parse. Note: we
/// intentionally avoid `Ctrl-o` here because it would shadow
/// the default `open_settings` (Ctrl-O) — the conflict
/// diagnostic is tested separately in
/// `keybind_invalid_entries.rs`.
#[test]
fn parser_accepts_mixed_case_modifiers_and_keys_via_toml() {
    let cases = [
        ("PAGEUP", KeyCode::PageUp, KeyModifiers::empty()),
        ("page_up", KeyCode::PageUp, KeyModifiers::empty()),
        // CTRL + lowercase final char -> no SHIFT.
        ("Ctrl-i", KeyCode::Char('i'), KeyModifiers::CONTROL),
        // CTRL + uppercase final char -> SHIFT auto-applied.
        (
            "Control-X",
            KeyCode::Char('x'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
        ("shift+TAB", KeyCode::BackTab, KeyModifiers::empty()),
        // Modifier-prefix mixed case.
        ("shift+ctrl+z", KeyCode::Char('z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
    ];
    for (spelling, expected_code, expected_mods) in cases {
        let text = format!(
            r#"
[global]
page_up = "{spelling}"
"#,
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        // PAGEUP / page_up etc. all bind to page_up so we
        // assert no diagnostic on the bind surface; the
        // actual conflict vs. defaults is checked elsewhere.
        assert!(
            diags.is_empty() || diags.iter().all(|d| d.field.contains("also bound to")),
            "spelling `{spelling}` must not warn on the bind itself; got: {diags:?}"
        );
        let combo = kb.page_up.first().copied().expect("page_up must bind");
        assert_eq!(
            combo.0, expected_code,
            "spelling `{spelling}` must bind to expected KeyCode"
        );
        assert_eq!(
            combo.1, expected_mods,
            "spelling `{spelling}` must have expected modifiers"
        );
    }
}

/// AC-07: rejects unrecognised / malformed combos. The
/// `KEYBIND_ALIAS_REJECTS_GARBAGE` pin lives here so a future
/// relaxation of the grammar trips this test.
#[test]
fn parser_rejects_garbage_combos_via_toml() {
    for spelling in ["zzznotreal", "ctrl+", "+a", "a+b"] {
        let text = format!(
            r#"
[global]
page_up = "{}"
"#,
            spelling
        );
        let (diags, kb) = Keybinds::load_from_keybinds_toml(&text);
        assert!(
            diags.iter().any(|d| d.field == "global.page_up" || d.field == "page_up"),
            "spelling `{spelling}` must surface a diagnostic; got: {diags:?}"
        );
        // The field falls back to the default PageUp binding
        // (i.e. the original page_up default) — pinning the
        // recovery shape.
        assert_eq!(
            kb.page_up,
            vec![pair(KeyCode::PageUp, KeyModifiers::empty())],
            "spelling `{spelling}` must reset page_up to the default"
        );
    }
}

/// AC-07: every documented override surface stays parseable
/// when fed through the TOML loader. We pin this as a single
/// composite fixture so a future grammar change trips one
/// test instead of N.
#[test]
fn every_documented_alias_parses_in_a_single_fixture() {
    let text = r#"
[global]
page_up = "page-up"
page_down = "page_down"
previous_lane = "shift+tab"
next_lane = "ctrl+double-quote"
help = "ctrl+page_up"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    assert!(
        diags.is_empty(),
        "the documented-surface fixture must not warn; got: {diags:?}"
    );
    assert_eq!(
        kb.page_up,
        vec![pair(KeyCode::PageUp, KeyModifiers::empty())]
    );
    assert_eq!(
        kb.page_down,
        vec![pair(KeyCode::PageDown, KeyModifiers::empty())]
    );
    assert_eq!(
        kb.previous_lane,
        vec![pair(KeyCode::BackTab, KeyModifiers::empty())]
    );
    assert_eq!(
        kb.next_lane,
        vec![pair(KeyCode::Char('"'), KeyModifiers::CONTROL)]
    );
    assert_eq!(
        kb.help,
        vec![pair(KeyCode::PageUp, KeyModifiers::CONTROL)]
    );
}
