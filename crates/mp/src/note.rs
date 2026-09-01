use anyhow::{Context, Result};
use serde::Serialize;

use crate::idea;
use crate::paths::PlanContext;

#[derive(Debug, Serialize)]
pub struct NoteAddReport {
    pub ok: bool,
    pub idea_id: String,
    pub title: String,
    pub source: String,
}

pub fn note_add(
    ctx: &PlanContext,
    title: &str,
    body: Option<&str>,
    body_file: Option<&str>,
    to: Option<&str>,
    milestone_id: Option<&str>,
) -> Result<NoteAddReport> {
    const VALID_DESTINATIONS: &[&str] = &["idea"];
    let destination = to.unwrap_or("idea");
    if !VALID_DESTINATIONS.contains(&destination) {
        anyhow::bail!(
            "unsupported note destination: '{destination}' (valid destinations: {})",
            VALID_DESTINATIONS.join(", ")
        );
    }
    // Resolve the body (which may do file/stdin I/O) only after the destination
    // is known-good, so an invalid `--to` fails fast instead of blocking on stdin.
    if body.is_some() && body_file.is_some() {
        anyhow::bail!("--body and --body-file are mutually exclusive; pass one or the other");
    }
    let resolved_body = if let Some(path) = body_file {
        Some(read_body_file(path)?)
    } else {
        resolve_body(body)?
    };
    match destination {
        "idea" => {
            let idea = idea::idea_create_meeting(ctx, title, resolved_body.as_deref())?;
            // M202 S9: post-complete document-done hook. When the caller
            // passed --milestone-id AND that milestone's lifecycle is
            // `complete`, flip flow_stages.document to done idempotently.
            // The hook does NOT fail the note write — a milestone load
            // failure or a non-complete lifecycle is a no-op. Hand-off
            // is intentionally not touched (AC-11).
            if let Some(ms_id) = milestone_id {
                let _ = apply_document_done_hook(ctx, ms_id);
            }
            Ok(NoteAddReport {
                ok: true,
                idea_id: idea.id,
                title: idea.title,
                source: idea.source,
            })
        }
        // Defensive: unreachable given the upfront check above, but kept so a
        // future destination added to VALID_DESTINATIONS without a match arm
        // degrades to a clear error instead of a panic.
        other => anyhow::bail!(
            "unsupported note destination: '{other}' (valid destinations: {})",
            VALID_DESTINATIONS.join(", ")
        ),
    }
}

/// M202 S9: post-complete document-done stage hook for `mp note add`.
/// Mirrors the reviews-finding-resolve path (S10) so any post-completion
/// note / finding resolution auto-closes the document stage. Idempotent:
/// re-running on a milestone whose document stage is already done is a
/// no-op (the same `at` timestamp is preserved).
///
/// Failures are swallowed + silently ignored — a missing or non-complete
/// milestone must not block the note write (the note is the user's
/// primary intent; the stage flip is a derived side effect).
fn apply_document_done_hook(ctx: &PlanContext, milestone_id: &str) {
    use crate::milestone;
    use crate::model::FlowStage;
    use crate::store;
    let normalized = crate::paths::normalize_milestone_id(milestone_id);
    let path = match milestone::load_milestone_path(ctx, &normalized) {
        Ok(p) => p,
        Err(_) => return,
    };
    let mut m = match store::load_milestone(&path) {
        Ok(m) => m,
        Err(_) => return,
    };
    // The hook fires ONLY for milestones at terminal complete (AC-10
    // contract). Earlier lifecycles leave document pending — the
    // operator hasn't finished yet.
    if m.milestone.lifecycle != "complete" {
        return;
    }
    // Idempotent: skip when already done (preserve the original `at`).
    if let Some(existing) = m.milestone.flow_stages.get("document") {
        if existing.status == "done" {
            return;
        }
    }
    m.milestone.flow_stages.insert(
        "document".to_string(),
        FlowStage {
            status: "done".to_string(),
            at: Some(store::now_rfc3339()),
        },
    );
    m.milestone.updated = store::today();
    let _ = milestone::write_milestone_synced(ctx, &path, &m);
}

/// Read the body from a file path (or `-` for stdin) given to `--body-file`.
/// Expands a leading `~/` to `$HOME` so the natural `--body-file ~/notes.md`
/// works even when quoted (the shell doesn't expand `~` inside quotes).
fn read_body_file(path: &str) -> Result<String> {
    if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }
    let expanded = expand_tilde(path);
    let p = std::path::Path::new(&expanded);
    let content = std::fs::read_to_string(p)
        .with_context(|| format!("could not read --body-file: {}", p.display()))?;
    Ok(content)
}

/// Resolve a `--body` value that may be inline text, `@<path>` (read from file),
/// or `@-` (read from stdin). Inline text without a leading `@` is returned
/// verbatim so shell-quoted markdown still works.
///
/// The `@` sentinels are kept for backward-compat but are ambiguous with bodies
/// that legitimately start with `@` (e.g. `@username ping`); `--body-file` is
/// the unambiguous path and should be preferred (F-03).
fn resolve_body(body: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = body else {
        return Ok(None);
    };
    if let Some(rest) = raw.strip_prefix('@') {
        if rest.is_empty() {
            anyhow::bail!("--body '@' is ambiguous; use --body-file <path> for a file body");
        }
        if rest == "-" {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            return Ok(Some(buf));
        }
        let expanded = expand_tilde(rest);
        let path = std::path::Path::new(&expanded);
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "could not read body file: {} (tip: a literal body starting with '@' must use --body-file, or pass the text without a leading '@')",
                path.display()
            )
        })?;
        return Ok(Some(content));
    }
    Ok(Some(raw.to_string()))
}

/// Expand a leading `~/` to `$HOME`. Rust's `std::path::Path` deliberately
/// doesn't do this, so a quoted `~/foo` passed to `--body-file` (or `--body
/// @~/foo`) would otherwise fail with a confusing "could not read ~/foo".
/// Other uses of `~` (e.g. `~user`) are left untouched.
fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            if home.is_empty() {
                return p.to_string();
            }
            return format!("{home}/{rest}");
        }
    }
    if p == "~" {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return home;
            }
        }
    }
    p.to_string()
}
