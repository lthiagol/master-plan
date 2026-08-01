use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, ShowTarget};
use crate::commands::common::{emit, emit_value, emit_value_mut};
use crate::milestone_health;
use crate::paths::{self, PlanContext};
use crate::reviews;
use crate::store;

/// M112 S2: overlay top-level keys from the raw on-disk JSON onto a typed
/// milestone value so legacy/dropped ceremony fields (e.g. `follow_ups`,
/// `behavior`, `context`) are reachable through `--fields`. The typed
/// struct wins for keys it knows about (`steps`, `acceptance_criteria`,
/// `milestone.id`, etc.); keys the typed struct doesn't model pass
/// through from raw. Used only when `--fields` is supplied — the
/// default (full-shape) response still uses the typed-struct path.
fn merge_typed_and_raw(typed: &mut serde_json::Value, raw: &serde_json::Value) {
    let (Some(t_obj), Some(r_obj)) = (typed.as_object_mut(), raw.as_object()) else {
        return;
    };
    for (key, raw_val) in r_obj {
        if !t_obj.contains_key(key) {
            t_obj.insert(key.clone(), raw_val.clone());
        }
    }
}

pub(crate) fn cmd_show(
    ctx: &PlanContext,
    target: ShowTarget,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    match target {
        ShowTarget::Milestone { id, summary } => {
            if summary {
                let health = milestone_health::build_milestone_health_summary(ctx, &id)?;
                return emit_value(Fmt::Json, &serde_json::to_value(health)?, fields);
            }
            ctx.ensure_plan_exists()?;
            let norm = paths::normalize_milestone_id(&id);
            let path = match paths::find_milestone_file(&ctx.milestones_dir(), &norm) {
                Some(p) => p,
                None => store::archived_milestone_path(ctx, &id)
                    .with_context(|| format!("milestone {norm} not found"))?,
            };
            let m = store::load_milestone(&path)?;
            match format {
                Fmt::Raw => {
                    // Verbatim on-disk JSON — the raw persisted document.
                    let raw = store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)?;
                    println!("{raw}");
                    Ok(())
                }
                _ => {
                    // AC-04: default JSON uses the same serialize path as write_milestone
                    // (the struct itself), not a separate hand-built view.
                    let mut value = serde_json::to_value(&m)?;
                    // M100: during the migration window, derive the legacy
                    // `spec_status` + `execution_status` view from the unified
                    // lifecycle so projection callers using the legacy paths
                    // (`milestone.spec_status`, etc.) keep working. Once the
                    // bulk migration has been applied to every milestone and
                    // the legacy fields are gone, this layer can be removed.
                    value = inject_legacy_status_view(value, &m);
                    // M133 AC-03: surface the durable review conversation
                    // (threaded comments + coordinator/runner hand-offs)
                    // alongside the milestone body. Additive — the existing
                    // review-verdict shape is unchanged, so consumers that
                    // read only `reviews` / `findings` keep working.
                    value = inject_review_trail(ctx, value, &norm)?;

                    // M112 S2: when --fields is supplied, overlay any
                    // top-level fields the raw on-disk JSON carries but the
                    // typed struct doesn't model (legacy/dropped ceremony
                    // keys like `follow_ups`, `behavior`, `context`, plus any
                    // new fields the schema admits). This is an additive
                    // merge: typed-struct values win for keys it knows about
                    // (so `steps`/`acceptance_criteria` from the merged
                    // milestone load surface), extra keys from raw layer on
                    // top, so `--fields behavior` reads back without
                    // "unknown path".
                    if !fields.is_empty() {
                        let raw_text = store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)?;
                        if let Ok(raw_value) = serde_json::from_str::<serde_json::Value>(&raw_text)
                        {
                            merge_typed_and_raw(&mut value, &raw_value);
                            value = inject_legacy_status_view(value, &m);
                            // Re-inject the review trail so a `--fields`
                            // projection that targets `comments` or
                            // `handoffs` can still read them (the
                            // merge_typed_and_raw path runs after the
                            // initial inject and would otherwise overwrite
                            // our injected `comments`/`handoffs` keys).
                            value = inject_review_trail(ctx, value, &norm)?;
                        }
                    }
                    emit_value_mut(Fmt::Json, &mut value, fields)
                }
            }
        }
        ShowTarget::Archived {
            entity_type,
            id,
            kind,
        } => {
            ctx.ensure_plan_exists()?;
            if entity_type == "milestone" {
                let path = store::archived_milestone_path(ctx, &id)?;
                let m = store::load_milestone(&path)?;
                match format {
                    Fmt::Raw => {
                        println!(
                            "{}",
                            store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)?
                        );
                        Ok(())
                    }
                    _ => {
                        let mut value = serde_json::to_value(&m)?;
                        value = inject_legacy_status_view(value, &m);
                        if !fields.is_empty() {
                            let raw_text =
                                store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)?;
                            if let Ok(raw_value) =
                                serde_json::from_str::<serde_json::Value>(&raw_text)
                            {
                                merge_typed_and_raw(&mut value, &raw_value);
                            }
                            let mut merged = inject_legacy_status_view(value, &m);
                            return emit_value_mut(Fmt::Json, &mut merged, fields);
                        }
                        emit(format, &value)
                    }
                }
            } else if entity_type == "track-item" {
                let kind = kind.context("--kind required for track-item")?;
                let track = store::load_track(ctx, &kind)?;
                let item = track
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .context("archived track item not found")?;
                let value = json!({ "kind": kind, "item": item });
                emit_value(format, &value, fields)
            } else {
                bail!("unknown archived type: {entity_type}")
            }
        }
    }
}

/// M100: inject the legacy `spec_status` + `execution_status` view into the
/// serialized milestone JSON. The on-disk milestone after the bulk
/// migration carries ONLY the unified `lifecycle` field; this helper
/// re-derives the legacy fields from the lifecycle value plus orthogonal
/// overlays (blocked/cancelled) so projection callers using
/// `--fields milestone.spec_status` and downstream consumers (gates,
/// `path` engine, raul TUI) keep working.
///
/// The helper stays after the migration window closes because several
/// callers (`gates.rs`, `path_engine.rs`, `hygiene.rs`, `groom.rs`,
/// `raul/tui/render.rs`, plus legacy projection tests) still read the
/// two-string field shape. Removal is gated on a future migration that
/// converts each caller to `effective_lifecycle()` first; until then
/// removing this helper would break `mp show --fields milestone.spec_status`
/// for any migrated milestone.
fn inject_legacy_status_view(
    mut value: serde_json::Value,
    m: &crate::model::MilestoneFile,
) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        if let Some(milestone) = obj.get_mut("milestone").and_then(|v| v.as_object_mut()) {
            if !milestone.contains_key("lifecycle") {
                milestone.insert(
                    "lifecycle".to_string(),
                    serde_json::Value::String(m.effective_lifecycle()),
                );
            }
            // Always populate the legacy view from the model directly so it
            // matches the on-disk state (legacy file or already-migrated).
            let spec = if !m.milestone.spec_status.is_empty() {
                m.milestone.spec_status.clone()
            } else {
                derive_legacy_spec_status(&m.effective_lifecycle())
            };
            let exec = if !m.milestone.execution_status.is_empty() {
                m.milestone.execution_status.clone()
            } else {
                derive_legacy_execution_status(&m.effective_lifecycle(), m)
            };
            milestone.insert("spec_status".to_string(), serde_json::Value::String(spec));
            milestone.insert(
                "execution_status".to_string(),
                serde_json::Value::String(exec),
            );
        }
    }
    value
}

fn derive_legacy_spec_status(lifecycle: &str) -> String {
    match lifecycle {
        "draft" => "draft".to_string(),
        "groomed" => "review".to_string(),
        "approved" => "ready".to_string(),
        "in-progress" => "ready".to_string(),
        "done" => "implemented".to_string(),
        "self-reviewed" => "implemented".to_string(),
        "reviewed" => "implemented".to_string(),
        "complete" => "verified".to_string(),
        "remediation" => "implemented".to_string(),
        other => other.to_string(),
    }
}

fn derive_legacy_execution_status(lifecycle: &str, m: &crate::model::MilestoneFile) -> String {
    if m.milestone.blocked {
        return "blocked".to_string();
    }
    if m.milestone.deferred {
        return "deferred".to_string();
    }
    if m.milestone.cancelled {
        return "cancelled".to_string();
    }
    match lifecycle {
        "draft" | "groomed" | "approved" => "planned".to_string(),
        "in-progress" => "in-progress".to_string(),
        "done" | "self-reviewed" | "reviewed" | "complete" | "remediation" => "done".to_string(),
        _ => "planned".to_string(),
    }
}

/// M133 AC-03: surface the durable review trail (threaded comments +
/// coordinator/runner hand-offs) on `mp show milestone` output. Pulls
/// the latest snapshot from `reviews.json` via the existing
/// `reviews::review_trail` helper and inserts three top-level fields
/// onto the serialized milestone value:
///
/// - `reviews`: review verdicts (newest-first) — already present in
///   reviews.json before M133; surfaced for parity with `mp reviews show`.
/// - `comments`: threaded review comments (oldest-first).
/// - `handoffs`: coordinator/runner hand-off records (oldest-first).
///
/// Errors reading `reviews.json`: a **missing** file is benign (the
/// common case for a fresh plan — `load_reviews` returns an empty
/// `ReviewsFile`) and surfaces as empty arrays. A **corrupt/unreadable**
/// file is a real data-integrity defect and is surfaced via a
/// `review_trail_error` field (mirroring the BF-17 `lanes_error` pattern
/// in `mp status`) rather than silently masked as empty arrays — so an
/// operator notices the corruption instead of seeing an all-zero trail.
fn inject_review_trail(
    ctx: &PlanContext,
    mut value: serde_json::Value,
    milestone_id: &str,
) -> Result<serde_json::Value> {
    let (verdicts, comments, handoffs, review_trail_error) =
        match reviews::review_trail(ctx, milestone_id) {
            Ok((v, c, h)) => (v, c, h, None),
            // A missing reviews.json is the benign fresh-plan case —
            // `load_reviews` returns Ok(default) for it, so this Err branch
            // is reached ONLY for a present-but-corrupt/unreadable file.
            // Surface it (BF-17 pattern) instead of hiding the defect.
            Err(e) => (vec![], vec![], vec![], Some(format!("{e:#}"))),
        };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "reviews".to_string(),
            serde_json::to_value(&verdicts).unwrap_or(serde_json::Value::Array(vec![])),
        );
        obj.insert(
            "comments".to_string(),
            serde_json::to_value(&comments).unwrap_or(serde_json::Value::Array(vec![])),
        );
        obj.insert(
            "handoffs".to_string(),
            serde_json::to_value(&handoffs).unwrap_or(serde_json::Value::Array(vec![])),
        );
        // null when the review file is healthy/missing; a non-empty
        // string when reviews.json exists but failed to read/parse.
        // Kept on the object even when None so consumers can rely on
        // the field's presence (consistent with `reviews`/`comments`).
        obj.insert(
            "review_trail_error".to_string(),
            serde_json::to_value(&review_trail_error).unwrap_or(serde_json::Value::Null),
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MilestoneFile, MilestoneMeta};

    #[test]
    fn derive_legacy_spec_status_for_each_lifecycle() {
        assert_eq!(derive_legacy_spec_status("draft"), "draft");
        assert_eq!(derive_legacy_spec_status("groomed"), "review");
        assert_eq!(derive_legacy_spec_status("approved"), "ready");
        assert_eq!(derive_legacy_spec_status("in-progress"), "ready");
        assert_eq!(derive_legacy_spec_status("done"), "implemented");
        assert_eq!(derive_legacy_spec_status("self-reviewed"), "implemented");
        assert_eq!(derive_legacy_spec_status("reviewed"), "implemented");
        assert_eq!(derive_legacy_spec_status("complete"), "verified");
        assert_eq!(derive_legacy_spec_status("remediation"), "implemented");
    }

    #[test]
    fn derive_legacy_execution_status_overlay_precedence() {
        let mut m = MilestoneFile::default();
        m.milestone.lifecycle = "approved".to_string();
        m.milestone.blocked = true;
        assert_eq!(derive_legacy_execution_status("approved", &m), "blocked");
        m.milestone.blocked = false;
        m.milestone.deferred = true;
        assert_eq!(derive_legacy_execution_status("approved", &m), "deferred");
        m.milestone.deferred = false;
        m.milestone.cancelled = true;
        assert_eq!(derive_legacy_execution_status("approved", &m), "cancelled");
    }

    #[test]
    fn inject_legacy_status_adds_both_fields() {
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "42".into(),
                title: "T".into(),
                slug: "t".into(),
                lifecycle: "complete".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let v = serde_json::to_value(&m).unwrap();
        let out = inject_legacy_status_view(v, &m);
        assert_eq!(out["milestone"]["lifecycle"], "complete");
        assert_eq!(out["milestone"]["spec_status"], "verified");
        assert_eq!(out["milestone"]["execution_status"], "done");
    }
}
