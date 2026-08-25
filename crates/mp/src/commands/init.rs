use anyhow::{Context, Result};
use serde_json::json;

use crate::bootstrap;
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::install;
use crate::paths::PlanContext;
use crate::store;

pub(crate) struct InitOptions<'a> {
    pub ctx: &'a PlanContext,
    pub profile: Option<&'a str>,
    pub from_repo: bool,
    pub force: bool,
    /// M194: append the root-AGENTS snippet to an existing
    /// root `AGENTS.md` instead of warning.
    pub merge_root_agents: bool,
    pub with_cursor_skill: bool,
    pub with_opencode_skill: bool,
    pub skip_root_agents: bool,
    /// M194: rewrite `master-plan/AGENTS.md` from the current
    /// binary's embedded template. Scope is AGENTS.md only
    /// (Q-02 resolution: config.json / plan.json drift is a
    /// separate doctor check).
    pub refresh: bool,
    /// M194: skip the confirmation prompt for `--refresh`.
    pub yes: bool,
    pub format: Fmt,
}

pub(crate) fn cmd_init(opts: InitOptions) -> Result<()> {
    let txn = crate::plan_io::PlanWriteTxn::acquire_project_root(&opts.ctx.project_root)?;
    txn.run(|_| cmd_init_inner(opts))
}

fn cmd_init_inner(opts: InitOptions) -> Result<()> {
    // M194: `--refresh` short-circuits the rest of init. It
    // operates on `master-plan/AGENTS.md` only and never touches
    // `config.json` / `plan.json` (Q-02). The plan dir must
    // exist; otherwise the refresh is a no-op error rather than
    // a silent init.
    if opts.refresh {
        return cmd_init_refresh(&opts);
    }

    let created = store::init_plan(opts.ctx, opts.profile, opts.force)?;
    let mut payload = json!({
        "ok": true,
        "plan_dir": opts.ctx.plan_dir,
        "profile": opts.profile.unwrap_or("full"),
        "created": created,
    });
    if opts.from_repo {
        let bootstrap_report = bootstrap::apply_from_repo(opts.ctx, opts.profile)?;
        payload["bootstrap"] = serde_json::to_value(bootstrap_report)?;
    }
    let mut skills_installed = Vec::new();
    if opts.with_cursor_skill {
        let paths = install::install_project_skill(
            &opts.ctx.project_root,
            install::ProjectSkillHarness::Cursor,
        )?;
        for path in paths {
            skills_installed.push(json!({ "harness": "cursor", "path": path }));
        }
    }
    if opts.with_opencode_skill {
        let paths = install::install_project_skill(
            &opts.ctx.project_root,
            install::ProjectSkillHarness::Opencode,
        )?;
        for path in paths {
            skills_installed.push(json!({ "harness": "opencode", "path": path }));
        }
    }
    if !skills_installed.is_empty() {
        payload["skills_installed"] = json!(skills_installed);
    }
    if !opts.skip_root_agents {
        let snippet = crate::assets::read_embedded("templates/ROOT-AGENTS-SNIPPET.md")?;
        let root_path = opts.ctx.project_root.join("AGENTS.md");
        let root_agents_status =
            write_root_agents(&root_path, &snippet, opts.force, opts.merge_root_agents)?;
        payload["root_agents"] = serde_json::to_value(&root_agents_status)?;
    }
    emit(opts.format, &payload)
}

/// M194: re-write `master-plan/AGENTS.md` from the current
/// binary's embedded template. Refuses to operate on a project
/// that hasn't been `mp init`'d yet (the plan dir must exist).
/// Honors a confirmation prompt unless `--yes` is passed.
fn cmd_init_refresh(opts: &InitOptions) -> Result<()> {
    let plan_dir = &opts.ctx.plan_dir;
    if !plan_dir.is_dir() {
        anyhow::bail!(
            "mp init --refresh requires an existing plan directory at {plan_dir:?}; \
             run `mp init` first."
        );
    }
    let template = crate::assets::read_embedded("templates/AGENTS-TEMPLATE.md")?;
    let target = plan_dir.join("AGENTS.md");
    let prior = if target.exists() {
        Some(
            std::fs::read_to_string(&target)
                .with_context(|| format!("read prior {}", target.display()))?,
        )
    } else {
        None
    };
    // Confirmation: by default, show a brief diff stat and
    // ask the user to confirm. `--yes` skips the prompt for
    // scripted use. The prompt is bypassed when stdin is
    // not a TTY (CI, automation); the caller is expected to
    // pass `--yes` for explicit non-interactive rewrites.
    let proceed = if opts.yes {
        true
    } else if !atty_stdin() {
        // Non-interactive stdin (CI, pipe) — require --yes.
        eprintln!(
            "mp init --refresh: stdin is not a TTY; pass --yes to rewrite {} non-interactively.",
            target.display()
        );
        false
    } else {
        let input = read_stdin_line()?;
        prompt_confirm_refresh(&target, prior.as_deref(), &template, &input)?
    };
    if !proceed {
        // F-03 (external review): cancellation is a non-zero
        // exit. Scripts that check `if mp init --refresh; then`
        // must NOT see a false positive — the file was not
        // rewritten. We emit the JSON payload first so the
        // caller (or raul) can still read the cancellation
        // reason, then return an error so the process exit
        // code reflects the outcome.
        let payload = json!({
            "ok": false,
            "refresh": "cancelled",
            "target": target.to_string_lossy(),
        });
        emit(opts.format, &payload)?;
        anyhow::bail!(
            "mp init --refresh cancelled; {} was not rewritten",
            target.display()
        );
    }
    std::fs::write(&target, &template).with_context(|| format!("write {}", target.display()))?;
    let payload = json!({
        "ok": true,
        "refresh": "rewritten",
        "target": target.to_string_lossy(),
        "bytes": template.len(),
    });
    emit(opts.format, &payload)
}

/// M194: confirmation prompt for `mp init --refresh`. Shows a
/// brief diff stat (lines added / removed) and asks the user
/// to confirm. Returns `Ok(true)` on yes, `Ok(false)` on no.
///
/// F-06 (external review): the input source is a parameter
/// (`input`) so the function is unit-testable without faking
/// stdin. The caller in `cmd_init_refresh` passes
/// `read_stdin_line` (which reads one line from real stdin)
/// when running interactively.
fn prompt_confirm_refresh(
    target: &std::path::Path,
    prior: Option<&str>,
    template: &str,
    input: &str,
) -> Result<bool> {
    use std::io::Write;
    let (added, removed) = match prior {
        Some(p) => diff_unique_line_counts(p, template),
        None => (template.lines().count(), 0),
    };
    eprintln!(
        "mp init --refresh: rewrite {} (+{} / -{} lines, {} bytes)",
        target.display(),
        added,
        removed,
        template.len(),
    );
    eprint!("Proceed? [y/N] ");
    std::io::stderr().flush().ok();
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// M194: read one line from real stdin. Used by
/// `cmd_init_refresh` when a confirmation prompt is needed
/// (TTY, no `--yes`). Split out from `prompt_confirm_refresh`
/// so the prompt function can be unit-tested without faking
/// stdin (F-06).
fn read_stdin_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read confirmation from stdin")?;
    Ok(line)
}

/// M194: cheap diff stat — counts *unique* lines in `after`
/// that are not in `before` (added) and *unique* lines in
/// `before` that are not in `after` (removed). Approximate
/// (set-based, so duplicate lines are counted once) but
/// enough for a "Proceed? [y/N]" prompt.
///
/// F-04 (external review): renamed from `diff_line_counts` to
/// `diff_unique_line_counts` so the set-based limitation is
/// visible at the call site. A user reading "(+2 / -1 lines)"
/// for a real 100-line change (where many lines are duplicates)
/// would otherwise be misled — the function only counts the
/// *unique* lines.
fn diff_unique_line_counts(before: &str, after: &str) -> (usize, usize) {
    use std::collections::HashSet;
    let before: HashSet<&str> = before.lines().collect();
    let after: HashSet<&str> = after.lines().collect();
    let added = after.difference(&before).count();
    let removed = before.difference(&after).count();
    (added, removed)
}

/// M194: returns `true` if stdin is a TTY. Used to decide
/// whether to show the `--refresh` confirmation prompt; if
/// stdin is a pipe (CI, automation) the user must opt in via
/// `--yes` to avoid hanging.
fn atty_stdin() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// M194: write the root `AGENTS.md` snippet to disk, handling the
/// already-exists case explicitly so the user never gets a silent
/// "init succeeded but the snippet didn't land" surprise.
///
/// Modes (clap rejects `--force --merge-root-agents` at parse time,
/// so this function only ever sees one of them or neither):
/// - **No file**: write the snippet (default install behavior).
/// - **File exists + `--force`**: overwrite with the snippet
///   (the existing `--force` flag was originally for the plan
///   directory; M194 extends it to the root AGENTS.md too so
///   the destructive intent is one flag).
/// - **File exists + `--merge-root-agents`**: append the snippet
///   to the existing file with a separator comment so the
///   boundary is visible to humans and agents.
/// - **File exists + no opt-in flag**: print a warning to
///   stderr, return `RootAgentsStatus::Skipped` with the
///   snippet body in the payload so the caller can render
///   the instructions in their preferred output format (the
///   JSON output path embeds the snippet; the human output
///   path falls through to the same JSON's `snippet` field).
fn write_root_agents(
    root_path: &std::path::Path,
    snippet: &str,
    force: bool,
    merge: bool,
) -> Result<RootAgentsStatus> {
    if !root_path.exists() {
        std::fs::write(root_path, snippet)
            .with_context(|| format!("write {}", root_path.display()))?;
        return Ok(RootAgentsStatus {
            action: "created".to_string(),
            path: root_path.to_string_lossy().to_string(),
            snippet: None,
        });
    }
    // File already exists. `--force` and `--merge-root-agents`
    // are mutually exclusive at the clap level, so we only
    // check one of them per branch.
    if force {
        std::fs::write(root_path, snippet)
            .with_context(|| format!("overwrite {}", root_path.display()))?;
        return Ok(RootAgentsStatus {
            action: "overwritten".to_string(),
            path: root_path.to_string_lossy().to_string(),
            snippet: None,
        });
    }
    if merge {
        // Append with a visible separator so the boundary
        // is unambiguous in the merged file.
        let separator = "\n\n<!-- master-plan: appended by `mp init --merge` on next run; safe to keep or move -->\n\n";
        let mut current = std::fs::read_to_string(root_path)
            .with_context(|| format!("read {}", root_path.display()))?;
        current.push_str(separator);
        current.push_str(snippet);
        std::fs::write(root_path, current)
            .with_context(|| format!("append {}", root_path.display()))?;
        return Ok(RootAgentsStatus {
            action: "merged".to_string(),
            path: root_path.to_string_lossy().to_string(),
            snippet: None,
        });
    }
    // Default: explicit warn + print the snippet so the user
    // can do a manual merge. The `snippet` field in the JSON
    // payload carries the body; the human output writes the
    // warning + snippet to stderr.
    eprintln!(
        "warning: {} already exists; mp init did not insert the master-plan snippet. \
         Re-run with --force to overwrite or --merge to append. Snippet follows:",
        root_path.display()
    );
    eprintln!("--- begin snippet ---\n{snippet}\n--- end snippet ---");
    Ok(RootAgentsStatus {
        action: "skipped".to_string(),
        path: root_path.to_string_lossy().to_string(),
        snippet: Some(snippet.to_string()),
    })
}

/// M194: serializable status for the root-AGENTS write path so
/// the JSON output includes the action and (when skipped) the
/// snippet body, and tests can assert on the exact outcome.
#[derive(Debug, serde::Serialize)]
struct RootAgentsStatus {
    action: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    snippet: Option<String>,
}

// M194 self-review: unit tests for the small helpers in
// this module that the integration tests don't cover
// directly. Keeps the diff_stat + TTY-detection contracts
// honest if the prompt path is later refactored.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_unique_line_counts_added_and_removed_lines() {
        let before = "a\nb\nc\n";
        let after = "a\nB\nc\nd\n";
        let (added, removed) = diff_unique_line_counts(before, after);
        assert_eq!(added, 2, "added should count 'B' and 'd'");
        assert_eq!(removed, 1, "removed should count 'b'");
    }

    #[test]
    fn diff_unique_line_counts_no_change() {
        let s = "a\nb\nc\n";
        let (added, removed) = diff_unique_line_counts(s, s);
        assert_eq!(added, 0);
        assert_eq!(removed, 0);
    }

    #[test]
    fn diff_unique_line_counts_handles_empty_strings() {
        let (added, removed) = diff_unique_line_counts("", "a\nb\n");
        assert_eq!(added, 2);
        assert_eq!(removed, 0);
    }

    #[test]
    fn diff_unique_line_counts_counts_unique_lines() {
        // Repeated lines are still one set member; the
        // function is set-based so it undercounts duplicate
        // added/removed lines. The test pins the current
        // approximate behavior.
        let before = "x\n";
        let after = "x\nx\nx\n";
        let (added, removed) = diff_unique_line_counts(before, after);
        assert_eq!(added, 0, "duplicate 'x' is in both sets");
        assert_eq!(removed, 0);
    }

    // F-06 (external review): the prompt path is now
    // unit-testable. We assert both the "yes" and "no" /
    // empty-input cases without faking stdin.
    #[test]
    fn prompt_confirm_refresh_accepts_lowercase_y() {
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let prior = Some("a\nb\nc\n");
        let template = "a\nB\nc\nd\n";
        assert!(
            prompt_confirm_refresh(target, prior, template, "y\n").unwrap(),
            "lowercase y must be accepted"
        );
    }

    #[test]
    fn prompt_confirm_refresh_accepts_uppercase_y() {
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let prior = Some("a\nb\nc\n");
        let template = "a\nB\nc\nd\n";
        assert!(
            prompt_confirm_refresh(target, prior, template, "Y\n").unwrap(),
            "uppercase Y must be accepted"
        );
    }

    #[test]
    fn prompt_confirm_refresh_rejects_n() {
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let prior = Some("a\nb\nc\n");
        let template = "a\nB\nc\nd\n";
        assert!(
            !prompt_confirm_refresh(target, prior, template, "n\n").unwrap(),
            "n must NOT be accepted"
        );
    }

    #[test]
    fn prompt_confirm_refresh_rejects_empty_input() {
        // Empty input (user pressed Enter) is the default-N
        // behavior. Pin it so a future refactor that defaults
        // to yes would be caught.
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let prior = Some("a\nb\nc\n");
        let template = "a\nB\nc\nd\n";
        assert!(
            !prompt_confirm_refresh(target, prior, template, "\n").unwrap(),
            "empty input must default to no"
        );
    }

    #[test]
    fn prompt_confirm_refresh_rejects_garbage() {
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let prior = Some("a\nb\nc\n");
        let template = "a\nB\nc\nd\n";
        assert!(
            !prompt_confirm_refresh(target, prior, template, "yes please\n").unwrap(),
            "only the literal y / Y must be accepted"
        );
    }

    #[test]
    fn prompt_confirm_refresh_handles_no_prior() {
        // No prior file: diff stat is (template.len(), 0).
        // The function must still parse the input correctly.
        let target = std::path::Path::new("/tmp/AGENTS.md");
        let template = "a\nb\nc\n";
        assert!(prompt_confirm_refresh(target, None, template, "y\n").unwrap());
        assert!(!prompt_confirm_refresh(target, None, template, "n\n").unwrap());
    }
}
