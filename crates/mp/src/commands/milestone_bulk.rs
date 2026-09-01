use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;

use crate::cli::{BulkCmd, BulkDependsOnAction, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::milestone::{self, format_gate_errors, ApplySpecStatusResult};
use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::store;
use crate::ExitCode;

/// Resolve the union of `--ids` and `--where` targets, normalized, deduped.
/// Returns an error if both are empty.
fn resolve_targets(
    ctx: &PlanContext,
    ids: Option<&[String]>,
    where_filters: &[String],
) -> Result<Vec<String>> {
    let mut targets: BTreeSet<String> = BTreeSet::new();

    if let Some(id_list) = ids {
        for raw in id_list {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            targets.insert(paths::normalize_milestone_id(trimmed));
        }
    }

    if !where_filters.is_empty() {
        let parsed = crate::commands::list::parse_where_filters(where_filters);
        // M124 (M94 ER-4): a bogus --where entry must not abort the
        // whole command when --ids alone carries valid targets. Pre-fix
        // this `bail!` fired unconditionally on empty parsed, killing
        // the entire bulk dispatch (no apply_* ran) even when --ids
        // resolved cleanly. The new contract: warn on stderr so the
        // operator notices the typo, then fall through to --ids-only
        // resolution. The user still gets the per-target apply report
        // (which already records successes/failures) and the command
        // exits with the bulk partial-failure code if any apply failed.
        if parsed.is_empty() {
            // M124 review L-4: check the trimmed-and-skipped form to
            // match the actual filter semantics above. The previous
            // `!v.is_empty()` was true even for `[""]` (slice of one
            // empty string), which then went through trim-and-skip,
            // produced zero targets, and re-bailed at the bottom of
            // resolve_targets — so the user saw both a warning AND a
            // redundant error.
            let ids_have_real_target = ids
                .map(|v| v.iter().any(|s| !s.trim().is_empty()))
                .unwrap_or(false);
            if ids_have_real_target {
                eprintln!(
                    "warning: --where entries did not parse ({}); falling back to --ids targets",
                    where_filters.join(", ")
                );
            } else {
                anyhow::bail!(
                    "no valid --where entries provided (expected field==value or field!=value)"
                );
            }
        } else {
            let milestones = store::load_all_milestones(ctx)?;
            for (_, m) in milestones {
                if parsed
                    .iter()
                    .all(|wf| crate::commands::list::milestone_matches_where(&m, wf))
                {
                    targets.insert(paths::normalize_milestone_id(&m.milestone.id));
                }
            }
        }
    }

    if targets.is_empty() {
        anyhow::bail!("bulk milestone requires at least one target via --ids or --where");
    }

    Ok(targets.into_iter().collect())
}

pub(crate) fn cmd_milestone_bulk(ctx: &PlanContext, cmd: BulkCmd, format: Fmt) -> Result<()> {
    match cmd {
        BulkCmd::SetPriority {
            ids,
            r#where,
            priority,
            dry_run,
        } => {
            if !milestone::VALID_PRIORITIES.contains(&priority.as_str()) {
                anyhow::bail!(
                    "invalid priority: {priority} (expected one of: {})",
                    milestone::VALID_PRIORITIES.join(", ")
                );
            }
            let commit = !dry_run;
            run_bulk(
                ctx,
                format,
                ids.as_deref(),
                &r#where,
                dry_run,
                "set-priority",
                |id| apply_set_priority(ctx, id, &priority, commit),
            )
        }
        BulkCmd::SetSpecStatus {
            ids,
            r#where,
            status,
            dry_run,
        } => {
            if !milestone::SPEC_STATUSES.contains(&status.as_str()) {
                anyhow::bail!(
                    "invalid spec_status: {status} (expected one of: {})",
                    milestone::SPEC_STATUSES.join(", ")
                );
            }
            let commit = !dry_run;
            run_bulk(
                ctx,
                format,
                ids.as_deref(),
                &r#where,
                dry_run,
                "set-spec-status",
                |id| apply_set_spec_status(ctx, id, &status, commit),
            )
        }
        BulkCmd::SetLifecycle {
            ids,
            r#where,
            status,
            dry_run,
        } => {
            // `set-lifecycle` accepts both a `LIFECYCLE_STATES` value and
            // `""` (which `migrate-lifecycle` uses to clear the field
            // before re-deriving). Validation lives in
            // `milestone::set_lifecycle`; we pre-validate here so the
            // bulk error message references the exact offending value
            // (per-id failure messages are less actionable for a 100-milestone
            // batch than a single upfront check).
            if !status.is_empty() && !crate::model::LIFECYCLE_STATES.contains(&status.as_str()) {
                anyhow::bail!(
                    "invalid lifecycle: {status:?} (expected one of: {} or \"\" to reset)",
                    crate::model::LIFECYCLE_STATES.join(", ")
                );
            }
            let commit = !dry_run;
            run_bulk(
                ctx,
                format,
                ids.as_deref(),
                &r#where,
                dry_run,
                "set-lifecycle",
                |id| apply_set_lifecycle(ctx, id, &status, commit),
            )
        }
        BulkCmd::DependsOn { action } => match action {
            BulkDependsOnAction::Add {
                ids,
                r#where,
                depends_on,
                dry_run,
            } => {
                let dep_norm = paths::normalize_milestone_id(&depends_on);
                // F-03: validate --depends-on points to an existing milestone once,
                // not per id. Also load the graph once for cycle checks.
                let graph = milestone::build_depends_on_graph(ctx)?;
                if !graph.contains_key(&dep_norm) {
                    anyhow::bail!(
                        "depends_on target {dep_norm} does not match any milestone in the plan"
                    );
                }
                run_bulk(
                    ctx,
                    format,
                    ids.as_deref(),
                    &r#where,
                    dry_run,
                    "depends-on add",
                    |id| apply_add_depends_on(ctx, id, &dep_norm, &graph, !dry_run),
                )
            }
            BulkDependsOnAction::Remove {
                ids,
                r#where,
                depends_on,
                dry_run,
            } => {
                let dep_norm = paths::normalize_milestone_id(&depends_on);
                // No existence check on remove: dropping a dep that wasn't there
                // is a no-op and should not error. No cycle check either — remove
                // cannot introduce cycles. No graph load needed.
                run_bulk(
                    ctx,
                    format,
                    ids.as_deref(),
                    &r#where,
                    dry_run,
                    "depends-on remove",
                    |id| apply_remove_depends_on(ctx, id, &dep_norm, !dry_run),
                )
            }
        },
        BulkCmd::SetStage {
            ids,
            r#where,
            stage,
            status,
            dry_run,
        } => {
            // Upfront validation: typo in --stage or --status should fail
            // the whole batch loudly instead of silently no-oping per target.
            // The single-id `mp milestone stage set` does the same guard.
            if !crate::model::MP_FLOW_STAGE_KEYS.contains(&stage.as_str()) {
                anyhow::bail!(
                    "invalid stage: {stage:?} (expected one of: {})",
                    crate::model::MP_FLOW_STAGE_KEYS.join(", ")
                );
            }
            if !crate::model::MP_FLOW_STAGE_STATUSES.contains(&status.as_str()) {
                anyhow::bail!(
                    "invalid status: {status:?} (expected one of: {})",
                    crate::model::MP_FLOW_STAGE_STATUSES.join(", ")
                );
            }
            let stage = stage.clone();
            let status = status.clone();
            run_bulk(
                ctx,
                format,
                ids.as_deref(),
                &r#where,
                dry_run,
                "set-stage",
                |id| apply_set_stage(ctx, id, &stage, &status, !dry_run),
            )
        }
    }
}

fn run_bulk<F>(
    ctx: &PlanContext,
    format: Fmt,
    ids: Option<&[String]>,
    where_filters: &[String],
    dry_run: bool,
    operation: &str,
    mut apply: F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<ApplyOutcome>,
{
    let targets = resolve_targets(ctx, ids, where_filters)?;
    let mut results = Vec::with_capacity(targets.len());
    let mut succeeded: usize = 0;
    let mut failed: usize = 0;

    for id in &targets {
        if dry_run {
            // Always report dry-run rows but still run the apply so per-id
            // failures (cycle, gate block) surface in the preview. Count both
            // succeeded and failed so `succeeded + failed == target_count`;
            // the `dry_run` flag carries the "nothing was written" signal.
            match apply(id) {
                Ok(outcome) => {
                    let mut row = json!({
                        "id": id,
                        "ok": outcome.ok,
                        "dry_run": true,
                        "operation": operation,
                    });
                    if let Some(b) = outcome.before {
                        row["before"] = b;
                    }
                    if let Some(a) = outcome.after {
                        row["after"] = a;
                    }
                    if let Some(e) = outcome.error {
                        row["error"] = Value::String(e);
                    }
                    if let Some(r) = outcome.reason {
                        row["reason"] = Value::String(r);
                    }
                    if outcome.ok {
                        succeeded += 1;
                    } else {
                        failed += 1;
                    }
                    results.push(row);
                }
                Err(err) => {
                    results.push(json!({
                        "id": id,
                        "ok": false,
                        "dry_run": true,
                        "operation": operation,
                        "error": format!("{err}"),
                    }));
                    failed += 1;
                }
            }
            continue;
        }
        match apply(id) {
            Ok(outcome) => {
                let mut row = json!({
                    "id": id,
                    "ok": outcome.ok,
                    "operation": operation,
                });
                if let Some(b) = outcome.before {
                    row["before"] = b;
                }
                if let Some(a) = outcome.after {
                    row["after"] = a;
                }
                if let Some(e) = outcome.error {
                    row["error"] = Value::String(e);
                }
                if let Some(r) = outcome.reason {
                    row["reason"] = Value::String(r);
                }
                results.push(row);
                if outcome.ok {
                    succeeded += 1;
                } else {
                    failed += 1;
                }
            }
            Err(err) => {
                results.push(json!({
                    "id": id,
                    "ok": false,
                    "operation": operation,
                    "error": format!("{err}"),
                }));
                failed += 1;
            }
        }
    }

    let target_count = targets.len();
    let payload = json!({
        "ok": failed == 0,
        "operation": operation,
        "dry_run": dry_run,
        "target_count": target_count,
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
    });
    emit(format, &payload)?;
    if !dry_run && failed > 0 {
        // Exit 2 (not anyhow's 1) so callers can distinguish a bulk partial
        // failure from a hard error. Dry-run always exits 0 so scripts can
        // preview without erroring. Return a silent sentinel so `main` can
        // pick the code without re-printing the JSON payload as an error.
        return Err(anyhow::Error::new(ExitCode::partial_failure()));
    }
    Ok(())
}

#[derive(Default)]
struct ApplyOutcome {
    ok: bool,
    before: Option<Value>,
    after: Option<Value>,
    error: Option<String>,
    /// M202 AC-18: free-form marker for skipped rows (e.g.
    /// `reason: cancelled` for cancelled-milestone skips). Rendered
    /// as a top-level `reason` field on the per-id row so operators
    /// can distinguish "skipped on purpose" from "real mutation".
    reason: Option<String>,
}

fn apply_set_priority(
    ctx: &PlanContext,
    id: &str,
    priority: &str,
    commit: bool,
) -> Result<ApplyOutcome> {
    let before = load_field(ctx, id, |m| m.milestone.priority.clone());
    let result = if commit {
        milestone::set_priority(ctx, id, priority)
    } else {
        milestone::set_priority_preview(ctx, id, priority)
    };
    match result {
        Ok(m) => Ok(ApplyOutcome {
            ok: true,
            before: before.map(|v| json!(v)),
            after: Some(json!(m.milestone.priority)),
            error: None,
            reason: None,
        }),
        Err(e) => Ok(ApplyOutcome {
            ok: false,
            before: before.map(|v| json!(v)),
            after: None,
            error: Some(format!("{e}")),
            reason: None,
        }),
    }
}

fn apply_set_spec_status(
    ctx: &PlanContext,
    id: &str,
    status: &str,
    commit: bool,
) -> Result<ApplyOutcome> {
    let before = load_field(ctx, id, |m| m.milestone.spec_status.clone());
    match milestone::apply_spec_status_with_gates(ctx, id, status, commit) {
        Ok(ApplySpecStatusResult::Applied(m)) => Ok(ApplyOutcome {
            ok: true,
            before: before.map(|v| json!(v)),
            after: Some(json!(m.milestone.spec_status)),
            error: None,
            reason: None,
        }),
        Ok(ApplySpecStatusResult::Blocked { gate_errors, .. }) => Ok(ApplyOutcome {
            ok: false,
            before: before.map(|v| json!(v)),
            after: None,
            error: Some(format_gate_errors(&gate_errors)),
            reason: None,
        }),
        Err(e) => Ok(ApplyOutcome {
            ok: false,
            before: before.map(|v| json!(v)),
            after: None,
            error: Some(format!("{e}")),
            reason: None,
        }),
    }
}

fn apply_add_depends_on(
    ctx: &PlanContext,
    id: &str,
    dep: &str,
    graph: &std::collections::HashMap<String, Vec<String>>,
    commit: bool,
) -> Result<ApplyOutcome> {
    let before_vec = load_depends_on(ctx, id);
    let before_value = before_vec.clone().map(|v| json!(v));
    // Idempotent: if already present, no-op success.
    if let Some(existing) = &before_vec {
        if existing
            .iter()
            .any(|d| paths::normalize_milestone_id(d) == dep)
        {
            return Ok(ApplyOutcome {
                ok: true,
                before: before_value.clone(),
                after: before_value,
                error: None,
                reason: None,
            });
        }
    }
    let mut prospective = before_vec.clone().unwrap_or_default();
    prospective.push(dep.to_string());
    if milestone::depends_on_creates_cycle_in_graph(graph, id, &prospective) {
        return Ok(ApplyOutcome {
            ok: false,
            before: before_value,
            after: None,
            error: Some(format!(
                "adding depends_on={dep} on {id} would create a cycle"
            )),
            reason: None,
        });
    }
    match milestone::add_depends_on_with_graph(ctx, id, dep, commit) {
        Ok(m) => Ok(ApplyOutcome {
            ok: true,
            before: before_value,
            after: Some(json!(m.milestone.depends_on)),
            error: None,
            reason: None,
        }),
        Err(e) => Ok(ApplyOutcome {
            ok: false,
            before: before_vec.map(|v| json!(v)),
            after: None,
            error: Some(format!("{e}")),
            reason: None,
        }),
    }
}

fn apply_remove_depends_on(
    ctx: &PlanContext,
    id: &str,
    dep: &str,
    commit: bool,
) -> Result<ApplyOutcome> {
    let before_vec = load_depends_on(ctx, id);
    let before_value = before_vec.clone().map(|v| json!(v));
    match milestone::remove_depends_on(ctx, id, dep, commit) {
        Ok(m) => Ok(ApplyOutcome {
            ok: true,
            before: before_value,
            after: Some(json!(m.milestone.depends_on)),
            error: None,
            reason: None,
        }),
        Err(e) => Ok(ApplyOutcome {
            ok: false,
            before: before_value,
            after: None,
            error: Some(format!("{e}")),
            reason: None,
        }),
    }
}

fn load_depends_on(ctx: &PlanContext, id: &str) -> Option<Vec<String>> {
    milestone::load_milestone_by_id(ctx, id)
        .ok()
        .map(|m| m.milestone.depends_on.clone())
}

fn load_field<T, F>(ctx: &PlanContext, id: &str, extract: F) -> Option<Value>
where
    T: Into<Value>,
    F: FnOnce(&MilestoneFile) -> T,
{
    milestone::load_milestone_by_id(ctx, id)
        .ok()
        .map(|m| extract(&m).into())
}

/// Apply `set-lifecycle` to one milestone — bulk variant. The commit
/// flag is held by the caller; this closure keeps the per-id
/// `ApplyOutcome` shape consistent with the other bulk mutators
/// (`before`, `after`, `error`).
fn apply_set_lifecycle(
    ctx: &PlanContext,
    id: &str,
    status: &str,
    commit: bool,
) -> Result<ApplyOutcome> {
    let before = load_field(ctx, id, |m| m.milestone.lifecycle.clone());
    match milestone::set_lifecycle(ctx, id, status, commit) {
        Ok(m) => Ok(ApplyOutcome {
            ok: true,
            before: before.map(|v| json!(v)),
            after: Some(json!(m.milestone.lifecycle)),
            error: None,
            reason: None,
        }),
        Err(e) => Ok(ApplyOutcome {
            ok: false,
            before: before.map(|v| json!(v)),
            after: None,
            error: Some(format!("{e}")),
            reason: None,
        }),
    }
}

/// M202: apply a single `mp milestone stage set <id> <stage> <status>`
/// to one milestone. Cancelled milestones are skipped (no-op) and the
/// per-id result lists `reason: cancelled` — mirrors the AC-18
/// contract that bulk-set-stage must not disturb terminal-cancelled
/// state. The single-id CLI has no equivalent skip (the operator can
/// see the milestone is cancelled and choose), so this is bulk-only
/// behaviour.
fn apply_set_stage(
    ctx: &PlanContext,
    id: &str,
    stage: &str,
    status: &str,
    commit: bool,
) -> Result<ApplyOutcome> {
    let path = match milestone::load_milestone_path(ctx, id) {
        Ok(p) => p,
        Err(e) => {
            return Ok(ApplyOutcome {
                ok: false,
                before: None,
                after: None,
                error: Some(format!("{e}")),
                reason: None,
            });
        }
    };
    let mut m = store::load_milestone(&path)?;
    if m.milestone.cancelled {
        // AC-18: cancelled milestones are skipped (no mutation). The
        // row carries `reason: "cancelled"` so the operator can
        // distinguish the skip from a real mutation without comparing
        // `before` to `after`.
        return Ok(ApplyOutcome {
            ok: true,
            before: Some(json!({ "cancelled": true })),
            after: Some(json!({ "cancelled": true })),
            error: None,
            reason: Some("cancelled".to_string()),
        });
    }
    let before_status = m
        .milestone
        .flow_stages
        .get(stage)
        .map(|s| s.status.clone())
        .unwrap_or_default();
    if !commit {
        return Ok(ApplyOutcome {
            ok: true,
            before: Some(json!({ "stage": stage, "status": before_status })),
            after: Some(json!({ "stage": stage, "status": status })),
            error: None,
            reason: None,
        });
    }
    m.milestone.flow_stages.insert(
        stage.to_string(),
        crate::model::FlowStage {
            status: status.to_string(),
            at: Some(crate::store::now_rfc3339()),
        },
    );
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(ApplyOutcome {
        ok: true,
        before: Some(json!({ "stage": stage, "status": before_status })),
        after: Some(json!({ "stage": stage, "status": status })),
        error: None,
        reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_outcome_defaults_to_zero_value() {
        let o = ApplyOutcome::default();
        assert!(!o.ok);
        assert!(o.before.is_none());
        assert!(o.after.is_none());
        assert!(o.error.is_none());
    }
}
