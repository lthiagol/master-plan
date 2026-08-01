/// M116 S2 — value parser for `--files` (StepCmd::Add / StepCmd::Update).
/// Accepts a bare path (`crates/mp/src/main.rs`) or a comma-separated
/// list (`a.rs,b.rs`). Rejects anything that looks like a JSON literal
/// (array or object) so the user gets a clear error instead of having
/// a literal `["a.rs"]` or `{"a.rs"}` ending up persisted in
/// `step.files`. Returns the trimmed input on success; bails with a
/// structured error otherwise. M116 CR: broadened the check from
/// `starts_with('[') && ends_with(']')` to also cover unclosed arrays,
/// trailing-bracket bare paths, and object literals — the original
/// check let `[a.rs`, `a.rs]`, and `{"a.rs"}` slip through and corrupt
/// `step.files` (dogfood log entry 30).
pub(crate) fn files_value_parser(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("--files cannot be empty".to_string());
    }
    // Reject anything that smells like a JSON literal. `[` is the
    // array marker; `{` is the object marker. Both signal "the user
    // typed a JSON value" and we should never store a JSON literal in
    // `step.files` — it must be a list of bare paths. We try to parse
    // the trimmed value to give a precise error: a valid JSON literal
    // means the user almost certainly meant a JSON array (use
    // comma-separated instead); a malformed one is still a JSON
    // literal attempt (malformed input is not silently coerced to a
    // bare path).
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(_) => Err(format!(
                "--files received '{trimmed}' which looks like a JSON literal. \
                 Pass bare paths or comma-separated values instead, e.g. \
                 `--files crates/mp/src/main.rs` or `--files a.rs,b.rs`"
            )),
            Err(e) => Err(format!(
                "--files received '{trimmed}' which is not a valid bare path or \
                 comma-separated list (JSON parse error: {e}). Pass bare paths \
                 or comma-separated values, e.g. \
                 `--files crates/mp/src/main.rs` or `--files a.rs,b.rs`"
            )),
        };
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod files_value_parser_tests {
    use super::files_value_parser;

    fn ok(s: &str) {
        let parsed = files_value_parser(s).unwrap_or_else(|e| {
            panic!("expected '{s}' to be accepted, got error: {e}");
        });
        assert_eq!(
            parsed,
            s.trim(),
            "returned value should be the trimmed input"
        );
    }

    fn err(s: &str, needle: &str) {
        let e = files_value_parser(s)
            .err()
            .unwrap_or_else(|| panic!("expected '{s}' to be rejected, got ok"));
        assert!(
            e.contains(needle),
            "error message for '{s}' should mention {needle:?}, got: {e}"
        );
    }

    #[test]
    fn accepts_bare_path() {
        ok("crates/mp/src/main.rs");
    }

    #[test]
    fn accepts_comma_separated_list() {
        ok("a.rs,b.rs,c.rs");
    }

    #[test]
    fn accepts_path_with_trailing_bracket() {
        // Trailing `]` is a legitimate bare path (rare but legal).
        ok("a.rs]");
    }

    #[test]
    fn accepts_whitespace_around_input() {
        ok("  a.rs  ");
        let parsed = files_value_parser("  a.rs  ").unwrap();
        assert_eq!(
            parsed, "a.rs",
            "leading/trailing whitespace should be trimmed"
        );
    }

    #[test]
    fn rejects_empty() {
        err("", "cannot be empty");
        err("   ", "cannot be empty");
    }

    #[test]
    fn rejects_quoted_json_array() {
        err("[\"a.rs\"]", "JSON literal");
    }

    #[test]
    fn rejects_unquoted_json_array() {
        err("[\"a.rs\",\"b.rs\"]", "JSON literal");
    }

    #[test]
    fn rejects_empty_json_array() {
        err("[]", "JSON literal");
    }

    #[test]
    fn rejects_unclosed_array() {
        // The M116-shipped parser accepted this and corrupted step.files
        // (dogfood log entry 30 — found during external review).
        err("[a.rs", "JSON parse error");
    }

    #[test]
    fn rejects_unclosed_array_with_comma() {
        err("[a.rs,b.rs", "JSON parse error");
    }

    #[test]
    fn rejects_object_literal_attempt() {
        // `{"a.rs"}` is not valid JSON (objects need `key: value`),
        // so the parser surfaces the JSON parse error rather than the
        // generic "looks like a JSON literal" rejection.
        err("{\"a.rs\"}", "JSON parse error");
    }

    #[test]
    fn rejects_malformed_object() {
        err("{\"a.rs\":", "JSON parse error");
    }

    #[test]
    fn rejects_whitespace_padded_json_array() {
        err("  [\"a.rs\"]  ", "JSON literal");
    }

    #[test]
    fn error_suggests_comma_separated() {
        let e = files_value_parser("[\"a.rs\"]").unwrap_err();
        assert!(
            e.contains("--files a.rs,b.rs"),
            "error should suggest comma-separated form, got: {e}"
        );
    }
}
