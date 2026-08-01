//! M177 S3: prefix non-runnable prose AC `verification` fields with
//! `manual: ` so the complete gate never shell-executes them.
//!
//! Idempotent: already-`manual:` values and non-prose runnables are
//! skipped. Each rewritten field also appends
//! `[manual-auto-prefix: <YYYY-MM-DD>]` so the rewrite is auditable.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::ac_verify;
use crate::model::MilestoneFile;
use crate::paths::PlanContext;
use crate::store;

const MANUAL_PREFIX: &str = "manual:";
const ANNOTATION_MARKER: &str = "[manual-auto-prefix:";

#[derive(Debug, Clone, Serialize)]
pub struct ManualPrefixHit {
    pub milestone_id: String,
    pub ac_id: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualPrefixBackfillReport {
    /// True when no write happened (`--dry-run` or missing `--yes`).
    pub dry_run: bool,
    pub yes: bool,
    pub files_scanned: usize,
    pub files_modified: usize,
    pub acs_rewritten: usize,
    pub hits: Vec<ManualPrefixHit>,
    /// True when this run rewrote nothing (already clean, or dry-run
    /// with zero candidates).
    pub idempotent_run: bool,
}

/// Walk every milestone; rewrite prose AC verifications that lack a
/// `manual:` prefix. Dry-run / missing `--yes` previews only.
pub fn run_manual_prefix_backfill(
    ctx: &PlanContext,
    dry_run: bool,
    yes: bool,
) -> Result<ManualPrefixBackfillReport> {
    let date = chrono_today();
    let paths = store::list_milestone_paths(ctx).context("list milestone paths")?;
    let mut hits: Vec<ManualPrefixHit> = Vec::new();
    let mut pending: Vec<(PathBuf, MilestoneFile)> = Vec::new();
    let files_scanned = paths.len();

    for path in &paths {
        let m = store::load_milestone(path).with_context(|| format!("load {}", path.display()))?;
        let mut changed = false;
        let mut file = m;
        for ac in &mut file.acceptance_criteria {
            if let Some(after) = rewrite_verification(&ac.verification, &date) {
                hits.push(ManualPrefixHit {
                    milestone_id: file.milestone.id.clone(),
                    ac_id: ac.id.clone(),
                    before: ac.verification.clone(),
                    after: after.clone(),
                });
                ac.verification = after;
                changed = true;
            }
        }
        if changed {
            pending.push((path.clone(), file));
        }
    }

    let acs_rewritten = hits.len();
    let mut files_modified = 0usize;
    let do_write = !dry_run && yes;
    if do_write {
        for (path, file) in &pending {
            write_milestone_file(path, file)
                .with_context(|| format!("write {}", path.display()))?;
            files_modified += 1;
        }
    }

    Ok(ManualPrefixBackfillReport {
        dry_run: dry_run || !yes,
        yes,
        files_scanned,
        files_modified,
        acs_rewritten,
        hits,
        idempotent_run: acs_rewritten == 0,
    })
}

fn rewrite_verification(value: &str, date: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_lowercase().starts_with(MANUAL_PREFIX) {
        return None;
    }
    if !ac_verify::looks_like_prose(trimmed) {
        return None;
    }
    // Avoid double-annotating if a prior partial rewrite left the marker
    // without the `manual:` prefix.
    if trimmed.contains(ANNOTATION_MARKER) {
        return Some(format!("{MANUAL_PREFIX} {trimmed}"));
    }
    Some(format!(
        "{MANUAL_PREFIX} {trimmed} [manual-auto-prefix: {date}]"
    ))
}

fn write_milestone_file(path: &Path, file: &MilestoneFile) -> Result<()> {
    // Canonical write path (prepare_for_disk + schema enforce), not a raw
    // pretty-print, so bulk migration stays aligned with single-file mutators.
    store::write_milestone(path, file)
}

fn chrono_today() -> String {
    // UTC calendar date (YYYY-MM-DD). Annotation is audit metadata, not
    // timezone-sensitive scheduling.
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => {
            let days = (d.as_secs() / 86_400) as i64;
            let (y, m, day) = civil_from_days(days);
            format!("{y:04}-{m:02}-{day:02}")
        }
        Err(_) => "1970-01-01".to_string(),
    }
}

/// Howard Hinnant civil-from-days (public domain).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_prefixes_prose_and_annotates() {
        let before = "crates/raul/tests/tui_view_state.rs (grep-based test)";
        let after = rewrite_verification(before, "2026-07-16").expect("rewrite");
        assert!(after.starts_with("manual: "));
        assert!(after.contains("[manual-auto-prefix: 2026-07-16]"));
        assert!(after.contains(before));
    }

    #[test]
    fn rewrite_skips_manual_prefix() {
        assert!(rewrite_verification(
            "manual: crates/raul/tests/tui_view_state.rs (grep-based test)",
            "2026-07-16"
        )
        .is_none());
    }

    #[test]
    fn rewrite_skips_runnable() {
        assert!(rewrite_verification("cargo test -p mp", "2026-07-16").is_none());
        assert!(rewrite_verification("crates/mp/tests/foo.rs", "2026-07-16").is_none());
    }

    #[test]
    fn rewrite_is_idempotent_after_prefix() {
        let once = rewrite_verification(
            "crates/raul/tests/keybinds.rs + rg for legends",
            "2026-07-16",
        )
        .expect("first");
        assert!(rewrite_verification(&once, "2026-07-16").is_none());
    }
}
