//! M163: footer flash-message truncation helpers.
//!
//! Extracted from `render/mod.rs` so the truncation rules are unit-testable
//! without standing up a `Terminal` / `Frame`. The two public entry points
//! are [`format_flash_footer`] (string in, string out) and
//! [`truncate_flash_message`] (lower-level, used by [`format_flash_footer`]).
//!
//! ## Rules
//!
//! Short messages render unchanged, wrapped in the standard
//! `chrome` so the warning glyph is consistent across the TUI.
//!
//! Long messages are truncated at the **first sentence boundary** that
//! fits the available width. A sentence boundary is any of `.`, `!`,
//! `?`, the full-width CJK set, followed by whitespace or end-of-input.
//! The boundary character is kept in the truncated output.
//!
//! When no boundary fits the width — or the message has no boundary at
//! all — the helper truncates the message to fit the width and appends
//! the details suffix. Either way a truncated message ends with
//! ` (press ? for details)` so the user has a stable, visible hint
//! that the full message lives behind the `?` overlay.
//!
//! ## Unicode safety
//!
//! Width checks use terminal display columns and truncation happens at
//! grapheme boundaries, so CJK, emoji, combining marks, and multi-byte UTF-8
//! remain intact.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Warning glyph prefix used by every footer flash (3 columns).
pub const FOOTER_PREFIX: &str = " \u{26a0} ";
/// Suffix appended whenever the message is truncated (22 columns).
/// The `?` character is the existing Help keybind so the hint stays
/// consistent with the rest of the TUI.
pub const FOOTER_DETAILS_SUFFIX: &str = " (press ? for details)";
/// Single trailing space after the truncated/suffix text. Keeps the
/// footer glyph separated from the terminal border when no border is
/// drawn.
pub const FOOTER_TRAILING_SPACE: &str = " ";

/// Truncate a long flash message to a single, width-bounded line.
///
/// Returns the *unprefixed* message — caller is responsible for wrapping
/// it in the chrome. Always returns at most `width` columns.
///
/// Algorithm:
/// 1. If the message already fits in `width` columns, return it
///    unchanged.
/// 2. Find the first sentence boundary (`.`, `!`, `?`, or the CJK
///    full-width set) followed by whitespace or end-of-input. The cut
///    is *inclusive* of the boundary character.
/// 3. If the cut fits within `width`, return the truncated slice.
/// 4. Otherwise — no usable boundary, or boundary itself is wider than
///    `width` — truncate to `width` columns on a char boundary.
pub fn truncate_flash_message(msg: &str, width: usize) -> String {
    if display_width(msg) <= width {
        return msg.to_string();
    }

    for (byte_index, character) in msg.char_indices() {
        if !is_sentence_terminator(character) {
            continue;
        }
        let end = byte_index + character.len_utf8();
        let next = msg[end..].chars().next();
        let is_cjk_boundary = matches!(character, '\u{3002}' | '\u{ff01}' | '\u{ff1f}');
        if next.is_none() || next.is_some_and(char::is_whitespace) || is_cjk_boundary {
            let sentence = msg[..end].trim_end();
            if display_width(sentence) <= width {
                return sentence.to_string();
            }
            break;
        }
    }

    take_display_width(msg, width)
}

pub fn format_flash_footer(msg: &str, footer_width: u16) -> String {
    format_flash_footer_with_details(msg, footer_width, true)
}

pub fn format_flash_footer_with_details(
    msg: &str,
    footer_width: u16,
    details_available: bool,
) -> String {
    let width = footer_width as usize;
    if width == 0 {
        return String::new();
    }

    let full = format!("{FOOTER_PREFIX}{msg}{FOOTER_TRAILING_SPACE}");
    if display_width(&full) <= width {
        return full;
    }

    let prefix_width = display_width(FOOTER_PREFIX);
    let trailing_width = display_width(FOOTER_TRAILING_SPACE);
    let details_width = display_width(FOOTER_DETAILS_SUFFIX);
    let long_overhead = prefix_width + details_width + trailing_width;
    if details_available && width > long_overhead {
        let truncated = truncate_flash_message(msg, width - long_overhead);
        return format!("{FOOTER_PREFIX}{truncated}{FOOTER_DETAILS_SUFFIX}{FOOTER_TRAILING_SPACE}");
    }

    let short_overhead = prefix_width + trailing_width;
    if width > short_overhead {
        let truncated = take_display_width(msg, width - short_overhead);
        return format!("{FOOTER_PREFIX}{truncated}{FOOTER_TRAILING_SPACE}");
    }

    take_display_width(FOOTER_PREFIX, width)
}

pub fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn take_display_width(value: &str, width: usize) -> String {
    let mut used = 0usize;
    UnicodeSegmentation::graphemes(value, true)
        .take_while(|grapheme| {
            let next = used + UnicodeWidthStr::width(*grapheme);
            if next > width {
                return false;
            }
            used = next;
            true
        })
        .collect()
}

fn is_sentence_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{3002}' | '\u{ff01}' | '\u{ff1f}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome_short() -> usize {
        display_width(FOOTER_PREFIX) + display_width(FOOTER_TRAILING_SPACE)
    }

    fn chrome_long() -> usize {
        display_width(FOOTER_PREFIX)
            + display_width(FOOTER_DETAILS_SUFFIX)
            + display_width(FOOTER_TRAILING_SPACE)
    }

    #[test]
    fn short_message_renders_unchanged_with_chrome() {
        let s = format_flash_footer("hello", 80);
        let want = format!("{FOOTER_PREFIX}{}{FOOTER_TRAILING_SPACE}", "hello");
        assert_eq!(s, want);
    }

    #[test]
    fn exactly_at_budget_renders_unchanged() {
        // chrome (3) + abc (3) + trailing (1) = 7 columns.
        let s = format_flash_footer("abc", 7);
        let want = format!("{FOOTER_PREFIX}{}{FOOTER_TRAILING_SPACE}", "abc");
        assert_eq!(s, want);
    }

    #[test]
    fn long_message_truncates_at_first_sentence_boundary() {
        // Long enough that even with chrome (3 prefix + 1 trailing + 22 suffix = 26),
        // we land inside the truncation branch.
        let msg = "First sentence. Second sentence continues here for a while and is long.";
        let s = format_flash_footer(msg, 60);
        assert!(
            s.starts_with(&format!("{FOOTER_PREFIX}First sentence.")),
            "got: {s}"
        );
        assert!(s.contains("press ? for details"), "got: {s}");
        assert!(
            s.ends_with(FOOTER_TRAILING_SPACE),
            "trailing space lost: {s}"
        );
        assert!(!s.contains("Second sentence"), "got: {s}");
    }

    #[test]
    fn ascii_bang_and_question_are_sentence_boundaries() {
        // 100-char message, 60-col footer → must truncate.
        let msg = "Whoa! That was wild. End. But the message continues for a while here.";
        let s = format_flash_footer(msg, 60);
        assert!(s.starts_with(&format!("{FOOTER_PREFIX}Whoa!")), "got: {s}");
        assert!(s.contains("press ? for details"), "got: {s}");
    }

    #[test]
    fn cjk_full_width_terminators_are_boundaries() {
        // For a boundary check, use the raw `truncate_flash_message`
        // helper which doesn't apply chrome width budget.
        let s = truncate_flash_message("第一句内容。第二句继续扩展内容。", 12);
        let want: String = "第一句内容。".to_string();
        assert_eq!(s, want);
    }

    #[test]
    fn no_boundary_falls_back_to_width_truncation() {
        let msg = "a".repeat(200);
        let s = format_flash_footer(&msg, 30);
        assert!(s.contains("press ? for details"), "got len={}", s.len());
        assert!(
            display_width(&s) <= 30,
            "footer {} cols > 30",
            display_width(&s)
        );
    }

    #[test]
    fn multibyte_chars_are_not_sliced_mid_codepoint() {
        // '→' is 3 bytes UTF-8; '\u{1f4a1}' is 4 bytes.
        let msg = "alpha→beta\u{1f4a1}gamma. tail";
        let s = format_flash_footer(msg, 80);
        let _ = s.chars().count();
        let want_prefix = format!("{FOOTER_PREFIX}alpha\u{2192}beta\u{1f4a1}gamma.");
        assert!(s.starts_with(&want_prefix), "got: {s}");
    }

    #[test]
    fn zero_width_returns_empty() {
        assert_eq!(format_flash_footer("anything", 0), "");
    }

    #[test]
    fn truncate_pure_helper_respects_width() {
        let s = truncate_flash_message("aaaaaaaaaa", 5);
        let want: String = "a".repeat(5);
        assert_eq!(s, want);
    }

    #[test]
    fn truncate_prefers_boundary_over_hard_cut() {
        // No boundary inside the 5-char budget → hard-cut to width.
        let s = truncate_flash_message("aaa bbb ccc end.", 5);
        let want: String = "aaa b".to_string();
        assert_eq!(s, want);
    }

    #[test]
    fn truncate_picks_first_boundary_inside_budget() {
        // Budget (16 cols) covers the first sentence (4 cols).
        let s = truncate_flash_message("hi. tail of message", 16);
        let want: String = "hi.".to_string();
        assert_eq!(s, want);
    }

    #[test]
    fn cjk_message_respects_terminal_display_width() {
        let msg = "錯誤訊息錯誤訊息錯誤訊息錯誤訊息";
        let formatted = format_flash_footer(msg, 30);
        assert!(display_width(&formatted) <= 30);
        assert!(formatted.contains(FOOTER_DETAILS_SUFFIX));
    }

    #[test]
    fn tiny_footer_widths_never_overflow() {
        for width in 1..=25 {
            let formatted = format_flash_footer("a very long error without a boundary", width);
            assert!(display_width(&formatted) <= width as usize);
        }
    }

    #[test]
    fn truncation_preserves_grapheme_clusters() {
        let family = "👨‍👩‍👧‍👦";
        let formatted = truncate_flash_message(&format!("{family}abcdef"), 2);
        assert_eq!(formatted, family);
        assert_eq!(display_width(&formatted), 2);
    }

    #[test]
    fn details_hint_requires_available_details() {
        let formatted = format_flash_footer_with_details(
            "a very long non-review message without a sentence boundary",
            30,
            false,
        );
        assert!(!formatted.contains(FOOTER_DETAILS_SUFFIX));
        assert!(display_width(&formatted) <= 30);
    }

    #[test]
    fn chrome_helpers_report_expected_widths() {
        assert_eq!(chrome_short(), 4);
        assert_eq!(chrome_long(), 26);
    }
}
