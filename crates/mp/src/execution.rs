use std::collections::HashSet;

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::paths::{self, PlanContext};
use crate::plan_gaps;
use crate::store;
use crate::track_kind;
use crate::validate;

#[derive(Debug, Serialize)]
pub struct ExecutionCheckReport {
    pub ok: bool,
    pub mode: String,
    pub planning_status: String,
    pub execution_ready_milestones: Vec<String>,
    pub not_ready: Vec<Value>,
    pub track_pending: i64,
    pub validate_ok: bool,
    pub can_handoff: bool,
    pub warnings: Vec<String>,
}

pub fn execution_check(ctx: &PlanContext) -> Result<ExecutionCheckReport> {
    let plan = store::load_plan(ctx)?;
    let validate_report = validate::validate_plan(ctx)?;
    let milestones = store::load_all_milestones(ctx)?;
    execution_check_with(ctx, &plan, &milestones, validate_report.ok)
}

/// Like [`execution_check`] but reuses a pre-loaded plan/milestone snapshot
/// and a known `validate_ok` bit (avoids re-scanning the plan dir).
pub fn execution_check_with(
    ctx: &PlanContext,
    plan: &crate::model::PlanFile,
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
    validate_ok: bool,
) -> Result<ExecutionCheckReport> {
    let done_ids: HashSet<String> = milestones
        .iter()
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty register as done.
        .filter(|(_, m)| validate::effective_execution_status(m) == "done")
        .map(|(_, m)| paths::normalize_milestone_id(&m.milestone.id))
        .collect();

    let mut execution_ready_milestones = Vec::new();
    let mut not_ready = Vec::new();
    let warnings = Vec::new();

    for (_, m) in milestones {
        if !handoff_candidate(m) {
            continue;
        }
        let id = paths::normalize_milestone_id(&m.milestone.id);
        let (ready, reasons) = plan_gaps::execution_ready(m, &done_ids);
        if ready {
            execution_ready_milestones.push(id);
        } else if !reasons.is_empty() {
            not_ready.push(json!({ "id": id, "reasons": reasons }));
        }
    }

    let mut track_pending = 0i64;
    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(t) = store::load_track(ctx, kind) {
            track_pending += t
                .items
                .iter()
                .filter(|i| i.status == "pending" || i.status == "in-progress")
                .count() as i64;
        }
    }

    let can_handoff = validate_ok && (!execution_ready_milestones.is_empty() || track_pending > 0);

    Ok(ExecutionCheckReport {
        ok: true,
        mode: plan.execution.mode.clone(),
        planning_status: plan.project.planning_status.clone(),
        execution_ready_milestones,
        not_ready,
        track_pending,
        validate_ok,
        can_handoff,
        warnings,
    })
}

pub fn execution_handoff(
    ctx: &PlanContext,
    allow_tracks_only: bool,
    handoff_by: Option<&str>,
) -> Result<serde_json::Value> {
    execution_handoff_impl(ctx, allow_tracks_only, handoff_by, None)
}

pub(crate) fn execution_handoff_in_txn(
    ctx: &PlanContext,
    allow_tracks_only: bool,
    handoff_by: Option<&str>,
    txn: &crate::plan_io::PlanWriteTxn,
) -> Result<serde_json::Value> {
    execution_handoff_impl(ctx, allow_tracks_only, handoff_by, Some(txn))
}

fn execution_handoff_impl(
    ctx: &PlanContext,
    allow_tracks_only: bool,
    handoff_by: Option<&str>,
    txn: Option<&crate::plan_io::PlanWriteTxn>,
) -> Result<serde_json::Value> {
    let check = execution_check(ctx)?;
    if !check.can_handoff {
        bail!(
            "cannot handoff: validate_ok={} execution_ready={} track_pending={}",
            check.validate_ok,
            check.execution_ready_milestones.len(),
            check.track_pending
        );
    }
    if check.execution_ready_milestones.is_empty() && !allow_tracks_only {
        bail!("no execution-ready milestones; use --allow-tracks-only for track-only work");
    }

    let mut plan = store::load_plan(ctx)?;
    let previous_baseline = plan.execution.handoff_baseline.clone();

    plan.execution.mode = "autonomous".to_string();
    plan.execution.handoff_at = store::now_rfc3339();
    plan.execution.handoff_by = handoff_by.unwrap_or("user").to_string();
    if plan.project.planning_status != "in-execution" {
        plan.project.planning_status = "in-execution".to_string();
    }

    let new_baseline = crate::plan_diff::capture_handoff_baseline(ctx, &plan)?;
    let changed_ids =
        crate::plan_diff::changed_milestone_ids_between(&previous_baseline, &new_baseline);
    plan.execution.handoff_changed_milestones = changed_ids.clone();
    plan.execution.handoff_baseline = new_baseline;
    store::write_plan(ctx, &plan)?;

    let by = plan.execution.handoff_by.clone();
    let count = plan.execution.handoff_changed_milestones.len();
    let event = crate::activity::execution_handoff_event(&by, count);
    if let Some(txn) = txn {
        txn.append_activity_best_effort(ctx, event)?;
    } else {
        crate::activity::append_event_best_effort(ctx, event)?;
    }

    Ok(json!({
        "ok": true,
        "mode": plan.execution.mode,
        "planning_status": plan.project.planning_status,
        "handoff_at": plan.execution.handoff_at,
        "handoff_by": plan.execution.handoff_by,
        "changed_milestone_ids": changed_ids,
    }))
}

pub fn execution_pause(ctx: &PlanContext, reason: Option<&str>) -> Result<serde_json::Value> {
    let mut plan = store::load_plan(ctx)?;
    plan.execution.mode = "planning".to_string();
    store::write_plan(ctx, &plan)?;
    Ok(json!({
        "ok": true,
        "mode": "planning",
        "reason": reason.unwrap_or(""),
    }))
}

pub fn execution_status(ctx: &PlanContext) -> Result<serde_json::Value> {
    let plan = store::load_plan(ctx)?;
    let check = execution_check(ctx)?;
    let handoff_at = if plan.execution.handoff_at.is_empty() {
        Value::Null
    } else {
        json!(plan.execution.handoff_at)
    };
    Ok(json!({
        "mode": plan.execution.mode,
        "handoff_at": handoff_at,
        "execution_ready_count": check.execution_ready_milestones.len(),
        "autonomous_allowed": plan.execution.mode == "autonomous",
        "planning_status": plan.project.planning_status,
        "validate_ok": check.validate_ok,
    }))
}

pub fn suggested_path_preview(ctx: &PlanContext) -> Result<Value> {
    let report = crate::path_engine::build_path(ctx, 5)?;
    let preview: Vec<String> = report
        .actions
        .iter()
        .filter_map(|a| {
            let mid = a.milestone.get("display")?.as_str()?;
            let step = a.step.as_ref()?.id.as_str();
            Some(format!("{mid}/{step}"))
        })
        .collect();
    let next_action = report.actions.first().map(|a| {
        json!({
            "type": a.r#type,
            "milestone": a.milestone.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "step": a.step.as_ref().map(|s| s.id.as_str()).unwrap_or(""),
            "display": preview.first().cloned().unwrap_or_default(),
        })
    });
    Ok(json!({
        "strategy": report.strategy,
        "next_action": next_action,
        "preview": preview,
        "blocked_count": report.blocked.len(),
    }))
}

fn handoff_candidate(m: &crate::model::MilestoneFile) -> bool {
    // M100: candidate is a milestone that has reached at least the approved
    // lifecycle state and is not yet terminal.
    let lc = m.effective_lifecycle();
    matches!(lc.as_str(), "approved" | "in-progress")
}
