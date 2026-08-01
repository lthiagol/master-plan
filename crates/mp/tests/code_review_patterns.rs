//! M173 S1: runnable Pattern: blocks for code-review lessons L6/L8/L13/L14/L15.
//!
//! Each lesson in `docs-old/code-review-lessons.md` gets a `**Pattern:**` block
//! with three sections: Pattern (description), Positive fixture (must match),
//! Negative fixture (must not match). These tests pin the docs structure and
//! drive the greps that surface matches/violations.

mod common;

use std::path::PathBuf;

/// Resolve the repo root from the test's `CARGO_MANIFEST_DIR`.
fn repo_root() -> PathBuf {
    common::repo_root()
}

/// Resolve `docs-old/code-review-lessons.md` from the repo root.
/// Canonical home after the docs/ restructure; consumer surface treats
/// `docs/code-review-lessons.md` as a dead path.
fn lessons_path() -> PathBuf {
    repo_root().join("docs-old").join("code-review-lessons.md")
}

/// Parse the lessons file into per-lesson sections keyed by lesson id
/// (e.g. "L6", "L8"). Each section's body is the markdown between the
/// `### LX. <title>` heading and the next `### L` or `---` divider.
fn split_lessons(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("### L") {
            // Lesson header: "### L6. <title>" or "### L47. <title>"
            // Extract the numeric id up to the first '.'.
            let id_end = rest.find('.').unwrap_or(rest.len());
            let id = format!("L{}", &rest[..id_end]);
            if let Some((prev_id, prev_lines)) = current.take() {
                out.push((prev_id, prev_lines.join("\n")));
            }
            current = Some((id, vec![line.to_string()]));
        } else if line.starts_with("### ") || line.trim() == "---" {
            if let Some((prev_id, prev_lines)) = current.take() {
                out.push((prev_id, prev_lines.join("\n")));
            }
        } else if let Some((_, ref mut lines)) = current {
            lines.push(line.to_string());
        }
    }
    if let Some((prev_id, prev_lines)) = current.take() {
        out.push((prev_id, prev_lines.join("\n")));
    }
    out
}

/// Pull the `**Pattern:** ... **Pattern end.** block from a lesson body.
/// We use a sentinel line rather than matching on `**Takeaway.**` or
/// `**How to find it.**` so future heading variants don't break parsing.
fn extract_pattern_block(body: &str) -> Option<String> {
    let start = body.find("**Pattern:**")?;
    let after = &body[start..];
    // The pattern block ends at the next lesson-section heading, the
    // divider, or end of body — whichever comes first.
    let mut end = after.len();
    for marker in ["\n### ", "\n---", "\n### L"] {
        if let Some(idx) = after.find(marker) {
            if idx < end {
                end = idx;
            }
        }
    }
    Some(after[..end].to_string())
}

/// Flatten a pattern block (newlines -> spaces) and split on backticks to
/// isolate inline-code command spans scoped to `cargo nextest run ... -p mp`.
/// Wrapping a command across two source lines (common in the lessons prose)
/// still yields a single span after the newline-to-space normalization.
fn cargo_nextest_mp_segments(block: &str) -> Vec<String> {
    let flat = block.replace('\n', " ");
    flat.split('`')
        .map(|s| s.trim().to_string())
        .filter(|s| s.contains("cargo nextest run") && (s.contains("-p mp") || s.contains("-p=mp")))
        .collect()
}

/// Pull every `--test <TARGET>` token from a command segment.
fn test_targets(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut toks = seg.split_whitespace();
    while let Some(t) = toks.next() {
        if t == "--test" {
            if let Some(v) = toks.next() {
                out.push(v.trim_matches(|c| c == '\'' || c == '"').to_string());
            }
        } else if let Some(rest) = t.strip_prefix("--test=") {
            out.push(rest.trim_matches(|c| c == '\'' || c == '"').to_string());
        }
    }
    out
}

/// Pull every `test(/NAME/)` filter from a command segment. Returns the
/// inner NAME strings verbatim (may be a regex like `a|b`).
fn test_filters(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = seg;
    while let Some(s) = rest.find("test(/") {
        let after = &rest[s + "test(/".len()..];
        match after.find("/)") {
            Some(e) => {
                out.push(after[..e].to_string());
                rest = &after[e + 2..];
            }
            None => break,
        }
    }
    out
}

/// Recursively concatenate every `.rs` file under `dir` into one string.
/// Used by the test-name resolution gate (T-07) so a plain-identifier
/// `test(/NAME/)` filter can be checked for any occurrence (`fn NAME`,
/// `mod NAME`, or a `#[path]` include) without invoking cargo.
fn read_tests_tree(dir: &std::path::Path) -> String {
    let mut out = String::new();
    read_tests_tree_into(dir, &mut out);
    out
}

fn read_tests_tree_into(dir: &std::path::Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            read_tests_tree_into(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

/// T-01: each of L6, L8, L13, L14, L15 has a `**Pattern:**` block in the
/// lessons doc. Locks in the M173 S1 deliverable.
#[test]
fn pattern_blocks_present_for_l6_l8_l13_l14_l15() {
    let path = lessons_path();
    assert!(
        path.is_file(),
        "lessons file must exist at {}",
        path.display()
    );
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let lessons = split_lessons(&text);
    let mut missing: Vec<&str> = Vec::new();
    for id in ["L6", "L8", "L13", "L14", "L15"] {
        let body = lessons
            .iter()
            .find(|(k, _)| k == id)
            .unwrap_or_else(|| panic!("lesson {id} missing from {}", path.display()))
            .1
            .clone();
        if extract_pattern_block(&body).is_none() {
            missing.push(id);
        }
    }
    assert!(
        missing.is_empty(),
        "missing **Pattern:** blocks for: {}",
        missing.join(", ")
    );
}

/// T-02: each `**Pattern:**` block has the three required sub-sections —
/// "Pattern.", "Positive fixture.", "Negative fixture.". Locks in the
/// structural shape so future maintainers don't drift into prose.
#[test]
fn pattern_blocks_have_pattern_positive_negative_sections() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let lessons = split_lessons(&text);
    for id in ["L6", "L8", "L13", "L14", "L15"] {
        let body = &lessons
            .iter()
            .find(|(k, _)| k == id)
            .unwrap_or_else(|| panic!("lesson {id} missing"))
            .1;
        let block = extract_pattern_block(body)
            .unwrap_or_else(|| panic!("lesson {id} missing **Pattern:** block"));
        for required in [
            "**Pattern.**",
            "**Positive fixture.**",
            "**Negative fixture.**",
        ] {
            assert!(
                block.contains(required),
                "lesson {id}: pattern block missing required sub-section '{required}'\nblock:\n{block}"
            );
        }
    }
}

/// T-03: at least 5 lessons carry a `**Pattern:**` block. Matches the
/// M173 AC-01 contract: "≥5 lessons with Pattern: blocks".
#[test]
fn at_least_five_pattern_blocks_present() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let count = text.matches("**Pattern:**").count();
    assert!(count >= 5, "expected ≥5 **Pattern:** blocks, found {count}");
}

/// T-04: each pattern block contains at least one runnable command. We
/// accept either a fenced `\`\`\`` shell block or an inline `cargo ` /
/// `rg ` / `make ` / `git ` / `bash ` token (with or without surrounding
/// backticks). This is the runnable-fixture contract — the blocks must
/// drive greps/tests, not just prose.
#[test]
fn pattern_blocks_contain_runnable_commands() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let lessons = split_lessons(&text);
    for id in ["L6", "L8", "L13", "L14", "L15"] {
        let body = &lessons
            .iter()
            .find(|(k, _)| k == id)
            .unwrap_or_else(|| panic!("lesson {id} missing"))
            .1;
        let block = extract_pattern_block(body)
            .unwrap_or_else(|| panic!("lesson {id} missing **Pattern:** block"));
        let has_fenced_shell = block.contains("```bash") || block.contains("```sh");
        let has_inline_command = block.lines().any(|l| {
            let t = l.trim_start();
            // Accept either bare command at line start, or one wrapped in
            // backticks (the lesson prose uses backticks for inline).
            let stripped = t.trim_matches('`').trim();
            stripped.starts_with("cargo ")
                || stripped.starts_with("rg ")
                || stripped.starts_with("make ")
                || stripped.starts_with("git ")
                || stripped.starts_with("bash ")
        });
        assert!(
            has_fenced_shell || has_inline_command,
            "lesson {id}: pattern block has no runnable command\nblock:\n{block}"
        );
    }
}

/// T-05: the file's lesson count (sections starting with `### L`) is ≥60,
/// i.e. the existing lessons catalog is intact (no truncation when the
/// new pattern blocks were added).
#[test]
fn lesson_count_not_truncated() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let count = text
        .lines()
        .filter(|l| l.starts_with("### L") && l.chars().nth(5).is_some_and(|c| c.is_ascii_digit()))
        .count();
    assert!(
        count >= 60,
        "expected ≥60 lessons, found {count} — pattern block edits may have truncated the catalog"
    );
}

/// T-06: every `--test <TARGET>` cited by a `-p mp` cargo-nextest command
/// in a Pattern block must resolve to a real test target file
/// `crates/mp/tests/<TARGET>.rs`. Catches the M173 F-16 regression where
/// L8/L14/L15 cited `--test milestone_bulk` (not a target; the real target
/// is `suite_milestone`, with `milestone_bulk` a `#[path]`-included module).
#[test]
fn pattern_block_cited_test_targets_resolve() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let lessons = split_lessons(&text);
    let tests_dir = repo_root().join("crates/mp/tests");
    let mut bad: Vec<String> = Vec::new();
    for id in ["L6", "L8", "L13", "L14", "L15"] {
        let body = &lessons
            .iter()
            .find(|(k, _)| k == id)
            .unwrap_or_else(|| panic!("lesson {id} missing"))
            .1;
        let block = extract_pattern_block(body)
            .unwrap_or_else(|| panic!("lesson {id} missing **Pattern:** block"));
        for seg in cargo_nextest_mp_segments(&block) {
            for target in test_targets(&seg) {
                let target_file = tests_dir.join(format!("{target}.rs"));
                if !target_file.is_file() {
                    bad.push(format!(
                        "{id}: --test {target} resolves to no file ({})",
                        target_file.display()
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "Pattern blocks cite non-resolving --test targets:\n{}",
        bad.join("\n")
    );
}

/// T-07: every `test(/NAME/)` filter whose NAME is a plain identifier
/// (no regex metacharacters) cited by a `-p mp` cargo-nextest command in a
/// Pattern block must appear as a substring somewhere under
/// `crates/mp/tests/` — a `fn NAME` test, a `mod NAME` module, or a
/// `#[path = "...NAME..."]` include. Catches the M173 F-16 regression
/// where L15 cited nonexistent test names (e.g.
/// `bulk_set_spec_status_runs_gate`). Regex filters (containing `|`, `(`,
/// etc.) are skipped — only plain identifiers are validated.
#[test]
fn pattern_block_cited_test_names_exist() {
    let path = lessons_path();
    let text = std::fs::read_to_string(&path).unwrap();
    let lessons = split_lessons(&text);
    let blob = read_tests_tree(&repo_root().join("crates/mp/tests"));
    let mut bad: Vec<String> = Vec::new();
    for id in ["L6", "L8", "L13", "L14", "L15"] {
        let body = &lessons
            .iter()
            .find(|(k, _)| k == id)
            .unwrap_or_else(|| panic!("lesson {id} missing"))
            .1;
        let block = extract_pattern_block(body)
            .unwrap_or_else(|| panic!("lesson {id} missing **Pattern:** block"));
        for seg in cargo_nextest_mp_segments(&block) {
            for name in test_filters(&seg) {
                // Only validate plain identifiers; skip regex filters.
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                if !blob.contains(&name) {
                    bad.push(format!(
                        "{id}: test(/{name}/) names nothing under crates/mp/tests/"
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "Pattern blocks cite test names that resolve to nothing under crates/mp/tests/:\n{}",
        bad.join("\n")
    );
}
