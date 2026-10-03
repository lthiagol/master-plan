//! Fragment mutators + readers for the `mp` plan model.
//!
//! Every wrapper here calls into one of `mp::milestone::*`,
//! `mp::step::*`, `mp::wp::*`, `mp::reviews::*`, `mp::session::*`,
//! `mp::plan_diff::*`, `mp::milestone_trace::*`,
//! `mp::execution_report::*`, and returns the JSON-shaped
//! `serde_json::Value` so callers can assert on the same shape the
//! CLI emits.
//!
//! **Why wrappers, not just direct `mp::*` calls in tests:** the
//! wrapper layer keeps each call site one line and pins the JSON shape
//! the CLI claims, which the `lib_api_parity` test guards against
//! drift.

use anyhow::Result;
use mp::milestone::{
    approve_milestone as mp_approve_milestone, complete_milestone as mp_complete_milestone,
    create_milestone as mp_create_milestone, criterion_list, criterion_pass as mp_criterion_pass,
    criterion_show as mp_criterion_show, criterion_update as mp_criterion_update,
    CreateMilestoneInput,
};
use mp::paths::PlanContext;
use mp::step::{show_step as mp_show_step, AddStepInput, UpdateStepInput};
use mp::wp::AddWpInput;
use serde_json::{json, Value};

// =============================================================================
// Milestone acceptance-criterion fragments (M93)
// =============================================================================

/// `mp milestone ac show <id> <AC-id> --format json`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "show",
/// "<id>", "<AC-id>", "--format", "json"])`.
pub fn milestone_ac_show(ctx: &PlanContext, id: &str, ac_id: &str) -> Result<Value> {
    mp_criterion_show(ctx, id, ac_id)
}

/// `mp milestone ac list <id> --format json`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "list",
/// "<id>", "--format", "json"])`.
pub fn milestone_ac_list(ctx: &PlanContext, id: &str) -> Result<Value> {
    criterion_list(ctx, id)
}

/// `mp milestone ac pass <id> <AC-id> --evidence "..."`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "pass",
/// "<id>", "<AC-id>", "--evidence", "...", "--format", "json"])`.
pub fn milestone_ac_pass(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    evidence: &str,
) -> Result<Value> {
    let ac = mp_criterion_pass(ctx, id, ac_id, Some(evidence.to_string()))?;
    Ok(serde_json::to_value(ac)?)
}

/// `mp milestone ac fail <id> <AC-id> [--reason "..."]`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "fail",
/// "<id>", "<AC-id>", "--format", "json"])`.
pub fn milestone_ac_fail(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    reason: Option<&str>,
) -> Result<Value> {
    let ac = mp::milestone::criterion_fail(ctx, id, ac_id, reason.map(|s| s.to_string()))?;
    Ok(serde_json::to_value(ac)?)
}

/// `mp milestone ac update <id> <AC-id> --description "..."`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "update",
/// "<id>", "<AC-id>", "--description", "...", "--format", "json"])`.
pub fn milestone_ac_update_description(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    description: &str,
) -> Result<Value> {
    mp_criterion_update(ctx, id, ac_id, Some(description.to_string()), None, None)
}

/// `mp milestone ac update <id> <AC-id>` with optional fields.
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "update", …])`.
pub fn milestone_ac_update(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    description: Option<&str>,
    verification: Option<&str>,
    evidence: Option<&str>,
) -> Result<Value> {
    mp_criterion_update(
        ctx,
        id,
        ac_id,
        description.map(|s| s.to_string()),
        verification.map(|s| s.to_string()),
        evidence.map(|s| s.to_string()),
    )
}

/// `mp milestone ac add <id> --description "..." --verification "..."`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "add", …])`
/// (also accepts the legacy `milestone criterion add` alias).
pub fn milestone_ac_add(
    ctx: &PlanContext,
    id: &str,
    description: &str,
    verification: &str,
) -> Result<Value> {
    let ac = mp::milestone::criterion_add(ctx, id, description, verification)?;
    Ok(serde_json::to_value(ac)?)
}

/// `mp milestone ac remove <id> <AC-id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "ac", "remove",
/// "<id>", "<AC-id>", "--format", "json"])`.
pub fn milestone_ac_remove(ctx: &PlanContext, id: &str, ac_id: &str) -> Result<Value> {
    mp::milestone::criterion_remove(ctx, id, ac_id)
}

// =============================================================================
// Step fragments (M93)
// =============================================================================

/// `mp step show <mid> <step-id> --format json` / `mp milestone step show`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "show",
/// "<mid>", "<step-id>", "--format", "json"])`.
pub fn step_show(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Value> {
    mp_show_step(ctx, milestone_id, step_id)
}

/// `mp step list <mid>` — project `steps` from the milestone document.
///
/// **Subprocess equivalent:** `env.run(&["show", "milestone", "<mid>",
/// "--format", "json"])` then read `.steps` (or the CLI's step-list shape).
pub fn step_list(ctx: &PlanContext, milestone_id: &str) -> Result<Value> {
    let m = mp::milestone::load_milestone_by_id(ctx, milestone_id)?;
    Ok(json!(m.steps))
}

/// `mp milestone step add <mid> --wp WP1 --action "..." …`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "add", …])`.
pub fn step_add(ctx: &PlanContext, milestone_id: &str, input: AddStepInput) -> Result<Value> {
    let step = mp::step::add_step(ctx, milestone_id, input)?;
    Ok(serde_json::to_value(step)?)
}

/// Convenience: add a step with the common CLI flag set.
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "add", mid,
/// "--wp", wp, "--action", action, "--tests", tests, …])`.
pub fn step_add_simple(
    ctx: &PlanContext,
    milestone_id: &str,
    wp: &str,
    action: &str,
    tests: &str,
) -> Result<Value> {
    step_add(
        ctx,
        milestone_id,
        AddStepInput {
            wp: wp.to_string(),
            id: None,
            after: None,
            action: action.to_string(),
            files: vec![],
            tests: tests.to_string(),
            done_when: String::new(),
            covers_ac: vec![],
        },
    )
}

/// `mp milestone step update <mid> <step-id> …`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "update", …])`.
pub fn step_update(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    input: UpdateStepInput,
) -> Result<Value> {
    let step = mp::step::update_step(ctx, milestone_id, step_id, input)?;
    Ok(serde_json::to_value(step)?)
}

/// `mp milestone step remove <mid> <step-id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "remove", …])`.
pub fn step_remove(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Value> {
    mp::step::remove_step(ctx, milestone_id, step_id)
}

/// `mp milestone step set-status <mid> <step-id> <status>` /
/// `mp milestone step done <mid> <step-id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "done"|"set-status", …])`.
pub fn step_set_status(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    status: &str,
) -> Result<Value> {
    let step = mp::step::set_step_status(ctx, milestone_id, step_id, status)?;
    Ok(serde_json::to_value(step)?)
}

/// `mp milestone step done <mid> <step-id>`
pub fn step_done(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Value> {
    step_set_status(ctx, milestone_id, step_id, "done")
}

/// `mp milestone step split <mid> <step-id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "split", …])`.
pub fn step_split(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Value> {
    let steps = mp::step::split_step(ctx, milestone_id, step_id)?;
    Ok(serde_json::to_value(steps)?)
}

/// `mp milestone step claim <mid> <step-id> --by <who>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "claim", …])`.
pub fn step_claim(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    claimed_by: &str,
    lease: Option<&str>,
) -> Result<Value> {
    let step = mp::step_claim::claim_step(ctx, milestone_id, step_id, claimed_by, lease)?;
    Ok(serde_json::to_value(step)?)
}

/// `mp milestone step release <mid> <step-id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "step", "release", …])`.
pub fn step_release(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Value> {
    let step = mp::step_claim::release_step(ctx, milestone_id, step_id)?;
    Ok(serde_json::to_value(step)?)
}

// =============================================================================
// Work packages
// =============================================================================

/// `mp milestone wp add <mid> --name "..." [--id WP1] [--goal "..."]`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "wp", "add", …])`.
pub fn wp_add(
    ctx: &PlanContext,
    milestone_id: &str,
    name: &str,
    id: Option<&str>,
    goal: &str,
    rollback: &str,
) -> Result<Value> {
    let wp = mp::wp::add_work_package(
        ctx,
        milestone_id,
        AddWpInput {
            id: id.map(|s| s.to_string()),
            name: name.to_string(),
            goal: goal.to_string(),
            rollback: rollback.to_string(),
        },
    )?;
    Ok(serde_json::to_value(wp)?)
}

/// `mp milestone wp update <mid> <wp-id> …`
pub fn wp_update(
    ctx: &PlanContext,
    milestone_id: &str,
    wp_id: &str,
    name: Option<&str>,
    goal: Option<&str>,
    rollback: Option<&str>,
) -> Result<Value> {
    mp::wp::wp_update(
        ctx,
        milestone_id,
        wp_id,
        name.map(|s| s.to_string()),
        goal.map(|s| s.to_string()),
        rollback.map(|s| s.to_string()),
    )
}

/// `mp milestone wp remove <mid> <wp-id>`
pub fn wp_remove(ctx: &PlanContext, milestone_id: &str, wp_id: &str) -> Result<Value> {
    mp::wp::remove_work_package(ctx, milestone_id, wp_id)
}

// =============================================================================
// Read-only "show me the milestone"
// =============================================================================

/// `mp show milestone <id> --format json`
///
/// **Subprocess equivalent:** `env.run(&["show", "milestone", "<id>",
/// "--format", "json"])`.
pub fn show_milestone(ctx: &PlanContext, id: &str) -> Result<Value> {
    let m = mp::milestone::load_milestone_by_id(ctx, id)?;
    Ok(serde_json::to_value(m)?)
}

// =============================================================================
// Milestone mutators
// =============================================================================

/// `mp milestone create --json '{...}' --format json` — typed input.
///
/// **Subprocess equivalent:** `env.run(&["milestone", "create",
/// "--json", "<json>", "--format", "json"])`.
pub fn milestone_create(ctx: &PlanContext, input: CreateMilestoneInput) -> Result<Value> {
    let m = mp_create_milestone(ctx, input)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone create --json '<raw>'` — parse CLI JSON body.
///
/// **Subprocess equivalent:** `env.run(&["milestone", "create",
/// "--json", raw, "--format", "json"])`.
pub fn milestone_create_json(ctx: &PlanContext, raw_json: &str) -> Result<Value> {
    let input = mp::milestone::read_create_input(None, None, Some(raw_json))?;
    milestone_create(ctx, input)
}

/// `mp milestone approve <id> --format json`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "approve",
/// "<id>", "--format", "json"])`.
pub fn milestone_approve(ctx: &PlanContext, id: &str) -> Result<Value> {
    let m = mp_approve_milestone(ctx, id)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone complete <id> --evidence "..." --format json`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "complete",
/// "<id>", "--evidence", "...", "--format", "json"])`.
/// M196: `skip_review` is the recorded-debt escape hatch for the
/// review gate. Library callers that want to reach terminal `complete`
/// pass `true` here (which writes `[skip-review]` into evidence).
/// Library callers that want the gate-enforced default pass `false`.
pub fn milestone_complete(
    ctx: &PlanContext,
    id: &str,
    evidence: &str,
    skip_review: bool,
) -> Result<Value> {
    let m = mp_complete_milestone(ctx, id, Some(evidence.to_string()), None, skip_review)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone set-status <id> <status>` (execution_status).
///
/// **Subprocess equivalent:** `env.run(&["milestone", "set-status",
/// "<id>", "<status>", "--format", "json"])`.
pub fn milestone_set_status(ctx: &PlanContext, id: &str, status: &str) -> Result<Value> {
    let m = mp::milestone::set_execution_status(ctx, id, status)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone set-spec-status <id> <status>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "set-spec-status",
/// "<id>", "<status>", "--format", "json"])`.
pub fn milestone_set_spec_status(ctx: &PlanContext, id: &str, status: &str) -> Result<Value> {
    let m = mp::milestone::apply_spec_status(ctx, id, status)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone set-priority <id> <priority>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "set-priority",
/// "<id>", "<priority>", "--format", "json"])`.
pub fn milestone_set_priority(ctx: &PlanContext, id: &str, priority: &str) -> Result<Value> {
    let m = mp::milestone::set_priority(ctx, id, priority)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone depends-on add <id> <dep>`
pub fn milestone_depends_on_add(ctx: &PlanContext, id: &str, dep: &str) -> Result<Value> {
    let m = mp::milestone::add_depends_on(ctx, id, dep, true)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone depends-on remove <id> <dep>`
pub fn milestone_depends_on_remove(ctx: &PlanContext, id: &str, dep: &str) -> Result<Value> {
    let m = mp::milestone::remove_depends_on(ctx, id, dep, true)?;
    Ok(serde_json::to_value(m)?)
}

/// `mp milestone archive <id>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "archive", "<id>"])`.
pub fn milestone_archive(ctx: &PlanContext, id: &str) -> Result<Value> {
    mp::milestone::archive_milestone(ctx, id)?;
    Ok(json!({ "ok": true, "id": id, "archived": true }))
}

/// `mp milestone restore <id>` (from archive)
pub fn milestone_restore(ctx: &PlanContext, id: &str) -> Result<Value> {
    mp::milestone::restore_archived_milestone(ctx, id)?;
    Ok(json!({ "ok": true, "id": id, "restored": true }))
}

/// `mp milestone purge <id>` (archived)
pub fn milestone_purge(ctx: &PlanContext, id: &str) -> Result<Value> {
    mp::milestone::purge_archived_milestone(ctx, id)?;
    Ok(json!({ "ok": true, "id": id, "purged": true }))
}

/// `mp milestone create --from-handoff <path>`
pub fn milestone_from_handoff(ctx: &PlanContext, handoff_path: &str) -> Result<Value> {
    mp::milestone::create_from_handoff(ctx, handoff_path)
}

/// `mp milestone decompose <id> [--work-packages N]`
pub fn milestone_decompose(
    ctx: &PlanContext,
    id: &str,
    work_packages: Option<u32>,
) -> Result<Value> {
    let report = mp::plan_gaps::decompose_milestone(ctx, id, work_packages)?;
    Ok(serde_json::to_value(report)?)
}

/// `mp milestone update <id> --json '...'`
pub fn milestone_update_json(ctx: &PlanContext, id: &str, raw_json: &str) -> Result<Value> {
    let input = mp::milestone::read_update_input(None, Some(raw_json), false, false)?;
    let m = mp::milestone::update_milestone(ctx, id, input, None)?;
    Ok(serde_json::to_value(m)?)
}

// =============================================================================
// Bulk (ids-only; mirrors CLI report shape for common cases)
// =============================================================================

/// `mp milestone bulk set-priority --ids a,b -- <priority>`
///
/// **Subprocess equivalent:** `env.run(&["milestone", "bulk", "set-priority",
/// "--ids", "…", priority, "--format", "json"])`.
pub fn bulk_set_priority(
    ctx: &PlanContext,
    ids: &[&str],
    priority: &str,
    dry_run: bool,
) -> Result<Value> {
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for id in ids {
        let before = mp::milestone::load_milestone_by_id(ctx, id)
            .ok()
            .map(|m| json!(m.milestone.priority));
        let result = if dry_run {
            mp::milestone::set_priority_preview(ctx, id, priority)
        } else {
            mp::milestone::set_priority(ctx, id, priority)
        };
        match result {
            Ok(m) => {
                let mut row = json!({
                    "id": id,
                    "ok": true,
                    "operation": "set-priority",
                    "after": m.milestone.priority,
                });
                if dry_run {
                    row["dry_run"] = json!(true);
                }
                if let Some(b) = before {
                    row["before"] = b;
                }
                results.push(row);
                succeeded += 1;
            }
            Err(e) => {
                let mut row = json!({
                    "id": id,
                    "ok": false,
                    "operation": "set-priority",
                    "error": format!("{e}"),
                });
                if dry_run {
                    row["dry_run"] = json!(true);
                }
                if let Some(b) = before {
                    row["before"] = b;
                }
                results.push(row);
                failed += 1;
            }
        }
    }
    Ok(json!({
        "ok": failed == 0,
        "operation": "set-priority",
        "dry_run": dry_run,
        "target_count": ids.len(),
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
    }))
}

/// `mp milestone bulk set-spec-status --ids a,b -- <status>`
pub fn bulk_set_spec_status(
    ctx: &PlanContext,
    ids: &[&str],
    status: &str,
    dry_run: bool,
) -> Result<Value> {
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for id in ids {
        match mp::milestone::apply_spec_status_with_gates(ctx, id, status, !dry_run) {
            Ok(mp::milestone::ApplySpecStatusResult::Applied(m)) => {
                let mut row = json!({
                    "id": id,
                    "ok": true,
                    "operation": "set-spec-status",
                    "after": m.milestone.spec_status,
                });
                if dry_run {
                    row["dry_run"] = json!(true);
                }
                results.push(row);
                succeeded += 1;
            }
            Ok(mp::milestone::ApplySpecStatusResult::Blocked { gate_errors, .. }) => {
                let mut row = json!({
                    "id": id,
                    "ok": false,
                    "operation": "set-spec-status",
                    "error": format!("{gate_errors:?}"),
                });
                if dry_run {
                    row["dry_run"] = json!(true);
                }
                results.push(row);
                failed += 1;
            }
            Err(e) => {
                let mut row = json!({
                    "id": id,
                    "ok": false,
                    "operation": "set-spec-status",
                    "error": format!("{e}"),
                });
                if dry_run {
                    row["dry_run"] = json!(true);
                }
                results.push(row);
                failed += 1;
            }
        }
    }
    Ok(json!({
        "ok": failed == 0,
        "operation": "set-spec-status",
        "dry_run": dry_run,
        "target_count": ids.len(),
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
    }))
}

// =============================================================================
// Reviews / findings
// =============================================================================

/// `mp reviews finding add <mid> --severity … --category … --description …`
pub fn finding_add(
    ctx: &PlanContext,
    milestone_id: &str,
    severity: &str,
    category: &str,
    description: &str,
    author: Option<&str>,
) -> Result<Value> {
    let f = mp::reviews::add_finding(ctx, milestone_id, severity, category, description, author)?;
    Ok(serde_json::to_value(f)?)
}

/// `mp reviews finding resolve <mid> <F-id>`
pub fn finding_resolve(
    ctx: &PlanContext,
    milestone_id: &str,
    finding_id: &str,
    commit: Option<&str>,
) -> Result<Value> {
    let f = mp::reviews::resolve_finding(ctx, milestone_id, finding_id, commit)?;
    Ok(serde_json::to_value(f)?)
}

/// `mp reviews finding list <mid>`
pub fn finding_list(ctx: &PlanContext, milestone_id: &str) -> Result<Value> {
    let list = mp::reviews::list_findings(ctx, milestone_id, false)?;
    Ok(serde_json::to_value(list)?)
}

/// `mp reviews comment add <mid> --author … --body …`
pub fn review_comment_add(
    ctx: &PlanContext,
    milestone_id: &str,
    author: &str,
    body: &str,
) -> Result<Value> {
    let c = mp::reviews::add_comment(ctx, milestone_id, author, body, None, None, None)?;
    Ok(serde_json::to_value(c)?)
}

/// `mp reviews show <mid>` trail (verdicts + comments + handoffs).
pub fn review_trail(ctx: &PlanContext, milestone_id: &str) -> Result<Value> {
    let (verdicts, comments, handoffs) = mp::reviews::review_trail(ctx, milestone_id)?;
    Ok(json!({
        "verdicts": verdicts,
        "comments": comments,
        "handoffs": handoffs,
    }))
}

// =============================================================================
// Trace / verify / execution report
// =============================================================================

/// `mp milestone trace <id>`
pub fn milestone_trace(ctx: &PlanContext, id: &str) -> Result<Value> {
    let t = mp::milestone_trace::milestone_trace(ctx, id)?;
    Ok(serde_json::to_value(t)?)
}

/// `mp execution report <id>` / build_execution_report
pub fn execution_report(ctx: &PlanContext, id: &str) -> Result<Value> {
    let r = mp::execution_report::build_execution_report(ctx, id)?;
    Ok(serde_json::to_value(r)?)
}

/// `mp plan infer-deps` / step depends inference
pub fn infer_depends_on_steps(ctx: &PlanContext, milestone_id: &str) -> Result<Value> {
    mp::step::infer_depends_on_steps(ctx, milestone_id)
}

// =============================================================================
// Session / plan diff (domain-backed)
// =============================================================================

/// `mp session start --branch …`
pub fn session_start(
    ctx: &PlanContext,
    branch: Option<&str>,
    title: Option<&str>,
) -> Result<Value> {
    mp::session::session_start(ctx, branch, title)
}

/// `mp session focus <id>`
pub fn session_focus(ctx: &PlanContext, session_id: &str) -> Result<Value> {
    mp::session::session_focus(ctx, session_id)
}

/// `mp session unfocus`
pub fn session_unfocus(ctx: &PlanContext) -> Result<Value> {
    mp::session::session_unfocus(ctx)
}

/// `mp plan diff …` — domain plan_diff with default options where possible.
pub fn plan_diff(ctx: &PlanContext, opts: mp::plan_diff::PlanDiffOptions) -> Result<Value> {
    let r = mp::plan_diff::plan_diff(ctx, opts)?;
    Ok(serde_json::to_value(r)?)
}
