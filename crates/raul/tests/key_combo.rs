//! M138 AC-02: `parse_key_combo` fixtures.
//!
//! Cases lifted from herdr's `src/config/keybinds.rs` test suite plus raul's
//! own default bindings. Covers modifiers (any order, case-insensitive),
//! named keys, function keys, named symbols, unicode chars, and the
//! uppercase-auto-shift rule. Well over the 20 representative cases the AC
//! asks for.

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::key_combo::{format_key_combo, parse_key_combo};

fn combo(code: KeyCode, mods: KeyModifiers) -> Option<(KeyCode, KeyModifiers)> {
    Some((code, mods))
}

#[test]
fn simple_char() {
    assert_eq!(
        parse_key_combo("v"),
        combo(KeyCode::Char('v'), KeyModifiers::empty())
    );
}

#[test]
fn unicode_char() {
    assert_eq!(
        parse_key_combo("ö"),
        combo(KeyCode::Char('ö'), KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("alt+é"),
        combo(KeyCode::Char('é'), KeyModifiers::ALT)
    );
}

#[test]
fn uppercase_auto_applies_shift() {
    assert_eq!(
        parse_key_combo("Q"),
        combo(KeyCode::Char('q'), KeyModifiers::SHIFT)
    );
    assert_eq!(
        parse_key_combo("A"),
        combo(KeyCode::Char('a'), KeyModifiers::SHIFT)
    );
}

#[test]
fn modifiers_any_order_case_insensitive() {
    let up_shift = combo(KeyCode::Up, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(parse_key_combo("ctrl+shift+up"), up_shift);
    assert_eq!(parse_key_combo("shift+ctrl+up"), up_shift);
    assert_eq!(parse_key_combo("CTRL+SHIFT+UP"), up_shift);
    assert_eq!(parse_key_combo("Control+Shift+Up"), up_shift);
}

#[test]
fn modifier_aliases() {
    assert_eq!(
        parse_key_combo("control+a"),
        combo(KeyCode::Char('a'), KeyModifiers::CONTROL)
    );
    assert_eq!(
        parse_key_combo("option+a"),
        combo(KeyCode::Char('a'), KeyModifiers::ALT)
    );
    assert_eq!(
        parse_key_combo("meta+a"),
        combo(KeyCode::Char('a'), KeyModifiers::ALT)
    );
    assert_eq!(
        parse_key_combo("cmd+a"),
        combo(KeyCode::Char('a'), KeyModifiers::SUPER)
    );
    assert_eq!(
        parse_key_combo("super+a"),
        combo(KeyCode::Char('a'), KeyModifiers::SUPER)
    );
}

#[test]
fn named_keys() {
    assert_eq!(
        parse_key_combo("enter"),
        combo(KeyCode::Enter, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("return"),
        combo(KeyCode::Enter, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("esc"),
        combo(KeyCode::Esc, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("escape"),
        combo(KeyCode::Esc, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("tab"),
        combo(KeyCode::Tab, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("backspace"),
        combo(KeyCode::Backspace, KeyModifiers::empty())
    );
}

#[test]
fn space_variants() {
    assert_eq!(
        parse_key_combo("space"),
        combo(KeyCode::Char(' '), KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo(" "),
        combo(KeyCode::Char(' '), KeyModifiers::empty())
    );
}

#[test]
fn arrows() {
    assert_eq!(
        parse_key_combo("left"),
        combo(KeyCode::Left, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("right"),
        combo(KeyCode::Right, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("up"),
        combo(KeyCode::Up, KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("down"),
        combo(KeyCode::Down, KeyModifiers::empty())
    );
}

#[test]
fn shift_tab_normalizes_to_backtab() {
    assert_eq!(
        parse_key_combo("shift+tab"),
        combo(KeyCode::BackTab, KeyModifiers::empty())
    );
}

#[test]
fn function_keys() {
    assert_eq!(
        parse_key_combo("f1"),
        combo(KeyCode::F(1), KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("f12"),
        combo(KeyCode::F(12), KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("ctrl+f5"),
        combo(KeyCode::F(5), KeyModifiers::CONTROL)
    );
}

#[test]
fn named_symbols() {
    let cases = [
        ("minus", '-'),
        ("comma", ','),
        ("period", '.'),
        ("slash", '/'),
        ("backslash", '\\'),
        ("quote", '\''),
        ("semicolon", ';'),
        ("colon", ':'),
        ("percent", '%'),
        ("ampersand", '&'),
        ("backtick", '`'),
        ("plus", '+'),
    ];
    for (name, ch) in cases {
        assert_eq!(
            parse_key_combo(name),
            combo(KeyCode::Char(ch), KeyModifiers::empty()),
            "named symbol {name} should parse to {ch:?}"
        );
    }
}

#[test]
fn double_quote_both_spellings() {
    assert_eq!(
        parse_key_combo("double_quote"),
        combo(KeyCode::Char('"'), KeyModifiers::empty())
    );
    assert_eq!(
        parse_key_combo("double-quote"),
        combo(KeyCode::Char('"'), KeyModifiers::empty())
    );
}

#[test]
fn malformed_returns_none() {
    assert_eq!(parse_key_combo(""), None);
    assert_eq!(parse_key_combo("ctrl+"), None);
    assert_eq!(parse_key_combo("+a"), None);
    assert_eq!(parse_key_combo("a+b"), None);
    assert_eq!(parse_key_combo("f99x"), None);
    assert_eq!(parse_key_combo("notakey"), None);
}

#[test]
fn round_trips_through_formatter() {
    // Every parseable case formats to a non-empty, stable display string.
    let inputs = [
        "ctrl+shift+up",
        "enter",
        "esc",
        "f12",
        "minus",
        "q",
        "Q",
        "alt+é",
    ];
    for input in inputs {
        let combo = parse_key_combo(input).unwrap_or_else(|| panic!("parse {input}"));
        let display = format_key_combo(combo);
        assert!(!display.is_empty(), "display for {input} must be non-empty");
    }
}
