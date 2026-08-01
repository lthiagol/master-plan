//! One-time TOML → JSON migration for plan artifacts (milestone M92).
//!
//! Reads every `*.toml` under a plan dir, converts each to JSON (milestones
//! round-trip through the typed `MilestoneFile` struct so M82-dropped ceremony
//! fields are stripped), then writes all `.json` files and removes `.toml`
//! originals in separate phases so a parse error never leaves a half-converted tree.
//!
//! Exercised by `crates/mp/tests/json_migration.rs` and driven once against the
//! dogfood plan and fixtures by `crates/mp/examples/migrate-toml-to-json.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A single file conversion recorded in a [`MigrationReport`].
#[derive(Debug, Clone)]
pub struct ConvertedFile {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Summary of a migration run over one plan dir.
#[derive(Debug, Default, Clone)]
pub struct MigrationReport {
    pub converted: Vec<ConvertedFile>,
    pub skipped: Vec<PathBuf>,
}

impl MigrationReport {
    pub fn is_empty(&self) -> bool {
        self.converted.is_empty() && self.skipped.is_empty()
    }
}

/// True when `plan_dir` still contains legacy `.toml` plan artifacts (any path
/// under the dir except `legacy-toml/` snapshots).
pub fn plan_dir_has_legacy_toml(plan_dir: &Path) -> Result<bool> {
    let mut toml_files = Vec::new();
    collect_toml_files(plan_dir, &mut toml_files)?;
    Ok(!toml_files.is_empty())
}

struct PendingConversion {
    from: PathBuf,
    to: PathBuf,
    json: String,
}

/// Convert every `*.toml` under `plan_dir` to an equivalent `*.json` and
/// delete the originals. Returns a report of what was converted.
///
/// Phase 1 parses every file (fail fast on milestone decode errors).
/// Phase 2 writes all JSON. Phase 3 removes all TOML sources.
///
/// Files that are not valid TOML are skipped (and recorded), since a future
/// re-run after a partial migration must not choke on an already-migrated tree
/// that happens to contain a stray non-plan `.toml`.
pub fn migrate_plan_dir(plan_dir: &Path) -> Result<MigrationReport> {
    let mut report = MigrationReport::default();
    let mut toml_files = Vec::new();
    collect_toml_files(plan_dir, &mut toml_files)?;

    let mut pending = Vec::new();

    for from in toml_files {
        let raw = match fs::read_to_string(&from) {
            Ok(s) => s,
            Err(_) => {
                report.skipped.push(from);
                continue;
            }
        };
        let toml_value: toml::Value = match toml::from_str(&raw) {
            Ok(v) => v,
            Err(_) => {
                report.skipped.push(from);
                continue;
            }
        };
        let is_milestone = from
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("milestones")
            || from.file_name().and_then(|n| n.to_str()) == Some("milestone.toml");
        let json = if is_milestone {
            let m: crate::model::MilestoneFile = toml_value
                .try_into()
                .with_context(|| format!("decode milestone {}", from.display()))?;
            serde_json::to_string_pretty(&serde_json::to_value(&m)?)?
        } else {
            let json_value: serde_json::Value = serde_json::to_value(&toml_value)?;
            serde_json::to_string_pretty(&json_value)?
        };
        let to = from.with_extension("json");
        pending.push(PendingConversion { from, to, json });
    }

    for item in &pending {
        atomic_write(&item.to, format!("{}\n", item.json))
            .with_context(|| format!("write {}", item.to.display()))?;
        report.converted.push(ConvertedFile {
            from: item.from.clone(),
            to: item.to.clone(),
        });
    }

    for item in &pending {
        fs::remove_file(&item.from)
            .with_context(|| format!("remove old {}", item.from.display()))?;
    }

    Ok(report)
}

fn collect_toml_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("legacy-toml") {
                continue;
            }
            collect_toml_files(&path, out)?;
        } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
            out.push(path);
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, contents: String) -> Result<()> {
    crate::store::atomic_write(path, contents)
}

// ── Lifecycle migration (M100) ───────────────────────────────────────────────
//
// Maps legacy `spec_status` + `execution_status` (and the legacy `blocked`
// execution_status value) onto the new single `lifecycle` field plus orthogonal
// overlay fields (`blocked`, `deferred`, `cancelled`, `needs_regrooming`).
//
// Mapping rules:
//   spec_status: draft → draft, interview/review → groomed, ready → approved,
//                 implemented → done, verified → complete
//   execution_status: planned → draft, in-progress → in-progress,
//                      done → done, blocked → blocked overlay,
//                      deferred → deferred overlay, cancelled → cancelled overlay
//
// If both legacy fields are present, the more advanced lifecycle wins
// (verified > implemented > ready > review > interview > draft on the spec side,
// in-progress > planned on the exec side). The `done` execution_status implies
// the milestone is at `complete`, so it dominates `in-progress`.
//
// The legacy fields are removed from the on-disk file once mapped so subsequent
// reads go through the new field. (Reverse compat is provided by
// `effective_lifecycle` for the migration window only.)

use crate::model::{
    effective_lifecycle, effective_lifecycle_from_legacy, MilestoneFile, MilestoneMeta,
};

/// Map a legacy milestone to the new shape. Idempotent: re-running on an
/// already-migrated milestone is a no-op.
pub fn migrate_milestone_to_lifecycle(mut m: MilestoneFile) -> MilestoneFile {
    // 1. If lifecycle is already populated and legacy fields are empty,
    //    nothing to do — except backfill a missing lifecycle_at so the
    //    TUI "since" column is not stuck on "since updated"
    //    (external-review F-07). Also: if the lifecycle is still the
    //    legacy `"done"` (post-migration window), rewrite to the
    //    canonical `"executed"` (M196).
    if !m.milestone.lifecycle.is_empty()
        && m.milestone.spec_status.is_empty()
        && m.milestone.execution_status.is_empty()
    {
        if m.milestone.lifecycle == "done" {
            // M196: legacy `lifecycle: "done"` (post the M100 migration
            // window, pre the M196 rename) gets rewritten to the
            // canonical `"executed"`. Idempotent — re-running on a
            // milestone already at `"executed"` is a no-op.
            m.milestone.lifecycle = "executed".to_string();
        }
        if m.milestone.lifecycle_at.is_none() {
            m.milestone.lifecycle_at = Some(
                lifecycle_at_for_migration(&m.milestone.created)
                    .unwrap_or_else(crate::store::now_rfc3339),
            );
        }
        return m;
    }

    // 2. Pull overlay flags out of the legacy execution_status before we
    //    overwrite it.
    if m.milestone.execution_status == "blocked" {
        m.milestone.blocked = true;
    }
    if m.milestone.execution_status == "deferred" {
        m.milestone.deferred = true;
        m.milestone.deferred_reason = m.milestone.block_reason.clone();
    }
    if m.milestone.execution_status == "cancelled" {
        m.milestone.cancelled = true;
    }

    // 3. Set lifecycle through MigrateRaw so the shared transition table
    //    remains the sole writer. `effective_lifecycle` picks the more
    //    advanced value when both legacy fields are present.
    let new_lc = effective_lifecycle(&m.milestone);
    // M196: the executor's end-state was renamed from `"done"` to
    // `"executed"` on the lifecycle side. The legacy `execution_status`
    // value `"done"` still maps to the executor's end-state (which
    // is now the canonical lifecycle string `"executed"`); the
    // `effective_lifecycle` helper already does this mapping through
    // `legacy_execution_status_to_lifecycle`. Apply the rename here
    // before the MigrateRaw call so the on-disk write emits
    // `"executed"` not `"done"`.
    let new_lc = if new_lc == "done" {
        "executed"
    } else {
        new_lc.as_str()
    };
    crate::milestone::apply_migrate_raw(&mut m, new_lc).unwrap_or_else(|err| {
        panic!("migrate MigrateRaw({new_lc}) failed: {err}");
    });
    // M144: backfill `lifecycle_at` so the TUI's "since" column shows a
    // sensible relative time on migrated milestones. We don't have the
    // actual transition timestamp, but we DO have `created` (a YYYY-MM-DD
    // string) — format it into a UTC-midnight RFC3339 timestamp so the
    // humanizer renders an approximate relative time ("3d ago") rather
    // than "just now" / "unknown". Falls back to `now_rfc3339()` when
    // `created` is empty or malformed.
    m.milestone.lifecycle_at = Some(
        lifecycle_at_for_migration(&m.milestone.created).unwrap_or_else(crate::store::now_rfc3339),
    );

    // 4. If the new lifecycle is `complete` (legacy verified+done), and the
    //    legacy execution_status was `in-progress`, the legacy record was
    //    inconsistent — we prefer the lifecycle value.
    //    No special-case needed; the helper handles it.

    // 5. Clear legacy fields so on-disk reads go through `lifecycle` directly.
    m.milestone.spec_status = String::new();
    m.milestone.execution_status = String::new();

    m
}

/// Format a YYYY-MM-DD string into a UTC-midnight RFC3339 timestamp
/// suitable for `lifecycle_at` backfill during migration. Returns
/// `None` when `created` is empty or malformed so the caller falls
/// back to `now_rfc3339()`.
///
/// We anchor at 00:00:00Z (start of UTC day) instead of the current
/// time-of-day so the relative-time rendering reflects the milestone's
/// age in days/weeks rather than flickering from `just now` to `5m ago`
/// between migration invocations.
fn lifecycle_at_for_migration(created: &str) -> Option<String> {
    let bytes = created.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    // Verify the YYYY-MM-DD shape: bytes 0-3 are digits, byte 4 is
    // '-', bytes 5-6 are digits, byte 7 is '-', bytes 8-9 are digits.
    // A malformed `created` (e.g. from a half-written file) returns
    // None so the caller falls back to `now_rfc3339()`.
    let is_dash = |i: usize| bytes[i] == b'-';
    let is_digit = |i: usize| bytes[i].is_ascii_digit();
    let year_ok = (0..4).all(is_digit);
    if !(year_ok
        && is_dash(4)
        && is_digit(5)
        && is_digit(6)
        && is_dash(7)
        && is_digit(8)
        && is_digit(9))
    {
        return None;
    }
    // The basic shape check above is enough for migration purposes —
    // the humanizer in `raul::tui::humanize` parses positionally and
    // does not validate month/day ranges. We append a `T00:00:00Z`
    // suffix so the result is RFC3339-shaped and parseable.
    Some(format!("{created}T00:00:00Z"))
}

/// Count of milestones needing migration (lifecycle is the serde default AND
/// at least one legacy field is set). Used by `mp migrate lifecycle` to gate
/// on whether the migration is necessary at all.
pub fn count_legacy_milestones(milestones: &[MilestoneFile]) -> usize {
    milestones
        .iter()
        .filter(|m| is_legacy_shape(&m.milestone))
        .count()
}

fn is_legacy_shape(meta: &MilestoneMeta) -> bool {
    // Lifecycle is the serde default ("draft") AND at least one legacy field
    // is populated. After migration, lifecycle is set explicitly and legacy
    // fields are cleared.
    (meta.lifecycle.is_empty() || meta.lifecycle == "draft")
        && (!meta.spec_status.is_empty() || !meta.execution_status.is_empty())
}

/// Preview (dry-run) the lifecycle migration without writing any files.
/// Returns a list of (id, current_lifecycle) pairs the run would produce.
pub fn preview_lifecycle_migration(milestones: &[MilestoneFile]) -> Vec<(String, String)> {
    milestones
        .iter()
        .filter(|m| is_legacy_shape(&m.milestone))
        .map(|m| {
            let migrated = migrate_milestone_to_lifecycle(m.clone());
            (m.milestone.id.clone(), migrated.milestone.lifecycle.clone())
        })
        .collect()
}

/// M100 AC-07: bulk migration over a plan directory. Reads every milestone
/// file under `plan_dir`, migrates each from the legacy 3-field shape to the
/// new unified-lifecycle shape, writes the migrated JSON back, and returns a
/// per-file report.
///
/// Migration is per-file with phase separation (Phase 1 decode all → Phase 2
/// write all) so a single decode error never leaves a half-converted tree.
/// Idempotent: re-running on already-migrated milestones is a no-op.
pub fn migrate_plan_lifecycle(plan_dir: &Path) -> Result<LifecycleMigrationReport> {
    let mut report = LifecycleMigrationReport::default();
    let mut pending: Vec<(PathBuf, MilestoneFile)> = Vec::new();
    let mut decode_errors: Vec<(PathBuf, String)> = Vec::new();

    for path in collect_milestone_files(plan_dir) {
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                decode_errors.push((path.clone(), format!("read: {e}")));
                continue;
            }
        };
        let m: MilestoneFile = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(e) => {
                // M100 ER-5: decode errors must surface as a hard
                // failure, not a silent Ok-with-empty-decode_errors.
                // Partial-migration was masked behind `Ok(())` for the
                // whole batch; autonomous callers (and the dogfood
                // migration utility itself) relied on a non-zero exit
                // code to know the run was clean. We collect the
                // errors here and `bail!` after the read loop so the
                // report still carries the per-file breakdown for
                // diagnostic use.
                decode_errors.push((path.clone(), format!("decode: {e}")));
                continue;
            }
        };
        let migrated = migrate_milestone_to_lifecycle(m);
        pending.push((path, migrated));
    }

    // M100 ER-5: surface partial-failure decode errors as Err so the
    // CLI exits non-zero. The mutation loop has not yet started, so no
    // writes happened.
    if !decode_errors.is_empty() {
        anyhow::bail!(
            "migrate-lifecycle: {} file(s) failed to decode; partial migration refused. \
             Run `mp edit migrate-lifecycle --dry-run` to preview; fix the offending files and retry.",
            decode_errors.len()
        );
    }

    // M100 ER-9: migrate the plan.json index entries alongside the
    // milestone files. The 2026-07-05 bulk migration rewrote the files
    // but left the index carrying legacy `spec_status` /
    // `execution_status`. The W03 drift check paper-matches this via
    // `derive_legacy_status_for_w03`, but the index is now a legacy
    // artifact that must be kept in sync by derive helpers
    // indefinitely. Two fixes per entry: (a) add `lifecycle`, (b)
    // reconcile the index's `spec_status` / `execution_status` against
    // the migrated file so W03 stops firing on the index vs file
    // mismatch. We derive spec/exec on the legacy strings using the
    // same order-rank rules as `crate::model::effective_lifecycle` so
    // the index entries match the read-side derivation.
    let plan_json_path = plan_dir.join("plan.json");
    if plan_json_path.is_file() {
        let raw = fs::read_to_string(&plan_json_path)
            .with_context(|| format!("read {}", plan_json_path.display()))?;
        let mut parsed: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("decode {}", plan_json_path.display()))?;
        // Build a lookup from milestone id → migrated file so we can
        // rewrite the index entry's spec/exec from disk state.
        let migrated_by_id: std::collections::HashMap<String, &MilestoneFile> = pending
            .iter()
            .map(|(_, m)| (m.milestone.id.clone(), m))
            .collect();
        if let Some(entries) = parsed.get_mut("milestones").and_then(|v| v.as_array_mut()) {
            let mut index_changed = false;
            for entry in entries.iter_mut() {
                let entry_id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // L4: name the JSON-derived strings explicitly so they
                // don't shadow the imported `crate::validate::effective_*`
                // helpers when read below.
                let spec_legacy = entry
                    .get("spec_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let exec_legacy = entry
                    .get("execution_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(obj) = entry.as_object_mut() {
                    // (a) Add `lifecycle` if missing.
                    if obj.get("lifecycle").is_none() {
                        let lc = if let Some(m) = migrated_by_id.get(&entry_id) {
                            m.effective_lifecycle()
                        } else {
                            effective_lifecycle_from_legacy(&spec_legacy, &exec_legacy)
                        };
                        obj.insert("lifecycle".to_string(), serde_json::json!(lc));
                        index_changed = true;
                    }
                    // (b) Reconcile index spec/exec against the migrated
                    // file so W03 stops tripping on the index vs file
                    // mismatch. We use the file (authoritative) and
                    // derive the legacy string from its lifecycle,
                    // via the canonical helpers in
                    // `crate::validate::plan`.
                    if let Some(m) = migrated_by_id.get(&entry_id) {
                        let derived_spec = crate::validate::effective_spec_status(m);
                        let derived_exec = crate::validate::effective_execution_status(m);
                        if obj.get("spec_status").map(|v| v.as_str().unwrap_or(""))
                            != Some(derived_spec.as_str())
                            || obj
                                .get("execution_status")
                                .map(|v| v.as_str().unwrap_or(""))
                                != Some(derived_exec.as_str())
                        {
                            obj.insert("spec_status".to_string(), serde_json::json!(derived_spec));
                            obj.insert(
                                "execution_status".to_string(),
                                serde_json::json!(derived_exec),
                            );
                            index_changed = true;
                        }
                    }
                }
            }
            if index_changed {
                let formatted = format!(
                    "{}\n",
                    serde_json::to_string_pretty(&parsed).context("serialize plan.json")?
                );
                atomic_write(&plan_json_path, formatted)
                    .with_context(|| format!("write {}", plan_json_path.display()))?;
                report.changed.push(plan_json_path.clone());
                report.migrated += 1;
            }
        }
    }

    // Phase 2: write all migrated files.
    for (path, m) in &pending {
        // Skip files that didn't need migration (idempotent no-op).
        let original = fs::read_to_string(path).unwrap_or_default();
        let migrated_json = serde_json::to_string_pretty(m)?;
        if original.trim_end() == migrated_json.trim_end() {
            report.skipped += 1;
            continue;
        }
        let formatted = format!("{migrated_json}\n");
        atomic_write(path, formatted.clone())
            .with_context(|| format!("write {}", path.display()))?;
        report.migrated += 1;
        report.changed.push(path.clone());
    }

    // M100 ER-5 + M3 remediation: partial decode errors now bail! at
    // the top of this function (before any write), so by the time we
    // reach the write loop `decode_errors` is provably empty. No
    // tail-end merge is needed; just return the accumulated report.
    debug_assert!(
        decode_errors.is_empty(),
        "decode_errors leaked past the early bail in migrate_plan_lifecycle"
    );
    Ok(report)
}

/// Recursively find every `*.json` file directly under `plan_dir/milestones/`.
/// (Excludes archive/, sessions/, and other subdirs.)
fn collect_milestone_files(plan_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let dir = plan_dir.join("milestones");
    if !dir.is_dir() {
        return out;
    }
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[derive(Debug, Default, Clone)]
pub struct LifecycleMigrationReport {
    pub migrated: usize,
    pub skipped: usize,
    pub changed: Vec<PathBuf>,
    pub decode_errors: Vec<(PathBuf, String)>,
}

// ── Kinds migration (M102 R3 — F-04 / F-08) ───────────────────────

/// M102 R3 (F-04 / F-08): a `BacklogFile`-shape summary of a kinds
/// migration. Reported back so the CLI can show a diff (--dry-run) or
/// persist (default).
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct KindsMigrationReport {
    /// Backlog items appended from `track-bugfix.json` (kind=bug).
    pub from_bugfix: usize,
    /// Backlog items appended from `track-tweak.json` (kind=tweak).
    pub from_tweak: usize,
    /// Backlog items appended from `ideas.json` (kind=idea, priority=low).
    pub from_ideas: usize,
    /// Source files deleted after a non-dry-run migration.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files_deleted: Vec<PathBuf>,
}

/// M102 R3 (F-04 / F-08): collapse `track-bugfix.json`,
/// `track-tweak.json`, and `ideas.json` into `backlog.json` with
/// `BacklogItem.kind` populated (`bug` / `tweak` / `idea`).
///
/// Idempotent: each source file is read once, and items already
/// present in `backlog.json` (matched by id) are skipped — re-running
/// on a partially-migrated plan is a no-op for the items that already
/// landed.
///
/// `dry_run = true` computes the report without writing anything
/// (used by `mp migrate --kinds --dry-run`).
pub fn migrate_kinds(plan_dir: &Path, dry_run: bool) -> Result<KindsMigrationReport> {
    use crate::model::{BacklogFile, BacklogItem, IdeasFile, TrackFile};

    // BF-16 (M131): guard the plan dir before operating. This function
    // takes a raw `plan_dir` (not a PlanContext), so it bypassed the
    // `ensure_plan_exists` guard other commands run via the dispatcher.
    // Without this check it would silently no-op (or write into) a
    // non-existent directory.
    anyhow::ensure!(
        plan_dir.is_dir(),
        "plan directory not found: {}",
        plan_dir.display()
    );

    let mut report = KindsMigrationReport::default();

    // Load existing backlog (may be empty / missing) so we can dedup.
    let backlog_path = plan_dir.join("backlog.json");
    let mut backlog: BacklogFile = if backlog_path.exists() {
        serde_json::from_str(&fs::read_to_string(&backlog_path)?)
            .with_context(|| format!("read backlog.json at {}", backlog_path.display()))?
    } else {
        BacklogFile::default()
    };
    let mut existing_ids: std::collections::HashSet<String> =
        backlog.items.iter().map(|i| i.id.clone()).collect();
    let mut max_n: u32 = backlog
        .items
        .iter()
        .filter_map(|i| i.id.strip_prefix("B-").and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    let next_id = |max: &mut u32| {
        *max += 1;
        format!("B-{max:02}")
    };

    // C-1 fix: track files live at plan_dir/tracks/{kind}.json
    // (the canonical location used by every other reader in the
    // codebase via PlanContext::track_path). Reading from
    // plan_dir/track-{kind}.json was the old flat layout and would
    // silently no-op against any real plan.
    let tracks_dir = plan_dir.join("tracks");

    // H-1 atomicity: do all READS first, then WRITE the merged
    // backlog, then DELETE source files. Previously the function
    // deleted each source file as it was processed, so a parse
    // error or any later `?` would leave the source deleted but
    // the merge never persisted — the items in the deleted file
    // were gone. Now: rename-to-.bak first (so a hard kill between
    // rename and final delete can be recovered from .bak), build the
    // merged backlog in memory, write backlog.json via temp + atomic
    // rename, then delete the source files (and the .bak if any).
    // On dry-run: skip rename + delete + write; only report.
    let bugfix_path = tracks_dir.join("bugfix.json");
    let tweak_path = tracks_dir.join("tweak.json");
    let ideas_path = plan_dir.join("ideas.json");

    let bugfix_bak = tracks_dir.join("bugfix.json.bak");
    let tweak_bak = tracks_dir.join("tweak.json.bak");
    let ideas_bak = plan_dir.join("ideas.json.bak");

    if !dry_run {
        for (src, bak) in [
            (&bugfix_path, &bugfix_bak),
            (&tweak_path, &tweak_bak),
            (&ideas_path, &ideas_bak),
        ] {
            if src.exists() {
                fs::rename(src, bak)
                    .with_context(|| format!("rename {} to .bak", src.display()))?;
            }
        }
    }

    // track-bugfix → kind=bug
    if bugfix_path.exists() || bugfix_bak.exists() {
        let read_path = if dry_run { &bugfix_path } else { &bugfix_bak };
        let track: TrackFile = serde_json::from_str(&fs::read_to_string(read_path)?)
            .with_context(|| format!("read track-bugfix.json at {}", read_path.display()))?;
        for item in &track.items {
            if existing_ids.contains(&item.id) {
                continue;
            }
            let id = if item.id.starts_with("BF-") || item.id.starts_with("B-") {
                item.id.clone()
            } else {
                next_id(&mut max_n)
            };
            existing_ids.insert(id.clone());
            backlog.items.push(BacklogItem {
                id,
                description: format!("[bugfix] {}", item.title),
                source: "track-bugfix".to_string(),
                suggested_when: String::new(),
                priority: "regular".to_string(),
                status: item.status.clone(),
                resolution: String::new(),
                resolved_at: String::new(),
            });
            report.from_bugfix += 1;
        }
        // H-1 atomicity: deletion deferred until after the backlog write.
    }

    // track-tweak → kind=tweak
    if tweak_path.exists() || tweak_bak.exists() {
        let read_path = if dry_run { &tweak_path } else { &tweak_bak };
        let track: TrackFile = serde_json::from_str(&fs::read_to_string(read_path)?)
            .with_context(|| format!("read track-tweak.json at {}", read_path.display()))?;
        for item in &track.items {
            if existing_ids.contains(&item.id) {
                continue;
            }
            let id = if item.id.starts_with("TW-") || item.id.starts_with("B-") {
                item.id.clone()
            } else {
                next_id(&mut max_n)
            };
            existing_ids.insert(id.clone());
            backlog.items.push(BacklogItem {
                id,
                description: format!("[tweak] {}", item.title),
                source: "track-tweak".to_string(),
                suggested_when: String::new(),
                priority: "regular".to_string(),
                status: item.status.clone(),
                resolution: String::new(),
                resolved_at: String::new(),
            });
            report.from_tweak += 1;
        }
        // H-1 atomicity: deletion deferred until after the backlog write.
    }

    // ideas → kind=idea, priority=low
    if ideas_path.exists() || ideas_bak.exists() {
        let read_path = if dry_run { &ideas_path } else { &ideas_bak };
        let ideas: IdeasFile = serde_json::from_str(&fs::read_to_string(read_path)?)
            .with_context(|| format!("read ideas.json at {}", read_path.display()))?;
        for entry in &ideas.ideas {
            // Reuse the idea's existing id (ID-NN) or assign a new B-NN.
            let id = if entry.id.starts_with("ID-") || entry.id.starts_with("B-") {
                entry.id.clone()
            } else {
                next_id(&mut max_n)
            };
            if existing_ids.contains(&id) {
                continue;
            }
            existing_ids.insert(id.clone());
            backlog.items.push(BacklogItem {
                id,
                description: format!("[idea] {}", entry.title),
                source: "ideas".to_string(),
                suggested_when: String::new(),
                priority: "low".to_string(),
                status: entry.status.clone(),
                resolution: String::new(),
                resolved_at: String::new(),
            });
            report.from_ideas += 1;
        }
        // H-1 atomicity: deletion deferred until after the backlog write.
    }

    if !dry_run && (report.from_bugfix + report.from_tweak + report.from_ideas) > 0 {
        let backlog_path = plan_dir.join("backlog.json");
        std::fs::create_dir_all(plan_dir).ok();
        // H-1 atomicity: write to a temp file, then atomically rename
        // onto the target. A crash between write and rename leaves the
        // existing backlog.json intact and the new content as
        // `backlog.json.tmp` (recoverable). The earlier rename-to-.bak
        // step also protects the source files in the same way.
        let tmp_path = backlog_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serde_json::to_string_pretty(&backlog)?)
            .with_context(|| format!("write {} (temp)", tmp_path.display()))?;
        fs::rename(&tmp_path, &backlog_path).with_context(|| {
            format!("rename {} → {}", tmp_path.display(), backlog_path.display())
        })?;
    }

    // H-1 atomicity: AFTER the backlog write succeeds, delete the
    // .bak files (the originals were renamed to .bak at function start).
    // If this fails, the backlog has the migrated data and the .bak
    // files are recoverable; subsequent runs are idempotent because
    // the merged ids are already in `existing_ids`.
    if !dry_run {
        for bak in [&bugfix_bak, &tweak_bak, &ideas_bak] {
            if bak.exists() {
                fs::remove_file(bak).with_context(|| format!("delete {}", bak.display()))?;
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MilestoneFile, MilestoneMeta};

    fn meta_with_legacy(spec: &str, exec: &str) -> MilestoneFile {
        MilestoneFile {
            milestone: MilestoneMeta {
                id: "42".into(),
                title: "t".into(),
                slug: "t".into(),
                lifecycle: String::new(),
                spec_status: spec.into(),
                execution_status: exec.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn maps_verified_done_to_complete() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("verified", "done"));
        assert_eq!(m.milestone.lifecycle, "complete");
        assert!(m.milestone.spec_status.is_empty());
        assert!(m.milestone.execution_status.is_empty());
    }

    #[test]
    fn maps_ready_planned_to_approved() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("ready", "planned"));
        assert_eq!(m.milestone.lifecycle, "approved");
    }

    #[test]
    fn maps_draft_planned_to_draft() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("draft", "planned"));
        assert_eq!(m.milestone.lifecycle, "draft");
    }

    #[test]
    fn maps_interview_to_groomed() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("interview", "planned"));
        assert_eq!(m.milestone.lifecycle, "groomed");
    }

    #[test]
    fn maps_blocked_overlay() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("verified", "blocked"));
        // verified+blocked → complete (lifecycle) + blocked (overlay)
        assert_eq!(m.milestone.lifecycle, "complete");
        assert!(m.milestone.blocked);
    }

    #[test]
    fn maps_deferred_overlay_with_reason() {
        let mut m = meta_with_legacy("ready", "deferred");
        m.milestone.block_reason = "postponed to v3".into();
        let m = migrate_milestone_to_lifecycle(m);
        assert_eq!(m.milestone.lifecycle, "approved");
        assert!(m.milestone.deferred);
        assert_eq!(m.milestone.deferred_reason, "postponed to v3");
    }

    #[test]
    fn maps_cancelled_overlay() {
        let m = migrate_milestone_to_lifecycle(meta_with_legacy("ready", "cancelled"));
        assert!(m.milestone.cancelled);
        assert_eq!(m.milestone.lifecycle, "approved");
    }

    #[test]
    fn idempotent_on_already_migrated() {
        let mut m = meta_with_legacy("verified", "done");
        m = migrate_milestone_to_lifecycle(m);
        let m2 = migrate_milestone_to_lifecycle(m.clone());
        assert_eq!(m2.milestone.lifecycle, m.milestone.lifecycle);
        assert_eq!(m2.milestone.spec_status, m.milestone.spec_status);
    }

    #[test]
    fn count_legacy_only_counts_unmigrated() {
        let legacy = meta_with_legacy("ready", "planned");
        let mut migrated = legacy.clone();
        migrated.milestone.lifecycle = "approved".into();
        migrated.milestone.spec_status = String::new();
        migrated.milestone.execution_status = String::new();
        assert_eq!(count_legacy_milestones(&[legacy, migrated]), 1);
    }

    #[test]
    fn preview_returns_pairs() {
        let v = vec![
            meta_with_legacy("verified", "done"),
            meta_with_legacy("ready", "planned"),
        ];
        let pairs = preview_lifecycle_migration(&v);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].1, "complete");
        assert_eq!(pairs[1].1, "approved");
    }

    #[test]
    fn empty_legacy_does_not_migrate() {
        let mut m = MilestoneFile::default();
        m.milestone.id = "42".into();
        m.milestone.title = "t".into();
        m.milestone.lifecycle = "approved".into();
        let count = count_legacy_milestones(&[m.clone()]);
        assert_eq!(count, 0);
        let pairs = preview_lifecycle_migration(&[m.clone()]);
        assert!(pairs.is_empty());
    }

    #[test]
    fn bulk_migration_writes_only_changed_files() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let plan_dir = tmp.path();
        let milestones = plan_dir.join("milestones");
        std::fs::create_dir_all(&milestones).unwrap();

        // Legacy file 1: ready/done → executed (exec done wins over spec ready;
        // executed means "work finished, awaiting review", NOT terminal complete).
        let mut legacy = MilestoneFile::default();
        legacy.milestone.id = "01".into();
        legacy.milestone.title = "legacy".into();
        legacy.milestone.slug = "legacy".into();
        legacy.milestone.spec_status = "ready".into();
        legacy.milestone.execution_status = "done".into();
        std::fs::write(
            milestones.join("01-legacy.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        // Already-migrated file 2 (lifecycle populated + legacy fields empty).
        // Pin `lifecycle_at` so the M144 backfill (external-review F-07) is
        // a no-op and the skip-on-equality branch keeps the file untouched.
        let mut migrated = MilestoneFile::default();
        migrated.milestone.id = "02".into();
        migrated.milestone.title = "migrated".into();
        migrated.milestone.slug = "migrated".into();
        migrated.milestone.lifecycle = "complete".into();
        migrated.milestone.lifecycle_at = Some("2026-07-01T00:00:00Z".to_string());
        std::fs::write(
            milestones.join("02-migrated.json"),
            format!("{}\n", serde_json::to_string_pretty(&migrated).unwrap()),
        )
        .unwrap();

        let report = migrate_plan_lifecycle(plan_dir).unwrap();
        assert_eq!(report.migrated, 1, "should migrate exactly the legacy file");
        assert_eq!(report.skipped, 1, "should skip the already-migrated file");
        assert!(report.decode_errors.is_empty());

        // Verify the migrated file now has lifecycle="executed" (max of
        // spec=ready → approved, exec=done → executed; M196 rename) and no
        // legacy fields.
        let on_disk: MilestoneFile = serde_json::from_str(
            &std::fs::read_to_string(milestones.join("01-legacy.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.milestone.lifecycle, "executed");
        assert!(on_disk.milestone.spec_status.is_empty());
        assert!(on_disk.milestone.execution_status.is_empty());
    }

    #[test]
    fn bulk_migration_is_idempotent() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let plan_dir = tmp.path();
        let milestones = plan_dir.join("milestones");
        std::fs::create_dir_all(&milestones).unwrap();

        let mut legacy = MilestoneFile::default();
        legacy.milestone.id = "01".into();
        legacy.milestone.title = "t".into();
        legacy.milestone.slug = "t".into();
        legacy.milestone.spec_status = "ready".into();
        legacy.milestone.execution_status = "done".into();
        std::fs::write(
            milestones.join("01.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        // First run migrates.
        let r1 = migrate_plan_lifecycle(plan_dir).unwrap();
        assert_eq!(r1.migrated, 1);
        // Second run is a no-op.
        let r2 = migrate_plan_lifecycle(plan_dir).unwrap();
        assert_eq!(r2.migrated, 0);
        assert_eq!(r2.skipped, 1);
    }

    #[test]
    fn bulk_migration_handles_milestone_with_overlays() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let plan_dir = tmp.path();
        let milestones = plan_dir.join("milestones");
        std::fs::create_dir_all(&milestones).unwrap();

        // verified+blocked → complete + blocked overlay (with reason)
        let mut legacy = MilestoneFile::default();
        legacy.milestone.id = "01".into();
        legacy.milestone.title = "t".into();
        legacy.milestone.slug = "t".into();
        legacy.milestone.spec_status = "verified".into();
        legacy.milestone.execution_status = "blocked".into();
        legacy.milestone.block_reason = "awaiting review".into();
        legacy.milestone.blocked_at = "2026-07-04T00:00:00Z".into();
        std::fs::write(
            milestones.join("01.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let report = migrate_plan_lifecycle(plan_dir).unwrap();
        assert_eq!(report.migrated, 1);

        let on_disk: MilestoneFile =
            serde_json::from_str(&std::fs::read_to_string(milestones.join("01.json")).unwrap())
                .unwrap();
        assert_eq!(on_disk.milestone.lifecycle, "complete");
        assert!(on_disk.milestone.blocked);
        assert_eq!(on_disk.milestone.block_reason, "awaiting review");
    }

    // BF-16 (M131): migrate_kinds takes a raw plan_dir (not a
    // PlanContext), so it bypassed the ensure_plan_exists guard the
    // dispatcher runs for other commands. It must refuse to operate on
    // a non-existent plan dir rather than silently no-op / write into
    // a missing directory.
    #[test]
    fn migrate_kinds_requires_plan_exists() {
        let bogus = std::path::PathBuf::from("/this/plan/does/not/exist");
        let err = migrate_kinds(&bogus, true).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("plan directory not found"),
            "expected plan-dir guard, got: {msg}"
        );
    }

    // M144 fix: `lifecycle_at_for_migration` formats a YYYY-MM-DD
    // `created` into a UTC-midnight RFC3339 string for the humanizer.
    // Validates the shape check + the date-digit guard.
    #[test]
    fn lifecycle_at_for_migration_well_formed() {
        assert_eq!(
            lifecycle_at_for_migration("2026-01-01"),
            Some("2026-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn lifecycle_at_for_migration_rejects_empty_or_short() {
        assert_eq!(lifecycle_at_for_migration(""), None);
        assert_eq!(lifecycle_at_for_migration("2026"), None);
        assert_eq!(lifecycle_at_for_migration("2026-01"), None);
    }

    #[test]
    fn lifecycle_at_for_migration_rejects_malformed_digits() {
        // Wrong separator.
        assert_eq!(lifecycle_at_for_migration("2026/01/01"), None);
        // Non-digit characters in the year.
        assert_eq!(lifecycle_at_for_migration("abcd-01-01"), None);
        // Non-digit month.
        assert_eq!(lifecycle_at_for_migration("2026-aa-01"), None);
        // Non-digit day.
        assert_eq!(lifecycle_at_for_migration("2026-01-xx"), None);
    }
}
