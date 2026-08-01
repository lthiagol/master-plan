//! In-process test surface for `mp` integration tests.
//!
//! `lib_api` is the **in-process** alternative to spawning the `mp`
//! binary via `TestEnv::run`. Each public function maps 1:1 to a CLI
//! command's JSON-output contract; tests can call them directly from
//! the test process and skip the subprocess spawn (~50 ms saved per
//! call).
//!
//! Background: milestones **M162** / **M175**
//! (`master-plan/milestones/162-*.json`, `175-*.json`).
//! Taxonomy / decision tree: [`docs/concepts/03 - Testing/test-taxonomy.md`](../../../docs/concepts/03%20-%20Testing/test-taxonomy.md).
//!
//! ## When to use `lib_api` vs `env.run`
//!
//! - **Use `lib_api`** when the test asserts on the JSON shape of a
//!   read-only command, or a fragment read/write that goes through
//!   `mp::milestone::*` / `mp::step::*` / `mp::validate::*`.
//! - **Use `env.run`** for install / uninstall / doctor / watch / TUI /
//!   init / end-to-end smoke. Those categories MUST stay subprocess —
//!   see the taxonomy doc for the full list and the rationale.
//!
//! ## Surface
//!
//! All public wrappers take a `&PlanContext` (explicit `project_root` +
//! `plan_dir`). The four `ctx_for_*` helpers accept a `&Path` and
//! auto-derive a `PlanContext` via `discover`. Return types mirror the
//! CLI's stdout: `Result<serde_json::Value>` for both read-only and
//! mutator commands. No wrapper returns `Result<()>` — every mutator
//! returns the updated fragment as JSON.
//!
//! ## Parity guard
//!
//! `crates/mp/tests/lib_api_parity.rs` runs each wrapper side-by-side
//! with `env.run` on the same fixture and asserts key-set / value-type
//! shape parity. If a wrapper drifts from the CLI, that parity test
//! fails before the wrapper gets merged.

#![allow(dead_code)] // not every test uses every wrapper

use std::path::Path;

use anyhow::{Context, Result};
use mp::milestone::{
    approve_milestone as mp_approve_milestone, complete_milestone as mp_complete_milestone,
    create_milestone as mp_create_milestone, criterion_list, criterion_pass as mp_criterion_pass,
    criterion_show as mp_criterion_show, criterion_update as mp_criterion_update,
    CreateMilestoneInput,
};
use mp::paths::PlanContext;
use mp::step::{show_step as mp_show_step, AddStepInput, UpdateStepInput};
use mp::validate::validate_plan;
use mp::wp::AddWpInput;
use serde_json::{json, Value};

// =============================================================================
// PlanContext helpers
// =============================================================================

/// Build a `PlanContext` from a path. The path is treated as the
/// `project_root`; the `plan_dir` is `project_root.join("master-plan")`.
///
/// Use this when the test owns a `TempDir` and has copied a fixture into
/// it. For tests that only need the workspace's own plan, prefer
/// [`ctx_for_workspace`].
pub fn ctx_for(project_root: &Path) -> Result<PlanContext> {
    Ok(PlanContext {
        project_root: project_root.to_path_buf(),
        plan_dir: project_root.join("master-plan"),
    })
}

/// Convenience: build a `PlanContext` from a `TestEnv`'s temp dir.
/// Equivalent to `ctx_for(env.tmp.path())` but reads more naturally in
/// tests.
pub fn ctx_for_env(env: &crate::common::TestEnv) -> Result<PlanContext> {
    ctx_for(env.tmp.path())
}

/// Build a `PlanContext` pointing at the workspace's own
/// `master-plan/` tree (the one this repo's mp CLI is configured
/// against). Use for tests that exercise the live repo plan
/// (e.g. the M161 `repo_plan_validates_via_mini_schema` style).
pub fn ctx_for_workspace() -> Result<PlanContext> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let project_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("workspace root")?
        .to_path_buf();
    Ok(PlanContext {
        project_root: project_root.clone(),
        plan_dir: project_root.join("master-plan"),
    })
}

// =============================================================================
// Validate (read-only)
// =============================================================================

/// `mp validate --format json` — runs the full plan validator and
/// returns its JSON report (same shape the CLI emits).
///
/// **Subprocess equivalent:** `env.run(&["validate", "--format", "json"])`.
pub fn validate(ctx: &PlanContext) -> Result<Value> {
    let report = validate_plan(ctx)?;
    Ok(serde_json::to_value(report)?)
}

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

// =============================================================================
// In-process CLI runner (M175) — drop-in for `env.run` on plan operations
// =============================================================================

use std::sync::Mutex;

/// Serializes cwd / fd redirection across parallel tests in the same process.
static RUN_LOCK: Mutex<()> = Mutex::new(());

/// In-process equivalent of [`crate::common::TestEnv::run_json`].
///
/// **Subprocess equivalent:** `env.run_json(args)`.
#[allow(clippy::needless_borrow)] // callers pass both `env` and `&env`
pub fn run_json(env: &crate::common::TestEnv, args: &[&str]) -> Value {
    let out = run(env, args);
    assert!(
        out.status.success(),
        "mp {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

/// In-process equivalent of [`crate::common::TestEnv::run_validate`].
///
/// **Subprocess equivalent:** `env.run_validate()`.
pub fn run_validate(env: &crate::common::TestEnv) -> bool {
    run(env, &["validate", "--format", "json"]).status.success()
}

/// Harness skill-dir keys mirrored from [`crate::common::isolated_harness_env`].
const HARNESS_SKILL_ENV: &[(&str, &str)] = &[
    ("opencode", "harness/opencode/skills"),
    ("cursor", "harness/cursor/skills"),
    ("claude-code", "harness/claude-code/skills"),
    ("gemini", "harness/gemini/skills"),
    ("codex", "harness/codex/skills"),
    ("windsurf", "harness/windsurf/skills"),
    ("cline", "harness/cline/skills"),
    ("pi", "harness/pi/agent/skills"),
];

/// Restores cwd + a set of env vars on drop (shared by `run` / `run_at_repo`).
struct EnvRestore {
    cwd: std::path::PathBuf,
    vars: Vec<(String, Option<std::ffi::OsString>)>,
}
impl Drop for EnvRestore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.cwd);
        for (key, prev) in self.vars.drain(..) {
            match prev {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
    }
}

fn harness_skill_env_key(id: &str) -> String {
    format!("MP_{}_SKILL_DIR", id.to_uppercase().replace('-', "_"))
}

/// Apply isolated harness skill dirs under `root` (F-09: parity with
/// subprocess `isolated_harness_env`). Returns prior values for restore.
fn apply_isolated_harness_env(root: &Path) -> Vec<(String, Option<std::ffi::OsString>)> {
    let mut prev = Vec::with_capacity(HARNESS_SKILL_ENV.len());
    for (id, sub) in HARNESS_SKILL_ENV {
        let dir = root.join(sub);
        let _ = std::fs::create_dir_all(&dir);
        let key = harness_skill_env_key(id);
        prev.push((key.clone(), std::env::var_os(&key)));
        std::env::set_var(&key, &dir);
    }
    prev
}

/// In-process equivalent of [`crate::common::TestEnv::run_at_repo`].
///
/// Cwd stays at the workspace root; plan lives under the test tmp dir.
/// Mirrors subprocess isolation: `MP_INSTALL_DIR` + per-harness
/// `MP_*_SKILL_DIR` under the test tmp tree (F-09 / M158 AC-10).
///
/// **Subprocess equivalent:** `env.run_at_repo(args)`.
pub fn run_at_repo(env: &crate::common::TestEnv, args: &[&str]) -> std::process::Output {
    let _guard = RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let root = crate::common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    let install_dir = env.tmp.path().join("install-target");
    let _ = std::fs::create_dir_all(&install_dir);

    let prior_path = std::env::var_os("PATH");
    let new_path = crate::common::path_with_install_bin(&install_dir);

    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let mut vars = vec![
        ("MP_HOME".to_string(), std::env::var_os("MP_HOME")),
        (
            "MP_INSTALL_DIR".to_string(),
            std::env::var_os("MP_INSTALL_DIR"),
        ),
        ("PATH".to_string(), prior_path),
        (
            "MP_VERIFY_TRUST_REPOSITORY".to_string(),
            std::env::var_os("MP_VERIFY_TRUST_REPOSITORY"),
        ),
        (
            "MP_VERIFY_ALLOW_SHELL".to_string(),
            std::env::var_os("MP_VERIFY_ALLOW_SHELL"),
        ),
    ];
    vars.extend(apply_isolated_harness_env(env.tmp.path()));
    let _env_restore = EnvRestore {
        cwd: prev_cwd,
        vars,
    };

    std::env::set_current_dir(&root).expect("set_current_dir to repo root");
    std::env::set_var("MP_HOME", &root);
    std::env::set_var("MP_INSTALL_DIR", &install_dir);
    std::env::set_var("PATH", &new_path);
    // Same trust/shell opt-in as `run`: bare-repo cwd is the real workspace
    // root, which is not auto-trusted for verification commands.
    std::env::set_var("MP_VERIFY_TRUST_REPOSITORY", "1");
    std::env::set_var("MP_VERIFY_ALLOW_SHELL", "1");

    let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(args.len() + 5);
    argv.push(std::ffi::OsString::from("mp"));
    argv.push(std::ffi::OsString::from("--project-root"));
    argv.push(root.as_os_str().to_os_string());
    argv.push(std::ffi::OsString::from("--plan-dir"));
    argv.push(plan_dir.as_os_str().to_os_string());
    for a in args {
        argv.push(std::ffi::OsString::from(*a));
    }

    let (stdout, stderr, code) = capture_stdio(|| dispatch_cli(argv));
    make_output(code, stdout, stderr)
}

/// In-process equivalent of [`crate::common::TestEnv::run`].
///
/// Parses `args` with the real clap `Cli`, runs `mp::app::run` against the
/// test env's temp project root, and returns a `std::process::Output`-shaped
/// value (status / stdout / stderr) so existing suite assertions keep working.
///
/// **Subprocess equivalent:** `env.run(args)`.
///
/// Prefer domain wrappers (`validate`, `milestone_create`, …) when a test only
/// needs one domain call. Use this when the test exercises multi-flag CLI
/// surface, bulk dispatch, or other paths not yet wrapped.
#[allow(clippy::needless_borrow)] // callers pass both `env` and `&env`
pub fn run(env: &crate::common::TestEnv, args: &[&str]) -> std::process::Output {
    let _guard = RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let project_root = env.tmp.path().to_path_buf();
    let install_dir = project_root.join("install-target");
    let _ = std::fs::create_dir_all(&install_dir);

    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let _env_restore = EnvRestore {
        cwd: prev_cwd,
        vars: vec![
            ("MP_HOME".to_string(), std::env::var_os("MP_HOME")),
            (
                "MP_INSTALL_DIR".to_string(),
                std::env::var_os("MP_INSTALL_DIR"),
            ),
            (
                "MP_VERIFY_TRUST_REPOSITORY".to_string(),
                std::env::var_os("MP_VERIFY_TRUST_REPOSITORY"),
            ),
            (
                "MP_VERIFY_ALLOW_SHELL".to_string(),
                std::env::var_os("MP_VERIFY_ALLOW_SHELL"),
            ),
        ],
    };

    std::env::set_current_dir(&project_root).expect("set_current_dir to test tmp");
    std::env::set_var("MP_HOME", crate::common::repo_root());
    std::env::set_var("MP_INSTALL_DIR", &install_dir);
    std::env::set_var("MP_VERIFY_TRUST_REPOSITORY", "1");
    std::env::set_var("MP_VERIFY_ALLOW_SHELL", "1");

    let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(args.len() + 3);
    argv.push(std::ffi::OsString::from("mp"));
    argv.push(std::ffi::OsString::from("--project-root"));
    argv.push(project_root.as_os_str().to_os_string());
    for a in args {
        argv.push(std::ffi::OsString::from(*a));
    }

    let (stdout, stderr, code) = capture_stdio(|| dispatch_cli(argv));
    // EnvRestore Drop restores cwd / MP_HOME / MP_INSTALL_DIR.
    make_output(code, stdout, stderr)
}

fn dispatch_cli(argv: Vec<std::ffi::OsString>) -> i32 {
    use clap::Parser;
    let cli = match mp::cli::Cli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = e.print();
                    return 0;
                }
                _ => {
                    let _ = e.print();
                    return 2;
                }
            }
        }
    };
    match mp::app::run(cli) {
        Ok(()) => 0,
        Err(e) => {
            if let Some(code) = e.downcast_ref::<mp::ExitCode>() {
                return code.0;
            }
            // Mirror main.rs: real errors go to stderr as `Error: …`.
            eprintln!("Error: {e}");
            1
        }
    }
}

fn make_output(code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> std::process::Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // Wait-status encoding: exit code in high byte.
        let status = std::process::ExitStatus::from_raw(code << 8);
        std::process::Output {
            status,
            stdout,
            stderr,
        }
    }
    #[cfg(not(unix))]
    {
        // Fallback: spawn real binary (Windows not a primary target).
        let _ = (code, stdout, stderr);
        panic!("lib_api::run is unix-only in this tree");
    }
}

/// Redirect stdout+stderr to temp files for the duration of `f`, then restore.
/// Fds are always restored even if `f` panics (`catch_unwind` + Drop guard).
fn capture_stdio<F: FnOnce() -> i32 + std::panic::UnwindSafe>(f: F) -> (Vec<u8>, Vec<u8>, i32) {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::fd::AsRawFd;

        let out_path = std::env::temp_dir().join(format!(
            "mp-lib-api-stdout-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let err_path = out_path.with_extension("err.log");

        let out_file = std::fs::File::create(&out_path).expect("stdout capture file");
        let err_file = std::fs::File::create(&err_path).expect("stderr capture file");
        let out_fd = out_file.as_raw_fd();
        let err_fd = err_file.as_raw_fd();

        // SAFETY: single-threaded critical section under RUN_LOCK.
        // F-07: FdRestore is installed *before* any dup2, with live
        // flags so a failed second dup2 still restores fd 1.
        struct FdRestore {
            saved_out: i32,
            saved_err: i32,
            out_redirected: bool,
            err_redirected: bool,
        }
        impl Drop for FdRestore {
            fn drop(&mut self) {
                unsafe {
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    let _ = std::io::Write::flush(&mut std::io::stderr());
                    if self.out_redirected && self.saved_out >= 0 {
                        let _ = libc::dup2(self.saved_out, 1);
                    }
                    if self.err_redirected && self.saved_err >= 0 {
                        let _ = libc::dup2(self.saved_err, 2);
                    }
                    if self.saved_out >= 0 {
                        libc::close(self.saved_out);
                    }
                    if self.saved_err >= 0 {
                        libc::close(self.saved_err);
                    }
                }
            }
        }
        // Clean capture temps even if `f` panics.
        struct CaptureFiles {
            out: std::path::PathBuf,
            err: std::path::PathBuf,
        }
        impl Drop for CaptureFiles {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.out);
                let _ = std::fs::remove_file(&self.err);
            }
        }
        let _capture_files = CaptureFiles {
            out: out_path.clone(),
            err: err_path.clone(),
        };

        let code = unsafe {
            let saved_out = libc::dup(1);
            let saved_err = libc::dup(2);
            let mut restore = FdRestore {
                saved_out,
                saved_err,
                out_redirected: false,
                err_redirected: false,
            };
            assert!(saved_out >= 0 && saved_err >= 0, "dup stdout/stderr");
            assert_eq!(libc::dup2(out_fd, 1), 1);
            restore.out_redirected = true;
            assert_eq!(libc::dup2(err_fd, 2), 2);
            restore.err_redirected = true;
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            match result {
                Ok(code) => {
                    drop(restore);
                    code
                }
                Err(payload) => {
                    drop(restore);
                    std::panic::resume_unwind(payload);
                }
            }
        };
        drop(out_file);
        drop(err_file);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let _ = std::fs::File::open(&out_path).and_then(|mut f| f.read_to_end(&mut stdout));
        let _ = std::fs::File::open(&err_path).and_then(|mut f| f.read_to_end(&mut stderr));
        (stdout, stderr, code)
    }
    #[cfg(not(unix))]
    {
        let code = f();
        (Vec::new(), Vec::new(), code)
    }
}
