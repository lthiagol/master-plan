use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::{store, sync};

use super::spec::DROPPED_CEREMONY_KEYS;

pub fn load_milestone_path(ctx: &PlanContext, id: &str) -> Result<PathBuf> {
    let norm = paths::normalize_milestone_id(id);
    paths::find_milestone_file(&ctx.milestones_dir(), &norm)
        .with_context(|| format!("milestone {id} not found"))
}

pub fn load_milestone_by_id(ctx: &PlanContext, id: &str) -> Result<MilestoneFile> {
    let path = load_milestone_path(ctx, id)?;
    store::load_milestone(&path)
}

/// Compute the next `<PREFIX>-NN` id for a fragment list by taking the max
/// numeric suffix across existing ids + 1, NOT `len() + 1`. M111 external
/// review (2026-07-07): the len-based formula collides after a removal —
/// removing AC-02 from `[AC-01, AC-02, AC-03]` leaves `len()=2`, so the next
/// add computes `AC-03` and duplicates the surviving AC-03. Parity with
/// `step::next_step_id` (which already uses max+1). `<PREFIX>` is `AC` for
/// acceptance criteria, `Q` for open questions. Zero-padded two-digit for AC,
/// unprefixed-width for Q (matches existing on-disk shape).
pub fn next_fragment_id<T>(items: &[T], id_of: impl Fn(&T) -> &str, prefix: &str) -> String {
    let mut max = 0u32;
    for item in items {
        if let Some(n) = parse_fragment_suffix(id_of(item), prefix) {
            max = max.max(n);
        }
    }
    let next = max + 1;
    if prefix == "AC" {
        format!("AC-{:02}", next)
    } else {
        format!("{}-{:02}", prefix, next)
    }
}

/// Parse the numeric suffix off a `<PREFIX>-NN` / `<PREFIX>NN` id.
/// `AC-03` → `Some(3)`; `Q-02` → `Some(2)`; `S4` → `None` (wrong prefix).
fn parse_fragment_suffix(id: &str, prefix: &str) -> Option<u32> {
    let rest = id.strip_prefix(prefix)?;
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    rest.parse().ok()
}

/// Persist a milestone file, enforce schema, and refresh `plan.json` index.
pub fn write_milestone_synced(ctx: &PlanContext, path: &Path, m: &MilestoneFile) -> Result<()> {
    let cfg = store::try_load_config(ctx)?;
    crate::schema::enforce_milestone_file(&cfg, m)?;
    store::write_milestone(path, m)?;
    sync::sync_plan(ctx)?;
    Ok(())
}

/// M113 S1: hold an advisory process-scoped file lock around the
/// read-modify-write cycle so concurrent `mp` CLI invocations don't
/// clobber each other. The dogfood log entry 2026-07-04
/// `Parallel mp milestone wp|step invocations race and drop writes`
/// is the failure mode this guards.
///
/// Callers reachable from the outer `cmd_milestone` dispatcher
/// already hold the lock (the whole subtree is wrapped via
/// `cmd_milestone` → `cmd_milestone_inner`), so they MUST use
/// [`with_milestone_mut_unlocked`] instead — re-locking here would
/// deadlock on the same thread.
pub fn with_milestone_mut_unlocked<R>(
    ctx: &PlanContext,
    id: &str,
    f: impl FnOnce(&mut MilestoneFile) -> Result<R>,
) -> Result<R> {
    let path = load_milestone_path(ctx, id)?;
    // M124 (M94 ER-1): capture the on-disk raw bytes BEFORE the
    // closure runs so we can compare against the post-mutation
    // serialized form and skip the write when nothing changed.
    //
    // The re-read is unavoidable here because `load_milestone`
    // canonicalizes the on-disk format (key ordering, whitespace) —
    // the post-`f` `MilestoneFile` cannot be compared against the
    // canonicalized in-memory form for byte equality. Trade-off:
    // one extra `read_to_string` per write path. For bulk fan-out
    // with N targets this adds N reads; the bulk paths already pay
    // for a full plan load on entry so the marginal cost is
    // negligible. If profiling shows this matters, lift the raw read
    // into `with_milestone_mut` (the locked variant) or replace the
    // byte check with a structural comparison of just the fields the
    // caller mutated.
    let original_raw = store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES).ok();
    let mut m = store::load_milestone(&path)?;
    let result = f(&mut m)?;
    m.milestone.updated = store::today();
    // M124 (M94 ER-1): idempotent / no-op mutations (caller's `f`
    // didn't change any field) MUST NOT rewrite the file or bump
    // `updated`. Pre-fix every bulk call — even one where every target
    // was already at the desired priority — touched every file and
    // advanced every `updated` timestamp, polluting `mp milestone log`
    // with synthetic no-op entries.
    // M124 review F-02: serialize the PREPARED form (mirroring
    // `store::write_milestone` exactly: clone → `prepare_for_disk` →
    // `to_string_pretty` + trailing `\n`) so the idempotency check
    // compares prepared-vs-on-disk(prepared) bytes. Pre-fix this
    // compared the in-memory form WITHOUT calling `prepare_for_disk`,
    // which happened to be byte-equal only because `WorkPackage.steps`
    // is `#[serde(skip_serializing)]` and every other normalization
    // was symmetric — a fragile invariant that no test asserted. Any
    // future field gaining an asymmetric normalize-on-write would have
    // silently flipped every no-op into a rewrite.
    //
    // M124 review M-2: prepare_for_disk in place rather than cloning.
    // `m` is local to this function and dropped after we either return
    // (no-op path) or pass to `write_milestone_synced` (changed path);
    // the downstream `store::write_milestone` clones for its own
    // prepare_for_disk round, so the in-place mutation here halves the
    // clone count (one instead of two) on the no-op path. On the
    // changed path the clone still happens once, in
    // `write_milestone`. Functionally equivalent to the F-02 fix;
    // cheaper on the common no-op case.
    m.prepare_for_disk();
    let new_raw = serde_json::to_string_pretty(&m)
        .map_err(|e| anyhow::anyhow!("serialize milestone for idempotent write check: {e:#}"))?;
    let new_raw = format!("{new_raw}\n");
    let changed = match &original_raw {
        Some(orig) => orig.as_bytes() != new_raw.as_bytes(),
        None => true,
    };
    if !changed {
        return Ok(result);
    }
    write_milestone_synced(ctx, &path, &m)?;
    Ok(result)
}

pub(crate) fn milestone_path(ctx: &PlanContext, id: &str, slug: &str) -> PathBuf {
    ctx.milestones_dir()
        .join(store::milestone_filename(id, slug))
}

pub fn delete_milestone(ctx: &PlanContext, id: &str, force: bool) -> Result<()> {
    use crate::sync;
    let path = load_milestone_path(ctx, id)?;
    if !force {
        let m = store::load_milestone(&path)?;
        if !matches!(
            m.milestone.execution_status.as_str(),
            "planned" | "deferred"
        ) {
            bail!(
                "cannot delete milestone {id} with status {} (use --force to override)",
                m.milestone.execution_status
            );
        }
    }
    std::fs::remove_file(&path)?;
    sync::sync_plan(ctx)?;
    Ok(())
}

pub fn archive_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    use crate::sync;
    store::archive_milestone(ctx, id)?;
    sync::sync_plan(ctx)?;
    Ok(())
}

pub fn restore_archived_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    use crate::sync;
    store::restore_archived_milestone(ctx, id)?;
    sync::sync_plan(ctx)?;
    Ok(())
}

pub fn purge_archived_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    store::remove_archive_meta_entry(ctx, "milestone", &paths::normalize_milestone_id(id))?;
    store::purge_archived_milestone(ctx, id)?;
    Ok(())
}
// =============================================================================
// M105 S4 (B-41) — strip-dropped-keys utility
// =============================================================================
//
// Historical milestone files in this repo (and likely every 2.0 install)
// still carry empty arrays for keys that M82 dropped from the lean spec
// model. `validate_create_milestone_keys` / `validate_update_milestone_keys`
// already reject these keys on future writes — they're harmless residue,
// but they trip `make verify-lint` (broad-scope) and bloat each file by a
// few bytes.
//
// `strip_dropped_keys_from_path` is the per-file surgery. Stripping all
// dropped keys from the plan is exposed via the `mp edit
// strip-dropped-keys` CLI (see `commands::edit`). Idempotent: re-running
// on a clean file returns `Ok(None)` and never touches the file (no
// rewrite, no extra newline, no whitespace churn).

/// Outcome of `strip_dropped_keys_from_path`. `Some(removed)` lists the
/// keys that were actually removed (in `DROPPED_CEREMONY_KEYS` order);
/// `None` means the file was already clean and was not rewritten.
pub(crate) fn strip_dropped_keys_from_path(path: &Path) -> Result<Option<Vec<String>>> {
    let raw = store::read_text_bounded(path, store::MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read {}", path.display()))?;
    let mut value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            bail!("{}: invalid JSON ({})", path.display(), e);
        }
    };
    let obj = match value.as_object_mut() {
        Some(o) => o,
        None => return Ok(None), // unexpected; treat as no-op rather than crash
    };

    let mut removed: Vec<String> = Vec::new();
    for key in DROPPED_CEREMONY_KEYS {
        if obj.remove(*key).is_some() {
            removed.push((*key).to_string());
        }
    }
    if removed.is_empty() {
        return Ok(None);
    }

    let serialized = serde_json::to_string_pretty(&value)
        .with_context(|| format!("serialize {}", path.display()))?;
    let original_ended_with_newline = raw.ends_with('\n');
    let mut out = serialized;
    if original_ended_with_newline {
        out.push('\n');
    }
    // Create the temporary file beside the destination so persist uses a
    // same-filesystem atomic rename.
    let tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .with_context(|| format!("parent for {}", path.display()))?,
    )?;
    std::io::Write::write_all(&mut tmp.as_file(), out.as_bytes())
        .with_context(|| format!("write tmp for {}", path.display()))?;
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("persist {}: {}", path.display(), e))?;
    Ok(Some(removed))
}

/// Walk every milestone file in the plan and apply
/// `strip_dropped_keys_from_path`. Returns the total count and the
/// per-file breakdown; also reports whether any file was modified (false
/// = idempotent no-op run on an already-clean plan).
pub(crate) fn strip_dropped_keys_in_plan(
    ctx: &PlanContext,
) -> Result<(usize, std::collections::BTreeMap<String, Vec<String>>)> {
    let paths = store::list_milestone_paths(ctx)?;
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut modified = 0usize;
    for p in paths {
        if let Some(removed) = strip_dropped_keys_from_path(&p)? {
            modified += 1;
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                by_file.insert(name.to_string(), removed);
            }
        }
    }
    Ok((modified, by_file))
}

/// M177 S8: clear `deferred_reason` when `deferred == false` and the
/// reason text is non-empty. Returns the previous reason when rewritten,
/// `None` when no-op (already empty, or still deferred).
///
/// Operates on raw JSON (same pattern as [`strip_dropped_keys_from_path`])
/// so partial/legacy fixtures that fail typed decode still get cleaned.
pub(crate) fn strip_deferred_reason_from_path(path: &Path) -> Result<Option<String>> {
    let raw = store::read_text_bounded(path, store::MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read {}", path.display()))?;
    let mut value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => bail!("{}: invalid JSON ({})", path.display(), e),
    };
    let Some(milestone) = value.get_mut("milestone").and_then(|m| m.as_object_mut()) else {
        return Ok(None);
    };
    let deferred = milestone
        .get("deferred")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let reason = milestone
        .get("deferred_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if deferred || reason.is_empty() {
        return Ok(None);
    }
    milestone.insert(
        "deferred_reason".to_string(),
        serde_json::Value::String(String::new()),
    );

    let serialized = serde_json::to_string_pretty(&value)
        .with_context(|| format!("serialize {}", path.display()))?;
    let original_ended_with_newline = raw.ends_with('\n');
    let mut out = serialized;
    if original_ended_with_newline {
        out.push('\n');
    }
    store::atomic_write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(Some(reason))
}

/// Walk every milestone and apply [`strip_deferred_reason_from_path`].
/// When `dry_run` is true, report candidates without writing.
pub(crate) fn strip_deferred_reason_in_plan(
    ctx: &PlanContext,
    dry_run: bool,
) -> Result<(usize, usize, std::collections::BTreeMap<String, String>)> {
    let paths = store::list_milestone_paths(ctx)?;
    let files_scanned = paths.len();
    let mut by_file: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut modified = 0usize;
    for p in paths {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        if dry_run {
            if let Some(old) = deferred_reason_candidate(&p)? {
                modified += 1;
                by_file.insert(name, old);
            }
        } else if let Some(old) = strip_deferred_reason_from_path(&p)? {
            modified += 1;
            by_file.insert(name, old);
        }
    }
    Ok((files_scanned, modified, by_file))
}

fn deferred_reason_candidate(path: &Path) -> Result<Option<String>> {
    let raw = store::read_text_bounded(path, store::MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let Some(milestone) = value.get("milestone").and_then(|m| m.as_object()) else {
        return Ok(None);
    };
    let deferred = milestone
        .get("deferred")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let reason = milestone
        .get("deferred_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if deferred || reason.is_empty() {
        Ok(None)
    } else {
        Ok(Some(reason.to_string()))
    }
}
