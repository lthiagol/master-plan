//! Char-safe text helpers and the untrusted terminal display boundary.
//!
//! Truncation must operate on Unicode scalar values ([`char`]), not bytes.
//! Indexing a Rust string slice at an arbitrary byte index panics when the
//! index falls inside a multi-byte character (for example the em-dash `—`,
//! which is 3 bytes). Milestone titles regularly contain such characters, so
//! any byte-based truncation is a latent panic.

/// Truncate `s` to at most `max` chars, appending `"..."` when truncated.
/// Never panics on multi-byte input.
pub fn truncate(s: &str, max: usize) -> String {
    let sanitized = sanitize_display_line(s);
    if sanitized.chars().count() <= max {
        return sanitized;
    }
    let head: String = sanitized.chars().take(max).collect();
    format!("{}...", head)
}

/// Sanitize untrusted plan/subprocess text before it reaches a terminal.
///
/// Newline and tab are retained for structurally multiline widgets. All other
/// C0/C1 controls, DEL, bidi formatting controls, and unsafe zero-width
/// formatting characters become visible ASCII escapes or control pictures.
pub fn sanitize_display(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '\n' | '\t' => output.push(character),
            '\u{00}' => output.push('␀'),
            '\u{01}'..='\u{06}'
            | '\u{08}'
            | '\u{0b}'..='\u{0c}'
            | '\u{0e}'..='\u{1a}'
            | '\u{1c}'..='\u{1f}' => {
                output.push_str(&format!("\\u{{{:04X}}}", character as u32));
            }
            '\u{07}' => output.push('␇'),
            '\u{0d}' => output.push('␍'),
            '\u{1b}' => output.push('␛'),
            '\u{7f}'..='\u{9f}'
            | '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}' => {
                output.push_str(&format!("\\u{{{:04X}}}", character as u32));
            }
            _ => output.push(character),
        }
    }
    output
}

/// Single-line variant for table cells, titles, and status/error labels.
pub fn sanitize_display_line(input: &str) -> String {
    sanitize_display(input)
        .replace('\n', "␤")
        .replace('\t', "⇥")
}

/// Recursively sanitize every string in an mp JSON/subprocess payload.
pub fn sanitize_json_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(string) => *string = sanitize_display(string),
        serde_json::Value::Array(values) => {
            for value in values {
                sanitize_json_strings(value);
            }
        }
        serde_json::Value::Object(fields) => {
            for value in fields.values_mut() {
                sanitize_json_strings(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn ascii_truncates_with_ellipsis() {
        assert_eq!(truncate("abcdefghij", 7), "abcdefg...");
    }

    #[test]
    fn multibyte_em_dash_does_not_panic_at_boundary() {
        // Regression: byte-slicing at the cut point panicked because an
        // em-dash (3 bytes) straddled it. Must truncate by chars.
        let title = "raul TUI sidebar shell — tabbed lanes, responsive layout";
        // the em-dash sits at char index 23
        let before = truncate(title, 20); // cut before the dash
        assert!(before.ends_with("..."));
        assert!(!before.contains('—'));
        let across = truncate(title, 30); // cut well past the dash
        assert!(across.ends_with("..."));
        assert!(across.contains('—')); // dash preserved (char 23 < 30)
        assert!(across.chars().count() <= 33); // 30 chars + "..."
    }

    #[test]
    fn multibyte_only_input() {
        // Every byte index 1..=5 is inside a multibyte char here.
        let s = "——"; // 2 chars, 6 bytes
        assert_eq!(truncate(s, 1), "—...");
        assert_eq!(truncate(s, 5), "——");
    }

    #[test]
    fn sanitize_terminal_control_char_sequences_and_bidi() {
        let malicious = concat!(
            "\u{1b}]52;c;Y3JlZGVudGlhbA==\u{07}",
            "\u{1b}]8;;https://evil.invalid\u{07}link\u{1b}]8;;\u{07}",
            "\u{1b}[2J",
            "\rBEL:\u{07}",
            "\u{202e}txt.exe\u{202c}",
            "\u{200b}\u{2066}"
        );
        let sanitized = sanitize_display(malicious);
        assert!(!sanitized.contains('\u{1b}'));
        assert!(!sanitized.contains('\u{07}'));
        assert!(!sanitized.contains('\r'));
        assert!(!sanitized.contains('\u{202e}'));
        assert!(!sanitized.contains('\u{200b}'));
        assert!(sanitized.contains("␛]52"));
        assert!(sanitized.contains("\\u{202E}"));
    }

    #[test]
    fn control_char_single_line_sanitizer_preserves_width_after_sanitizing() {
        let sanitized = sanitize_display_line("safe\nnext\t\u{1b}[31mred");
        assert_eq!(sanitized, "safe␤next⇥␛[31mred");
        assert_eq!(truncate("\u{1b}[31mabcdef", 5), "␛[31m...");
    }
}
