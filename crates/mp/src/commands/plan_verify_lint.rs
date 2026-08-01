//! M110 (S2): soft lint for broad-scope `verification` / `tests` strings.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::model::MilestoneFile;
use crate::paths::PlanContext;

#[derive(Debug, Clone, Serialize)]
pub struct VerifyLintWarning {
    pub code: String,
    pub milestone_file: String,
    pub line: Option<usize>,
    pub field: String,
    pub pattern: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyLintReport {
    pub ok: bool,
    pub warning_count: usize,
    pub warnings: Vec<VerifyLintWarning>,
}

struct LintPatterns {
    workspace_re: Regex,
    all_re: Regex,
    make_test_re: Regex,
    cargo_p_re: Regex,
    wc_l_re: Regex,
    grep_l_re: Regex,
}

fn patterns() -> &'static LintPatterns {
    static PATTERNS: OnceLock<LintPatterns> = OnceLock::new();
    PATTERNS.get_or_init(|| LintPatterns {
        workspace_re: Regex::new(r"cargo[[:space:]]+test[[:space:]]+--workspace")
            .expect("workspace re"),
        all_re: Regex::new(r"cargo[[:space:]]+test[[:space:]]+--all").expect("all re"),
        make_test_re: Regex::new(r"make[[:space:]]+test([[:space:]]|&|\||;|$)")
            .expect("make test re"),
        cargo_p_re: Regex::new(r"-p[[:space:]]+([A-Za-z0-9_-]+)").expect("cargo -p re"),
        wc_l_re: Regex::new(r"\|[[:space:]]*wc[[:space:]]+-l\)?").expect("wc -l re"),
        grep_l_re: Regex::new(r"\|[[:space:]]*grep[[:space:]]+-l").expect("grep -l re"),
    })
}

pub fn verify_lint(ctx: &PlanContext) -> Result<VerifyLintReport> {
    let dir = ctx.milestones_dir();
    let mut warnings = Vec::new();
    let p = patterns();

    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "verify-lint: WARNING: could not read {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let json: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "verify-lint: WARNING: malformed JSON in {}; skipping ({e})",
                    path.display()
                );
                continue;
            }
        };

        let milestone: MilestoneFile = match serde_json::from_value(json.clone()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "verify-lint: WARNING: milestone schema mismatch in {}; skipping ({e})",
                    path.display()
                );
                continue;
            }
        };

        // Per-milestone crate set is approximate: derived only from
        // `steps[].files[]`. Milestones without steps/files skip `-p` checks
        // but still get global broad-scope and portability patterns.
        let affected = affected_crates(&milestone);
        let basename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut command_strings = Vec::new();
        collect_command_strings_from_value(&json, &mut command_strings);

        for (field, value) in command_strings {
            if let Some(pattern) = first_broad_scope_match(&value, p) {
                warnings.push(warning_row(&basename, &raw, &field, pattern, &value));
            }
            if let Some(pattern) = first_portability_match(&value, p) {
                warnings.push(warning_row(&basename, &raw, &field, pattern, &value));
            }
            if let Some(pattern) = crate_scope_match(&value, &affected, p) {
                warnings.push(warning_row(&basename, &raw, &field, pattern, &value));
            }
        }
    }

    let warning_count = warnings.len();
    Ok(VerifyLintReport {
        ok: true,
        warning_count,
        warnings,
    })
}

fn warning_row(
    basename: &str,
    raw: &str,
    field: &str,
    pattern: String,
    value: &str,
) -> VerifyLintWarning {
    VerifyLintWarning {
        code: "W-VERIFY-LINT".to_string(),
        milestone_file: basename.to_string(),
        line: find_line(raw, value),
        field: field.to_string(),
        pattern,
        value: value.to_string(),
    }
}

/// First matching broad-scope pattern only (matches legacy shell script behavior).
fn first_broad_scope_match(value: &str, p: &LintPatterns) -> Option<String> {
    if p.workspace_re.is_match(value) {
        return Some(p.workspace_re.as_str().to_string());
    }
    if p.make_test_re.is_match(value) {
        return Some(p.make_test_re.as_str().to_string());
    }
    if p.all_re.is_match(value) {
        return Some(p.all_re.as_str().to_string());
    }
    None
}

/// First matching portability pattern only.
fn first_portability_match(value: &str, p: &LintPatterns) -> Option<String> {
    if wc_l_without_xargs(value, &p.wc_l_re) {
        return Some("| wc -l without | xargs".to_string());
    }
    if grep_l_without_true_guard(value, &p.grep_l_re) {
        return Some("| grep -l without || true".to_string());
    }
    if raw_jq_without_pipe(value) {
        return Some("raw jq .field without newline-tolerant pipe".to_string());
    }
    None
}

fn wc_l_without_xargs(value: &str, re: &Regex) -> bool {
    let Some(m) = re.find(value) else {
        return false;
    };
    !value[m.start()..].contains("| xargs")
}

fn grep_l_without_true_guard(value: &str, re: &Regex) -> bool {
    let Some(m) = re.find(value) else {
        return false;
    };
    !value[m.start()..].contains("|| true")
}

/// Detect a raw `jq` invocation (no upstream pipe) that emits a number which
/// macOS `sh` test builtins compare with leading whitespace. ER-2 (M110
/// review): the original `contains("jq .")` substring check was retained after
/// a rewrite attempt — a word-boundary regex to reject quoted prose like
/// `echo "run jq .filter"` turned out to miss the common `jq ".foo"` and
/// `jq -r ".x"` quoted-filter forms, introducing more gaps than it closed.
/// Reverted to the substring form: it correctly catches the M105 brittleness
/// (`jq .field`), and a scan of this repo's plan found zero real verification
/// strings it would mis-flag. The theoretical false-positive surface (jq
/// mentioned inside a quoted echo) does not occur in practice.
fn raw_jq_without_pipe(value: &str) -> bool {
    let needs_pipe =
        value.contains("jq .") || value.contains("jq -r .") || value.contains("jq -e .");
    needs_pipe && !value.contains("| jq")
}

fn crate_scope_match(value: &str, affected: &HashSet<String>, p: &LintPatterns) -> Option<String> {
    if affected.is_empty() {
        return None;
    }
    let mentioned: BTreeSet<String> = p
        .cargo_p_re
        .captures_iter(value)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    if mentioned.len() > 1 {
        let broader: Vec<_> = mentioned
            .iter()
            .filter(|c| !affected.contains(c.as_str()))
            .collect();
        if !broader.is_empty() {
            return Some(format!(
                "multi-crate -p (affected={affected:?}, mentioned={mentioned:?})"
            ));
        }
    } else if mentioned.len() == 1 {
        let only = mentioned.iter().next().unwrap();
        if !affected.contains(only.as_str()) {
            return Some(format!("crate -p {only} not in affected set {affected:?}"));
        }
    }
    None
}

pub fn print_human_warnings(report: &VerifyLintReport) {
    if report.warnings.is_empty() {
        return;
    }

    let mut last_file = String::new();
    let mut milestone_hits = HashSet::new();

    // Human WARN lines go to stderr so stdout stays JSON-clean for agents.
    for w in &report.warnings {
        milestone_hits.insert(w.milestone_file.clone());
        if w.milestone_file != last_file {
            eprintln!("WARN: {}", w.milestone_file);
            last_file = w.milestone_file.clone();
        }
        if let Some(line) = w.line {
            eprintln!("    line {line}  pattern={}", w.pattern);
        } else {
            eprintln!("    pattern={}", w.pattern);
        }
        eprintln!("    cmd  : {}", w.value);
    }

    eprintln!();
    eprintln!(
        "verify-lint: {} milestone(s) with broad-scope verifications. WARN-only.",
        milestone_hits.len()
    );
}

/// Derive the set of workspace crates a milestone touches from its
/// `steps[].files[]` paths. ER-4 (M110 review): the original implementation
/// hard-coded `crates/mp/` and `crates/raul/`, which would silently ignore any
/// future workspace crate (`crates/mp-model/`, `crates/mp-store/`, …). The
/// generalized form maps any `crates/<name>/` segment to `<name>` so adding a
/// crate never requires touching the lint. Note: this does not by itself
/// silence warnings for milestones that run `cargo test -p <crate>` without
/// declaring the corresponding `crates/<crate>/` files in `steps[].files[]`
/// — those are real spec-hygiene findings the lint correctly surfaces.
fn affected_crates(m: &MilestoneFile) -> HashSet<String> {
    let mut crates = HashSet::new();
    for step in &m.steps {
        for file in &step.files {
            for name in crate_names_in_path(file) {
                crates.insert(name);
            }
        }
    }
    crates
}

/// Extract every `crates/<name>/` segment from a path string.
/// `crates/mp/src/lib.rs` → `mp`; `crates/mp-model/src/lib.rs` → `mp-model`.
/// Works on forward slashes regardless of platform (plan paths are POSIX).
fn crate_names_in_path(file: &str) -> Vec<String> {
    let mut out = Vec::new();
    let parts: Vec<&str> = file.split('/').collect();
    let mut i = 0;
    while i + 1 < parts.len() {
        if parts[i] == "crates" {
            let name = parts[i + 1];
            if !name.is_empty() && name != "crates" {
                out.push(name.to_string());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// Walk the milestone JSON tree (jq-equivalent) for every `verification` / `tests` string.
fn collect_command_strings_from_value(value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for key in ["verification", "tests"] {
                if let Some(Value::String(s)) = map.get(key) {
                    if !s.is_empty() {
                        out.push((key.to_string(), s.clone()));
                    }
                }
            }
            for v in map.values() {
                collect_command_strings_from_value(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_command_strings_from_value(v, out);
            }
        }
        _ => {}
    }
}

fn find_line(raw: &str, needle: &str) -> Option<usize> {
    raw.lines()
        .enumerate()
        .find(|(_, line)| line.contains(needle))
        .map(|(i, _)| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affected_crates_from_step_files() {
        let json = r#"{
            "milestone": {"id":"1","title":"t","slug":"s","spec_status":"ready","execution_status":"planned","depends_on":[],"effort":"S","risk":"low","created":"2026-01-01","updated":"2026-01-01"},
            "intent": {"outcome":"o"},
            "problem": {"description":"p"},
            "scope": {"in_scope":["a"],"out_of_scope":["b","c"]},
            "acceptance_criteria": [],
            "steps": [{"id":"S1","action":"a","status":"pending","tests":"t","done_when":"d","files":["crates/mp/src/foo.rs"],"covers_ac":[],"depends_on_steps":[],"order":1,"work_package":"WP1"}]
        }"#;
        let m: MilestoneFile = serde_json::from_str(json).unwrap();
        let crates = affected_crates(&m);
        assert_eq!(crates, HashSet::from(["mp".to_string()]));
    }

    /// ER-4 (M110 review): crate derivation must generalize beyond the
    /// originally-hard-coded `crates/mp/` and `crates/raul/`. A milestone
    /// touching `crates/mp-model/` (or any future workspace crate) must
    /// resolve without code changes to the linter.
    #[test]
    fn affected_crates_generalizes_to_arbitrary_workspace_crates() {
        let json = r#"{
            "milestone": {"id":"1","title":"t","slug":"s","spec_status":"ready","execution_status":"planned","depends_on":[],"effort":"S","risk":"low","created":"2026-01-01","updated":"2026-01-01"},
            "intent": {"outcome":"o"},
            "problem": {"description":"p"},
            "scope": {"in_scope":["a"],"out_of_scope":["b","c"]},
            "acceptance_criteria": [],
            "steps": [{"id":"S1","action":"a","status":"pending","tests":"cargo test -p mp-model","done_when":"d","files":["crates/mp-model/src/lib.rs","crates/mp/src/model.rs"],"covers_ac":[],"depends_on_steps":[],"order":1,"work_package":"WP1"}]
        }"#;
        let m: MilestoneFile = serde_json::from_str(json).unwrap();
        let crates = affected_crates(&m);
        assert_eq!(
            crates,
            HashSet::from(["mp".to_string(), "mp-model".to_string()]),
            "generalized derivation must resolve mp-model (and any crates/<name>/) without hard-coding"
        );
    }

    #[test]
    fn recursive_collect_finds_nested_verification() {
        let json: Value = serde_json::from_str(
            r#"{"acceptance_criteria":[{"verification":"cargo test --workspace"}],"nested":{"tests":"make test"}}"#,
        )
        .unwrap();
        let mut out = Vec::new();
        collect_command_strings_from_value(&json, &mut out);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn first_broad_scope_match_stops_at_first_pattern() {
        let p = patterns();
        let value = "make test && cargo test --workspace";
        let m = first_broad_scope_match(value, p).unwrap();
        assert!(
            m.contains("--workspace"),
            "workspace pattern is first in priority (shell script order)"
        );
    }

    #[test]
    fn grep_l_true_guard_only_after_match() {
        let p = patterns();
        let ok = "|| true && mp list | grep -l foo";
        assert!(first_portability_match(ok, p).is_some());
        let guarded = "mp list | grep -l foo || true";
        assert!(first_portability_match(guarded, p).is_none());
    }
}
