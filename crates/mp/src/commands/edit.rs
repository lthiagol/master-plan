//! `mp edit <SUBCOMMAND>` — bulk plan-shape mutations.
//!
//! M105 S4 (B-41): currently only carries `strip-dropped-keys`. Future
//! edit subcommands (e.g., one-shot schema repair helpers) belong here
//! too — see the design decision in M105's `design_decisions`: "Adding it
//! to `mp edit` rather than a new top-level subcommand keeps the namespace
//! surface area flat — `mp edit` is the right place for milestone-shape
//! mutations."

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::{EditCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::migrate;
use crate::milestone;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub(crate) struct StripDroppedKeysReport {
    /// Files visited (regardless of whether anything was removed).
    pub files_scanned: usize,
    /// Files whose JSON was rewritten.
    pub files_modified: usize,
    /// Total keys removed across all files (sum of `removed.len()`).
    pub total_keys_removed: usize,
    /// Per-file removal list, keyed by the milestone filename (e.g.
    /// "sweep-b-42-sort-regression.json"). Empty for files that needed
    /// no surgery.
    pub removed_by_file: BTreeMap<String, Vec<String>>,
    /// True iff the run was a complete no-op (no file rewritten). Lets
    /// agents / CI treat a second run as confirmation rather than work.
    pub idempotent_run: bool,
}

pub(crate) fn cmd_edit(ctx: &PlanContext, cmd: EditCmd, format: Fmt) -> Result<()> {
    // Bulk edit rewrites many milestone files. Acquire PlanWriteTxn and use the
    // recoverable envelope for live writes so a mid-batch failure restores the
    // plan to one consistent before-image.
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    let recoverable = match &cmd {
        EditCmd::StripDroppedKeys => true,
        EditCmd::MigrateLifecycle { dry_run, yes } => !*dry_run && *yes,
        EditCmd::StripDeferredReason { dry_run, yes } => !*dry_run && *yes,
    };
    let op = |_: &crate::plan_io::PlanWriteTxn| match cmd {
        EditCmd::StripDroppedKeys => {
            let report = strip_dropped_keys(ctx).context("strip-dropped-keys")?;
            emit(format, &report)
        }
        EditCmd::MigrateLifecycle { dry_run, yes } => {
            let report = lifecycle_migration(ctx, dry_run, yes).context("migrate-lifecycle")?;
            emit(format, &report)
        }
        EditCmd::StripDeferredReason { dry_run, yes } => {
            let report = strip_deferred_reason(ctx, dry_run, yes)
                .with_context(|| "strip-deferred-reason failed")?;
            emit(format, &report)
        }
    };
    if recoverable {
        txn.run_recoverable(op)
    } else {
        txn.run(op)
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct StripDeferredReasonReport {
    pub dry_run: bool,
    pub yes: bool,
    pub files_scanned: usize,
    pub files_modified: usize,
    pub removed_by_file: BTreeMap<String, String>,
    pub idempotent_run: bool,
}

fn strip_deferred_reason(
    ctx: &PlanContext,
    dry_run: bool,
    yes: bool,
) -> Result<StripDeferredReasonReport> {
    // Dry-run wins over yes: never write when dry_run is set.
    let apply = !dry_run && yes;
    let (files_scanned, candidates, removed_by_file) =
        milestone::strip_deferred_reason_in_plan(ctx, !apply)?;
    // Align with manual-prefix-backfill: files_modified counts actual
    // writes only; dry-run reports candidates via removed_by_file size.
    let files_modified = if apply { candidates } else { 0 };
    Ok(StripDeferredReasonReport {
        dry_run: dry_run || !yes,
        yes,
        files_scanned,
        files_modified,
        removed_by_file,
        idempotent_run: candidates == 0,
    })
}

fn strip_dropped_keys(ctx: &PlanContext) -> Result<StripDroppedKeysReport> {
    let (modified, by_file) = milestone::strip_dropped_keys_in_plan(ctx)?;
    let total_keys_removed = by_file.values().map(|v| v.len()).sum::<usize>();
    let idempotent_run = modified == 0;

    let files_scanned = crate::store::list_milestone_paths(ctx)
        .context("list milestone paths")?
        .len();

    Ok(StripDroppedKeysReport {
        files_scanned,
        files_modified: modified,
        total_keys_removed,
        removed_by_file: by_file,
        idempotent_run,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct LifecycleMigrationCliReport {
    /// True iff no write happened (--dry-run OR --yes not supplied).
    pub dry_run: bool,
    /// Yes-flag passed on the command line.
    pub yes: bool,
    /// Number of milestones currently in legacy shape (read at entry).
    pub legacy_count: usize,
    /// Total milestone files inspected.
    pub total_inspected: usize,
    /// Files actually rewritten (==0 when dry_run is true).
    pub files_rewritten: usize,
    /// Files skipped (idempotent re-run on already-migrated milestones).
    pub files_skipped: usize,
    /// Per-id before/after preview entries — populated for every legacy
    /// milestone (even in dry-run), so the caller can see what the migrate
    /// would change.
    pub previews: Vec<LifecycleMigrationPreview>,
    /// Decode errors surfaced by the migration utility.
    pub decode_errors: Vec<(String, String)>,
}

#[derive(Debug, Serialize)]
pub(crate) struct LifecycleMigrationPreview {
    pub id: String,
    pub before: String,
    pub after: String,
}

/// M100 S10 / M100 ER-4: bridge the library-level migration utility
/// to the CLI.
///
/// Safety gates beyond `--dry-run`:
///   - **Validate-before-write was the load-bearing claim in the prior
///     comment, but ER-4 noted that the migration's purpose is itself
///     to repair the very gate errors that would have tripped the
///     gate. The gate would block legitimate fix-up runs. The actual
///     pre-write check is file-decode (handled by
///     `migrate::migrate_plan_lifecycle` per ER-5: partial decode
///     failures bail before any write). After the migration lands,
///     the operator should run `mp validate` per `cli.rs:1576` to
///     confirm the plan is clean. The doc fix tracks ER-4's lower-
///     effort alternative.
///   - Without `--yes` the command only prints the preview. The combination
///     `--dry-run --yes` is treated as "dry-run wins": never writes.
///   - Stale git working tree check is the operator's responsibility
///     (per AGENT-READINESS destructive-op policy); surfaced as a hint
///     rather than enforced here.
fn lifecycle_migration(
    ctx: &PlanContext,
    dry_run: bool,
    yes: bool,
) -> Result<LifecycleMigrationCliReport> {
    let paths = store::list_milestone_paths(ctx).context("list milestone paths")?;
    let mut all: Vec<crate::model::MilestoneFile> = Vec::with_capacity(paths.len());
    for path in &paths {
        let m = store::load_milestone(path).with_context(|| format!("load {}", path.display()))?;
        all.push(m);
    }
    let previews = migrate::preview_lifecycle_migration(&all);
    let legacy_count = migrate::count_legacy_milestones(&all);

    let mut files_rewritten = 0usize;
    let mut files_skipped = 0usize;
    let mut decode_errors: Vec<(String, String)> = Vec::new();

    if !dry_run && yes {
        // M100 ER-4 (lower-effort path): the migrate library is the
        // authority on file-level safety (decode errors are surfaced
        // as Err per ER-5). The CLI defers to the library and asks
        // the operator to run `mp validate` after the migration lands.
        let r = migrate::migrate_plan_lifecycle(&ctx.plan_dir).context("migrate_plan_lifecycle")?;
        files_rewritten = r.migrated;
        files_skipped = r.skipped;
        decode_errors = r
            .decode_errors
            .into_iter()
            .map(|(p, e)| (p.display().to_string(), e))
            .collect();
    }

    let preview_entries = previews
        .into_iter()
        .map(|(id, lc)| LifecycleMigrationPreview {
            id,
            before: String::new(),
            after: lc,
        })
        .collect();

    Ok(LifecycleMigrationCliReport {
        dry_run,
        yes,
        legacy_count,
        total_inspected: all.len(),
        files_rewritten,
        files_skipped,
        previews: preview_entries,
        decode_errors,
    })
}
