use anyhow::Result;
use serde_json::{json, Value};

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::{emit_value, find_first_pending_track_item};
use crate::execution;
use crate::inbox;
use crate::path_engine;
use crate::paths::PlanContext;
use crate::reviews;
use crate::store;
use crate::validate::validate_plan_with_milestones;

pub(crate) fn cmd_status(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    summary: bool,
) -> Result<()> {
    // Single-load snapshot: lanes / validate / inbox / reviews / lifecycle
    // all share one milestone directory scan (code-review H3 remediation).
    // A corrupt milestone must still surface as `lanes_error` (BF-17) —
    // do not silently empty-out the snapshot on load failure.
    let plan = store::load_plan(ctx).unwrap_or_default();
    let (all_milestones, milestone_load_err) = match store::load_all_milestones(ctx) {
        Ok(ms) => (ms, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };

    let opts = path_engine::LaneOptions { no_ideas: false };
    let lane_report_result = match &milestone_load_err {
        Some(err) => Err(anyhow::anyhow!("{err}")),
        None => path_engine::build_lanes_from(ctx, &plan, &all_milestones, 50, opts),
    };
    let lanes_error = match &lane_report_result {
        Ok(_) => None,
        Err(e) => Some(format!("{e:#}")),
    };
    let lane_report = lane_report_result.ok();
    let archived_count = store::load_archive_meta(ctx)
        .map(|m| m.entries.len() as i64)
        .unwrap_or(0);

    let validate_report = validate_plan_with_milestones(ctx, &all_milestones).ok();
    let validate_ok = validate_report.as_ref().map(|r| r.ok).unwrap_or(false);
    let validate_error_count = validate_report.as_ref().map(|r| r.errors.len());
    let exec_check = execution::execution_check_with(ctx, &plan, &all_milestones, validate_ok).ok();
    let can_handoff = exec_check.as_ref().map(|c| c.can_handoff).unwrap_or(false);
    let blockers = inbox::status_blockers_from(&all_milestones);
    let pending = reviews::pending_reviews_from(ctx, &all_milestones).unwrap_or_default();
    let pending_review_count = pending.len();
    let inbox_report = inbox::build_inbox_from(
        ctx,
        "actionable",
        &all_milestones,
        validate_ok,
        validate_error_count,
        &pending,
    )
    .ok();
    let inbox_count = inbox_report.as_ref().map(|r| r.count).unwrap_or(0);

    let (lanes_json, lane_names) = if let Some(report) = lane_report.as_ref() {
        // M102 R3 (F-12): use serde_json::to_value on the actual
        // LaneSummary struct. The struct's #[serde(rename)] attributes
        // give the wire-format keys directly (no fragile
        // trim_end_matches hack). The skip_serializing_if on
        // total_effort is honored automatically.
        let summary_value = serde_json::to_value(&report.summary).unwrap_or_else(|_| json!({}));
        let lane_names: Vec<String> = report.lanes.iter().map(|l| l.name.clone()).collect();
        (summary_value, lane_names)
    } else {
        // build_lanes failed — fall back to empty lanes so mp status
        // still produces a coherent shape.
        (json!({}), Vec::new())
    };

    // C-2 backcompat: emit the legacy `milestones.{total,
    // by_execution_status, by_spec_status, by_lifecycle}` keys
    // alongside the new `lanes` block. Existing raul consumers
    // (status, explain, onboard, watch, tui/dashboard) read the legacy
    // keys; without this alias they silently zero-fill. The new
    // `lanes` block is the canonical source; legacy keys are derived
    // from the same `build_lanes` LaneReport so they don't drift.
    //
    // M146: `by_lifecycle` is computed from the canonical `lifecycle`
    // field by iterating milestones, not by deriving from lane
    // counts. Lane counts partition milestones by execution stage
    // (blocked/execution/review/grooming/backlog) which doesn't map
    // 1:1 to lifecycle stages — deriving `by_lifecycle` from lanes
    // left most buckets as a permanent 0 and pinned consumers
    // (raul TUI dashboard) to a stale view.
    let _milestones_total = lanes_json
        .as_object()
        .map(|o| o.values().map(|v| v.as_u64().unwrap_or(0)).sum::<u64>() as usize)
        .unwrap_or(0);

    // Walk milestones once and bucket by `effective_lifecycle()`. The
    // overlay booleans (blocked / deferred / cancelled) feed into the
    // bucketing here via `effective_lifecycle` — a blocked
    // `approved` milestone shows up under "approved" in the rollup
    // AND under "blocked" in the lanes block (the source of truth
    // for lane-based views). Counters here are deliberately NOT
    // disjoint: a milestone with `blocked=true` increments both
    // its lifecycle bucket and the blocked lane count (via the
    // blocked overlay), so a `by_lifecycle.total` over all buckets
    // equals the milestone count plus the blocked/deferred/cancelled
    // overlay instances. The `total` key is set to the milestone
    // count (no double-counting), the per-bucket counts show the
    // rollup.
    let mut by_lifecycle_obj = serde_json::Map::new();
    // M196: the executor's end-state bucket was renamed from "done"
    // to "executed". The list below is the canonical lifecycle stages
    // (the same set as `LIFECYCLE_STATES` in `mp-model`).
    let lc_buckets = [
        "draft",
        "groomed",
        "approved",
        "in-progress",
        "executed",
        "self-reviewed",
        "reviewed",
        "complete",
        "remediation",
    ];
    for bucket in &lc_buckets {
        by_lifecycle_obj.insert(bucket.to_string(), json!(0u64));
    }
    let lc_total_usize = all_milestones.len();
    for (_, m) in &all_milestones {
        let lc = m.effective_lifecycle();
        if let Some(v) = by_lifecycle_obj.get_mut(lc.as_str()) {
            if let serde_json::Value::Number(n) = v {
                if let Some(prev) = n.as_u64() {
                    *v = json!(prev + 1);
                }
            }
        }
    }
    let by_lifecycle = Value::Object(by_lifecycle_obj);

    // Legacy rollups: `by_execution_status.in-progress` / `planned` and
    // all of `by_spec_status` are hard-coded zeros post-lifecycle migration.
    // Prefer `lanes` + `by_lifecycle`. Kept for wire backcompat only.
    let by_execution_status = json!({
        "done": lanes_json.get("execution").and_then(|v| v.as_u64()).unwrap_or(0),
        "in-progress": 0u64,
        "planned": 0u64,
    });

    // Legacy track_pending: 0 in the post-kinds-migration world
    // (the kinds merged into backlog). Tweak / bugfix tracks are gone;
    // remaining "track" items live in backlog as kind=bug | kind=tweak.
    let track_pending = 0i64;

    let value = json!({
        "planning_status": plan.project.planning_status,
        "planning_phase": plan.project.planning_phase,
        "lanes": lanes_json,
        "lane_names": lane_names,
        "milestones": {
            "total": lc_total_usize,
            "by_execution_status": by_execution_status,
            "by_spec_status": {
                "verified": 0u64,
                "ready": 0u64,
                "implemented": 0u64,
            },
            "by_lifecycle": by_lifecycle,
        },
        "track_pending": track_pending,
        "annotations_open": store::load_annotations(ctx)
            .map(|a| a.annotations.iter().filter(|x| x.status == "open").count())
            .unwrap_or(0),
        "blockers": blockers,
        "inbox_count": inbox_count,
        "pending_review_count": pending_review_count,
        "archived_count": archived_count,
        "validate_ok": validate_ok,
        "lanes_error": lanes_error,
        "execution": {
            "mode": plan.execution.mode,
            "handoff_at": plan.execution.handoff_at,
            "can_handoff": can_handoff,
        },
        // Schema hint: canonical fields are `lanes` + `milestones.by_lifecycle`.
        "status_schema": "lanes-v1",
    });

    if summary {
        return emit_value(format, &value, fields);
    }

    // Non-summary: also include the suggested-path preview (heavy work).
    let suggested_path = execution::suggested_path_preview(ctx).ok();
    let mut value = value;
    if let Some(sp) = suggested_path {
        if let Ok(obj) = serde_json::to_value(sp) {
            value["suggested_path"] = obj;
        }
    }
    emit_value(format, &value, fields)
}

pub(crate) fn cmd_next_step(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    lane: Option<crate::cli::path::LaneArg>,
    summary: bool,
) -> Result<()> {
    let cfg = store::load_config(ctx);
    let prefer = cfg.next_prefer();

    // M102: --lane reroutes the head read. Without --lane, behavior is the
    // pre-M102 default (execution lane head).
    if let Some(lane_arg) = lane {
        let opts = path_engine::LaneOptions { no_ideas: false };
        let report = path_engine::build_lanes(ctx, 50, opts)?;
        // M102 R3 (F-09 + F-10): look up the lane by NAME (not positional
        // index into report.lanes[0..3]). The lane names are sourced from
        // Lane.name (the source of truth — same string in the wire format
        // and the build_lanes output). Renaming/reordering a lane enum
        // variant doesn't break the resolution.
        let target = report
            .lanes
            .iter()
            .find(|l| l.name == lane_arg.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown lane {:?}; available: {:?}",
                    lane_arg,
                    report.lanes.iter().map(|l| &l.name).collect::<Vec<_>>()
                )
            })?;
        if summary {
            return emit_value(
                format,
                &json!({
                    "lane": target.name.clone(),
                    "item_count": target.item_count,
                    "head": target.head,
                }),
                fields,
            );
        }
        return emit_value(
            format,
            &json!({
                "lane": target.name.clone(),
                "head": target.head,
                "items": &target.items[..target.items.len().min(5)],
            }),
            fields,
        );
    }

    // M102 R3 (F-11): --summary in the legacy branch (no --lane) now
    // returns a per-lane summary block instead of silently ignoring
    // the flag. Falls through to the head-only payload otherwise.
    if summary {
        let opts = path_engine::LaneOptions { no_ideas: false };
        if let Ok(report) = path_engine::build_lanes(ctx, 50, opts) {
            let lanes: Vec<(&str, usize)> = report
                .lanes
                .iter()
                .map(|l| (l.name.as_str(), l.item_count))
                .collect();
            return emit_value(
                format,
                &json!({
                    "head": report.lanes.iter().find_map(|l| {
                        l.head.as_ref().and_then(|h| h.milestone.get("id").and_then(|v| v.as_str().map(String::from)))
                    }),
                    "lanes": lanes.into_iter().collect::<std::collections::HashMap<_, _>>(),
                }),
                fields,
            );
        }
    }

    if prefer != "track" {
        if let Some(action) = path_engine::next_step_action(ctx)? {
            let value = path_engine::next_step_json(&action);
            return emit_value(format, &value, fields);
        }
    }
    if let Some(item) = find_first_pending_track_item(ctx)? {
        return emit_value(format, &item, fields);
    }
    if let Some(action) = path_engine::next_step_action(ctx)? {
        let value = path_engine::next_step_json(&action);
        return emit_value(format, &value, fields);
    }
    let value = json!({ "message": "No actionable steps found" });
    emit_value(format, &value, fields)
}

#[cfg(test)]
mod tests {
    use crate::model::{MilestoneFile, MilestoneMeta};
    use crate::validate::{effective_execution_status, effective_spec_status};

    fn fixture(name: &str, lifecycle: &str, blocked: bool) -> MilestoneFile {
        let mut meta = MilestoneMeta {
            id: "01".into(),
            title: "t".into(),
            slug: name.into(),
            lifecycle: lifecycle.into(),
            ..Default::default()
        };
        meta.blocked = blocked;
        MilestoneFile {
            milestone: meta,
            ..Default::default()
        }
    }

    /// M100 workaround-pass: after bulk migration cleared `spec_status` /
    /// `execution_status`, the legacy rollups collapse to a single empty
    /// bucket. Routing through `effective_spec_status` /
    /// `effective_execution_status` restores meaningful buckets.
    #[test]
    fn effective_helpers_derive_legacy_strings_from_lifecycle() {
        let m = fixture("done-only", "done", false);
        assert_eq!(effective_spec_status(&m), "implemented");
        assert_eq!(effective_execution_status(&m), "done");

        let m = fixture("approved-only", "approved", false);
        assert_eq!(effective_spec_status(&m), "ready");
        assert_eq!(effective_execution_status(&m), "planned");

        let m = fixture("complete-only", "complete", false);
        assert_eq!(effective_spec_status(&m), "verified");
        assert_eq!(effective_execution_status(&m), "done");

        let m = fixture("blocked-overlay", "approved", true);
        assert_eq!(effective_execution_status(&m), "blocked");
    }
}
