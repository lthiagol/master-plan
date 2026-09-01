use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;

use crate::ac_verify;
use crate::cli::{CriterionCmd, DesignDecisionCmd, MilestoneCmd, OutputFormat as Fmt, QuestionCmd, StageCmd};
use crate::commands::challenge as cmd_challenge_mod;
use crate::commands::common::{
    emit, emit_gate_failure, milestone_summary, prose_verification_warn, read_evidence,
    shell_parse_preflight,
};
use crate::commands::milestone_bulk as cmd_milestone_bulk_mod;
use crate::commands::plan_verify_ac;
use crate::commands::step as cmd_step_mod;
use crate::commands::wp as cmd_wp_mod;
use crate::git;
use crate::groom;
use crate::milestone;
use crate::paths::PlanContext;
use crate::plan_gaps;
use crate::store;
use crate::validate;

// =============================================================================
// M113 S2 — --dry-run previews for set-status / approve / complete.
// =============================================================================
//
// Each helper computes the change set the corresponding command would
// apply, without writing to disk. The shape:
//
//   {
//     "dry_run": true,
//     "command": "<what was invoked>",
//     "files":   ["<plan-relative paths that would change>"],
//     "fields":  { "milestone": {<before/after pairs>} },
//     "verifications": []  // [] for set-status/approve; for complete this
//                          //   is the AC + step verification list.
//   }

fn collect_milestone_field_flips(
    before: &crate::model::MilestoneFile,
    after: &crate::model::MilestoneFile,
) -> serde_json::Value {
    use serde_json::Value;
    let mut flips = serde_json::Map::new();
    let before_meta = serde_json::to_value(&before.milestone).unwrap_or(Value::Null);
    let after_meta = serde_json::to_value(&after.milestone).unwrap_or(Value::Null);
    if let (Some(b), Some(a)) = (before_meta.as_object(), after_meta.as_object()) {
        // M113 S2: emit a flip entry only for keys whose value differs
        // from `before` (no-replacement emission leaves the output
        // stable). Keys present in `before` but missing in `after` are
        // surfaced as `before=<original>` / `after=null`.
        for (k, av) in a {
            let bv = b.get(k).cloned().unwrap_or(Value::Null);
            if &bv != av {
                flips.insert(k.clone(), json!({ "before": bv, "after": av }));
            }
        }
    }
    Value::Object(flips)
}

fn dry_run_envelope(
    command_label: &str,
    files: Vec<String>,
    fields: serde_json::Value,
    verifications: serde_json::Value,
    gates: Vec<serde_json::Value>,
) -> serde_json::Value {
    // M113 review F-3: `gates` mirrors the gate checks the real command
    // runs before writing. Empty when the real invocation would succeed;
    // populated with one entry per failing gate when it would be rejected.
    // The preview still exits 0 and writes nothing — it reports what
    // *would* happen rather than enforcing it — so a human/agent can
    // preview a transition the gate would block.
    json!({
        "dry_run": true,
        "command": command_label,
        "files": files,
        "fields": fields,
        "verifications": verifications,
        "gates": gates,
    })
}

/// Serialize a `ValidationIssue` into the dry-run `gates` entry shape.
fn gate_issue_to_json(issue: &validate::ValidationIssue) -> serde_json::Value {
    json!({
        "code": issue.code,
        "message": issue.message,
        "milestone_id": issue.milestone,
    })
}

/// M113 S2: preview the effect of `mp milestone set-status <id> <state>`.
/// Mirrors live `set_execution_status` gates + `apply_transition` so the
/// dry-run cannot claim success for transitions the commit path rejects.
fn compute_set_status_preview(
    ctx: &PlanContext,
    id: &str,
    status: &str,
) -> Result<serde_json::Value> {
    let path = milestone::load_milestone_path(ctx, id)?;
    let current = store::load_milestone(&path)?;
    let mut gate_errors: Vec<validate::ValidationIssue> = Vec::new();
    let mut after = current.clone();

    if !matches!(
        status,
        "planned" | "in-progress" | "done" | "blocked" | "deferred" | "cancelled"
    ) {
        gate_errors.push(validate::issue(
            "set-status",
            &format!("invalid execution_status: {status}"),
            Some(id.to_string()),
        ));
    } else {
        if current.is_terminal() {
            match status {
                "done" | "cancelled" => {}
                _ => {
                    gate_errors.push(validate::issue(
                        "terminal",
                        &format!(
                            "milestone {id} is terminal (lifecycle={}, cancelled={}); \
                             refusing to set execution_status='{status}'",
                            current.milestone.lifecycle, current.milestone.cancelled
                        ),
                        Some(id.to_string()),
                    ));
                }
            }
        }
        gate_errors.extend(milestone::collect_set_execution_status_gates(
            ctx, &current, status,
        ));
        if gate_errors.is_empty() {
            let event = milestone::event_for_execution_status(&current, status);
            if let Err(err) = milestone::apply_transition(&mut after, event) {
                gate_errors.push(validate::issue(
                    "transition",
                    &err.to_string(),
                    Some(id.to_string()),
                ));
                after = current.clone();
            }
        }
    }

    let files = vec![path
        .strip_prefix(&ctx.project_root)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string()];
    let flips = if gate_errors.is_empty() {
        collect_milestone_field_flips(&current, &after)
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };
    let gates: Vec<serde_json::Value> = gate_errors.iter().map(gate_issue_to_json).collect();
    Ok(dry_run_envelope(
        "milestone set-status",
        files,
        flips,
        serde_json::Value::Array(Vec::new()),
        gates,
    ))
}

/// M113 S2: preview the effect of `mp milestone approve <id>`.
fn compute_approve_preview(ctx: &PlanContext, id: &str) -> Result<serde_json::Value> {
    let path = milestone::load_milestone_path(ctx, id)?;
    let current = store::load_milestone(&path)?;
    // M113 review F-3: mirror the real `approve` gate surface —
    // validate_milestone_ready + check_g14 — so the preview reports
    // the gates the real invocation would fail on instead of claiming
    // success on an un-groomed milestone.
    let cfg = store::try_load_config(ctx)?;
    let mut gate_errors = validate::validate_milestone_ready(&current, cfg.min_out_of_scope());
    gate_errors.extend(validate::check_g14_approval_requests(ctx, id));
    // M121 S9: AC verification integrity pre-flight.
    let verify_report =
        plan_verify_ac::verify_ac(ctx, id).unwrap_or_else(|e| plan_verify_ac::VerifyAcReport {
            ok: false,
            milestone_id: id.to_string(),
            ac_count: 0,
            unresolvable: 1,
            acs: vec![plan_verify_ac::ResolvedAc {
                ac_id: String::new(),
                verification: String::new(),
                status: "UNRESOLVABLE".to_string(),
                detail: format!("verify-ac failed: {}", e),
                target: None,
                symbol: None,
                crate_name: None,
            }],
        });
    for ac in &verify_report.acs {
        if ac.status == "UNRESOLVABLE" {
            gate_errors.push(validate::ValidationIssue {
                code: "M121".to_string(),
                message: format!(
                    "AC {} verification is unresolvable: {}",
                    ac.ac_id, ac.detail
                ),
                milestone: Some(id.to_string()),
            });
        }
    }
    let mut after = current.clone();
    // Preview lifecycle/spec/exec via the shared transition table.
    if let Err(err) = milestone::apply_transition(&mut after, crate::model::MilestoneEvent::Approve)
    {
        gate_errors.push(validate::issue(
            "transition",
            &err.to_string(),
            Some(id.to_string()),
        ));
        after = current.clone();
        after.milestone.spec_status = "ready".to_string();
        after.milestone.execution_status = "planned".to_string();
    }
    let mut gates: Vec<serde_json::Value> = gate_errors.iter().map(gate_issue_to_json).collect();
    gates.push(json!({
        "code": "verify-ac",
        "report": verify_report,
    }));
    let files = vec![path
        .strip_prefix(&ctx.project_root)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string()];
    let flips = collect_milestone_field_flips(&current, &after);
    Ok(dry_run_envelope(
        "milestone approve",
        files,
        flips,
        serde_json::Value::Array(Vec::new()),
        gates,
    ))
}

/// M113 S2: preview the effect of `mp milestone complete <id>`.
fn compute_complete_preview(
    ctx: &PlanContext,
    id: &str,
    evidence: Option<String>,
    force: bool,
    skip_verify: bool,
    _skip_review: bool,
    executor: Option<&str>,
) -> Result<serde_json::Value> {
    // M196: the preview includes the review-gate decision in the
    // `gates` table so the dry-run mirrors what the real invocation
    // would do. The `review_gate` gate fires when a non-track
    // milestone with no passing review tries to reach terminal
    // `complete`; the projection flips the lifecycle to `executed`
    // instead of `complete` (matching the live path).
    let _ = (id, force, skip_verify, _skip_review);
    let path = milestone::load_milestone_path(ctx, id)?;
    let current = store::load_milestone(&path)?;
    let mut after = current.clone();
    if let Some(exec) = executor {
        if !exec.is_empty() {
            after.milestone.executed_by = exec.to_string();
        }
    }
    // Mirror the AC-flips done in `complete_milestone` so the preview
    // matches what would actually land on disk.
    let evidence_text = evidence
        .clone()
        .unwrap_or_else(|| "milestone complete".to_string());
    for ac in &mut after.acceptance_criteria {
        if ac.status != "passed" {
            ac.status = "passed".to_string();
            if ac.evidence.is_empty() {
                ac.evidence = evidence_text.clone();
            }
        } else if evidence.is_some() {
            ac.evidence = evidence_text.clone();
        }
    }
    after.verification.date = store::today();
    if evidence.is_some() || after.verification.evidence.is_empty() {
        after.verification.evidence = evidence_text;
    }
    // Lifecycle/spec/exec flips come from the shared transition table.
    let mut transition_gate: Option<validate::ValidationIssue> = None;
    if let Err(err) =
        milestone::apply_transition(&mut after, crate::model::MilestoneEvent::Complete)
    {
        transition_gate = Some(validate::issue(
            "transition",
            &err.to_string(),
            Some(id.to_string()),
        ));
        // Keep AC/evidence preview mutations; restore delivery fields so we
        // do not claim a Complete projection the table rejected.
        after.milestone.lifecycle = current.milestone.lifecycle.clone();
        after.milestone.spec_status = current.milestone.spec_status.clone();
        after.milestone.execution_status = current.milestone.execution_status.clone();
        after.milestone.blocked = current.milestone.blocked;
        after.milestone.deferred = current.milestone.deferred;
        after.milestone.cancelled = current.milestone.cancelled;
        after.milestone.lifecycle_at = current.milestone.lifecycle_at.clone();
    }
    let flips = collect_milestone_field_flips(&current, &after);

    // Enumerate the verifications that would actually run, mirroring
    // the ac_verify gate logic. Empty `verifications: []` for skip-verify.
    let verifications: serde_json::Value = if skip_verify {
        serde_json::Value::Array(Vec::new())
    } else {
        let mut out = Vec::new();
        for ac in &current.acceptance_criteria {
            if crate::ac_verify::classify(&ac.verification) == crate::ac_verify::Kind::Runnable {
                out.push(json!({
                    "kind": "ac",
                    "id": ac.id,
                    "verification": ac.verification,
                }));
            }
        }
        for step in &current.steps {
            if crate::ac_verify::classify(&step.tests) == crate::ac_verify::Kind::Runnable {
                out.push(json!({
                    "kind": "step",
                    "id": step.id,
                    "tests": step.tests,
                }));
            }
        }
        serde_json::Value::Array(out)
    };
    let files = vec![path
        .strip_prefix(&ctx.project_root)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string()];
    // M113 review F-3: mirror the real `complete` gate surface —
    // G13 (delta completeness) + G15 (code_review enabled, all steps
    // done) — so the preview reports the gates the real invocation
    // would fail on. The real path (commands/milestone.rs::Complete)
    // runs these before touching disk; the preview runs them without
    // enforcing, so a reviewer can see a blocked completion up front.
    let mut gate_errors: Vec<validate::ValidationIssue> = Vec::new();
    if let Some(tg) = transition_gate {
        gate_errors.push(tg);
    }
    if current.is_delta_kind() {
        gate_errors.extend(validate::validate_delta_complete(ctx, &current));
    }
    let cfg = store::load_config(ctx);
    if cfg.code_review_enabled() {
        let pending: Vec<&str> = current
            .steps
            .iter()
            .filter(|s| s.status != "done")
            .map(|s| s.id.as_str())
            .collect();
        if !pending.is_empty() {
            gate_errors.push(validate::issue(
                "G15",
                &format!(
                    "code_review is enabled but {} step(s) are not done: {}",
                    pending.len(),
                    pending.join(", ")
                ),
                Some(id.to_string()),
            ));
        }
    }
    let gates: Vec<serde_json::Value> = gate_errors.iter().map(gate_issue_to_json).collect();
    let mut payload = dry_run_envelope("milestone complete", files, flips, verifications, gates);
    if force {
        payload["force"] = json!(true);
    }
    if skip_verify {
        payload["skip_verify"] = json!(true);
    }
    Ok(payload)
}

// === Public, top-level dispatcher ===

pub(crate) fn cmd_milestone(ctx: &PlanContext, cmd: MilestoneCmd, format: Fmt) -> Result<()> {
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    let recoverable = matches!(
        &cmd,
        MilestoneCmd::Create { .. }
            | MilestoneCmd::Split { .. }
            | MilestoneCmd::Delete { .. }
            | MilestoneCmd::Archive { .. }
            | MilestoneCmd::Restore { .. }
            | MilestoneCmd::Purge { .. }
            | MilestoneCmd::Challenge(_)
            | MilestoneCmd::Bulk(_)
    );
    if recoverable {
        txn.run_recoverable(|_| cmd_milestone_inner(ctx, cmd, format))
    } else {
        txn.run(|_| cmd_milestone_inner(ctx, cmd, format))
    }
}

fn cmd_milestone_inner(ctx: &PlanContext, cmd: MilestoneCmd, format: Fmt) -> Result<()> {
    match cmd {
        MilestoneCmd::Create {
            title,
            file,
            json,
            from_handoff,
            example,
        } => {
            if example {
                let template = serde_json::json!({
                    "title": "My Milestone",
                    "intent": { "outcome": "What users can do after this ships." },
                    "problem": { "description": "Why this is needed — the gap it fills." },
                    "scope": {
                        "in_scope": ["Specific deliverable"],
                        "out_of_scope": ["Explicit non-goal 1", "Explicit non-goal 2"]
                    },
                    "acceptance_criteria": [
                        { "description": "Observable behavior that proves completion", "verification": "How to verify (test command, manual check)" }
                    ],
                    "design_decisions": [],
                    "open_questions": []
                });
                return emit(format, &template);
            }
            if let Some(handoff_path) = from_handoff {
                let report = milestone::create_from_handoff(ctx, &handoff_path)?;
                return emit(format, &report);
            }
            let input =
                milestone::read_create_input(title.as_deref(), file.as_deref(), json.as_deref())?;
            let m = milestone::create_milestone(ctx, input)?;
            milestone::warn_dangling_deps(ctx, &m.milestone.depends_on);
            let payload = json!({
                "ok": true,
                "milestone": milestone_summary(&m),
            });
            emit(format, &payload)
        }
        MilestoneCmd::Update {
            id,
            file,
            json,
            if_updated,
            replace_arrays,
            accept_extra_fields,
            verification,
            verification_file,
            verification_date,
            verification_branch,
        } => {
            // M165: post-completion evidence amend surface. The four CLI
            // flags land without requiring --json / --file. When --json or
            // --file IS supplied, the verification block from JSON wins
            // field-by-field; CLI flags fill in only the supplied fields.
            let cli_provided_any = verification.is_some()
                || verification_file.is_some()
                || verification_date.is_some()
                || verification_branch.is_some();
            let mut input = if file.is_some() || json.is_some() {
                milestone::read_update_input(
                    file.as_deref(),
                    json.as_deref(),
                    replace_arrays,
                    accept_extra_fields,
                )?
            } else if cli_provided_any {
                crate::milestone::UpdateMilestoneInput::default()
            } else {
                // Caller supplied neither JSON nor any verification flag.
                // Refuse early — `mp milestone update` with no writes is a
                // no-op that risks confusing future readers of the audit
                // trail.
                anyhow::bail!(
                    "mp milestone update requires at least one write flag: --json, --file, \
                     --verification, --verification-file, --verification-date, or --verification-branch"
                );
            };

            if cli_provided_any {
                // M165: when only some verification flags are supplied
                // (e.g. just --verification-date + --verification-branch),
                // the unsupplied fields must NOT clobber the on-disk
                // value. `input.verification` only carries the caller's
                // JSON/file payload, so load the existing milestone
                // separately to use as the merge base.
                let existing_block = if input.verification.is_none() {
                    let path = crate::milestone::load_milestone_path(ctx, &id)?;
                    let current = crate::store::load_milestone(&path)?;
                    Some(current.verification)
                } else {
                    None
                };
                let mut block = input.verification.or(existing_block).unwrap_or_default();
                if let Some(text) = verification {
                    if text.is_empty() {
                        anyhow::bail!(
                            "--verification text is empty; pass --verification-file <path> for long values, or supply non-empty text"
                        );
                    }
                    block.evidence = text;
                }
                if let Some(path) = verification_file {
                    if path.as_os_str().is_empty() {
                        anyhow::bail!("--verification-file path is empty; pass a valid path");
                    }
                    let text = std::fs::read_to_string(&path).with_context(|| {
                        format!("failed to read --verification-file {}", path.display())
                    })?;
                    if text.is_empty() {
                        anyhow::bail!(
                            "--verification-file {} is empty; refusing to clobber verification.evidence with an empty string",
                            path.display()
                        );
                    }
                    block.evidence = text;
                }
                if let Some(date) = verification_date {
                    block.date = date;
                }
                if block.date.is_empty() {
                    // Stamp the date when nothing else set it — keeps
                    // `verification.date` from drifting to empty on every
                    // amend.
                    block.date = crate::store::today();
                }
                if let Some(branch) = verification_branch {
                    block.branch = branch;
                }
                input.verification = Some(block);
            }

            let m = milestone::update_milestone(ctx, &id, input, if_updated.as_deref())?;
            milestone::warn_dangling_deps(ctx, &m.milestone.depends_on);
            let payload = json!({
                "ok": true,
                "milestone": milestone_summary(&m),
            });
            emit(format, &payload)
        }
        MilestoneCmd::SetSpecStatus { id, status } => {
            let path = milestone::load_milestone_path(ctx, &id)?;
            let current = store::load_milestone(&path)?;
            let cfg = store::try_load_config(ctx)?;
            let mut errors =
                milestone::gate_errors_for_spec_status(&current, &status, cfg.min_out_of_scope());
            if status == "ready" {
                errors.extend(validate::check_g14_approval_requests(ctx, &id));
            }
            if !errors.is_empty() {
                emit_gate_failure(format, errors)?;
            }
            let m = milestone::apply_spec_status(ctx, &id, &status)?;
            let payload = json!({
                "ok": true,
                "milestone": milestone_summary(&m),
            });
            emit(format, &payload)
        }
        MilestoneCmd::SetPriority { id, priority } => {
            let m = milestone::set_priority(ctx, &id, &priority)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m), "priority": priority }),
            )
        }
        MilestoneCmd::SetLifecycle {
            id,
            status,
            dry_run,
        } => {
            if dry_run {
                let preview = milestone::set_lifecycle_preview(ctx, &id, &status)?;
                return emit(format, &preview);
            }
            let m = milestone::set_lifecycle(ctx, &id, &status, /* commit */ true)?;
            emit(
                format,
                &json!({
                    "ok": true,
                    "milestone": milestone_summary(&m),
                    "lifecycle": m.milestone.lifecycle,
                }),
            )
        }
        MilestoneCmd::SetTargetVersion { id, version } => {
            let m = milestone::set_target_version(ctx, &id, &version)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m), "target_version": version }),
            )
        }
        MilestoneCmd::Approve { id, dry_run } => {
            if dry_run {
                let preview = compute_approve_preview(ctx, &id)?;
                return emit(format, &preview);
            }
            let path = milestone::load_milestone_path(ctx, &id)?;
            let m = store::load_milestone(&path)?;
            let cfg = store::try_load_config(ctx)?;
            let mut errors = validate::validate_milestone_ready(&m, cfg.min_out_of_scope());
            errors.extend(validate::check_g14_approval_requests(ctx, &id));
            // M121 S9 + F-08: AC verification integrity pre-flight.
            // The gate fails on UNRESOLVABLE (definitely broken), empty
            // (no verification defined), and unknown (unrecognized command
            // form). runtime (cargo clippy/fmt/build — recognized but
            // not statically resolvable) and inline (grep/rg/awk) and
            // manual are surfaced in the integrity report but pass the
            // gate — the reviewer acknowledges them in the report.
            match plan_verify_ac::verify_ac(ctx, &id) {
                Ok(report) => {
                    for ac in &report.acs {
                        if ac.status == "UNRESOLVABLE"
                            || ac.status == "empty"
                            || ac.status == "unknown"
                        {
                            errors.push(validate::ValidationIssue {
                                code: "M121".to_string(),
                                message: format!(
                                    "AC {} verification is not gate-passing ({}): {}",
                                    ac.ac_id, ac.status, ac.detail
                                ),
                                milestone: Some(id.clone()),
                            });
                        }
                    }
                }
                Err(e) => {
                    errors.push(validate::ValidationIssue {
                        code: "M121".to_string(),
                        message: format!("verify-ac check failed: {}", e),
                        milestone: Some(id.clone()),
                    });
                }
            }
            if !errors.is_empty() {
                emit_gate_failure(format, errors)?;
            }
            let m = milestone::approve_milestone(ctx, &id)?;
            let payload = json!({
                "ok": true,
                "milestone": milestone_summary(&m),
            });
            emit(format, &payload)
        }
        MilestoneCmd::SetStatus {
            id,
            status,
            dry_run,
        } => {
            if dry_run {
                let preview = compute_set_status_preview(ctx, &id, &status)?;
                return emit(format, &preview);
            }
            let m = milestone::set_execution_status(ctx, &id, &status)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m) }),
            )
        }
        MilestoneCmd::Complete {
            id,
            evidence,
            evidence_file,
            force,
            skip_verify,
            skip_review,
            executor,
            dry_run,
        } => {
            // M113 S2: --dry-run prints the change set (files, fields,
            // verifications) without writing or running anything. Runs
            // before the structural / G15 gate checks so the preview
            // mirrors what the real invocation would do.
            if dry_run {
                let preview = compute_complete_preview(
                    ctx,
                    &id,
                    read_evidence(evidence, evidence_file)?,
                    force,
                    skip_verify,
                    skip_review,
                    executor.as_deref(),
                )?;
                return emit(format, &preview);
            }

            let mut ev = read_evidence(evidence, evidence_file)?;
            let path = milestone::load_milestone_path(ctx, &id)?;
            let current = Arc::new(store::load_milestone(&path)?);

            // Structural gates first (e.g. delta version mismatch G13).
            if current.is_delta_kind() {
                let errors = validate::validate_delta_complete(ctx, &current);
                if !errors.is_empty() {
                    emit_gate_failure(format, errors)?;
                }
            }

            // G15: code review gate — refuse to complete when code_review=true
            // and not all steps are done.
            let cfg = store::load_config(ctx);
            if cfg.code_review_enabled() {
                let pending: Vec<&str> = current
                    .steps
                    .iter()
                    .filter(|s| s.status != "done")
                    .map(|s| s.id.as_str())
                    .collect();
                if !pending.is_empty() {
                    let errors = vec![validate::issue(
                        "G15",
                        &format!(
                            "code_review is enabled but {} step(s) are not done: {}",
                            pending.len(),
                            pending.join(", ")
                        ),
                        Some(id.clone()),
                    )];
                    emit_gate_failure(format, errors)?;
                }
            }

            // M106 (S16): --skip-verify bypasses the verifier calls entirely.
            // Both --skip-verify and --force skip the gate-failure-on-error
            // path; --skip-verify is stronger (no verifications are run).
            if skip_verify {
                let skip_note = "[skip-verify: AC and step verifications skipped]";
                ev = Some(match ev {
                    Some(existing) => format!("{existing} {skip_note}"),
                    None => skip_note.to_string(),
                });
            } else {
                // M106 (S15) + M107 (S3) + M108 (S3) ER-2: run both
                // verifier calls on a worker thread and bound the wait
                // with `recv_timeout` so a wedged verifier can never
                // hang the session past
                // `MP_COMPLETE_GLOBAL_DEADLINE_SECS`. On timeout we
                // (a) flip a cooperative cancel flag the verifier polls
                // between `try_wait()` rounds, (b) call
                // `libc::killpg(pgid, SIGKILL)` on every child the
                // verifier registered as a new process-group leader —
                // belt-and-suspenders in case (a) is somehow ignored —
                // and (c) `join()` the worker instead of leaking it
                // via `mem::forget`. The worker exits within bounded
                // time once its children are reaped and the cancel
                // flag is observed.
                let global_deadline = global_complete_deadline_dur();
                let (tx, rx) =
                    mpsc::sync_channel::<(ac_verify::VerifyReport, ac_verify::StepTestsReport)>(1);
                let cwd_for_verifier = ctx.project_root.clone();
                let cancelled = Arc::new(AtomicBool::new(false));
                let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
                let verifier_handle = thread::Builder::new()
                    .name("mp_complete_verifier".into())
                    .spawn({
                        let current = Arc::clone(&current);
                        let cancelled = Arc::clone(&cancelled);
                        let child_pids = Arc::clone(&child_pids);
                        move || {
                            let mut cmd_cache = ac_verify::CommandCache::new();
                            let ac_report = ac_verify::verify_milestone_in(
                                &current,
                                Some(&cwd_for_verifier),
                                &cancelled,
                                &child_pids,
                                Some(&mut cmd_cache),
                            );
                            let step_report = ac_verify::verify_step_tests_in(
                                &current,
                                Some(&cwd_for_verifier),
                                &cancelled,
                                &child_pids,
                                Some(&mut cmd_cache),
                            );
                            let _ = tx.send((ac_report, step_report));
                        }
                    })
                    .expect("spawn mp_complete_verifier");
                let (report, step_report) = match rx.recv_timeout(global_deadline) {
                    Ok(reports) => reports,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // (a) Cooperative cancel: the verifier observes
                        // this flag at the top of every `try_wait` round
                        // and bails out cleanly (kills its child + waits
                        // on drain threads). Cheap to flip; safe under
                        // relaxed ordering because we only need eventual
                        // visibility.
                        cancelled.store(true, Ordering::Relaxed);

                        // (b) Process-group kill: each child the
                        // verifier spawned was placed in its own process
                        // group via `Command::process_group(0)`, so
                        // pgid == child_pid. `libc::killpg(pgid, SIGKILL)`
                        // (note: positive pgid — the libc binding
                        // translates to `kill(-pgid, sig)` internally)
                        // takes the entire subprocess tree in one
                        // syscall. ESRCH is fine — the child may have
                        // already exited under the cancel flag.
                        #[cfg(unix)]
                        {
                            let pids: Vec<u32> = match child_pids.lock() {
                                Ok(reg) => reg.clone(),
                                Err(_) => Vec::new(),
                            };
                            for pid in &pids {
                                // SAFETY: `libc::killpg(pgid, sig)` is
                                // a thin wrapper over the POSIX
                                // `killpg` syscall, which internally
                                // translates to `kill(-pgid, sig)`.
                                // We pass the positive `pgid` here;
                                // the libc binding handles the
                                // negation. (Earlier iterations of
                                // this code passed `-pgid` — a
                                // double-invert that would have sent
                                // the signal to pid=-pgid, an inert
                                // value; that bug was caught as F-1
                                // during M107 S3.3 and is also
                                // documented in the original M107
                                // review notes (since removed; see
                                // git history) F-1.) The `as i32` cast is a no-op
                                // for any valid Unix pid (which
                                // always fits in i32). ESRCH (no such
                                // process) is acceptable because we
                                // are already in an error path; the
                                // desired post-condition is
                                // "children dead", not "killpg
                                // returned 0".
                                let pgid = *pid as i32;
                                unsafe {
                                    libc::killpg(pgid, libc::SIGKILL);
                                }
                            }
                        }

                        // (c) Join the worker. Now that (a) made it
                        // cooperative-bail and (b) reaped the
                        // subprocesses, the worker will return within
                        // bounded time and `join` will not block
                        // indefinitely. If `join` returns Err, the
                        // worker panicked — we treat that as a verifier
                        // failure too.
                        let _ = verifier_handle.join();

                        let payload = json!({
                            "ok": false,
                            "gate": "global-deadline",
                            "milestone": milestone_summary(&current),
                            "message": format!(
                                "verifier ran longer than {}s (MP_COMPLETE_GLOBAL_DEADLINE_SECS); aborting. \
                                 Historical milestones with broad-scope AC/step strings (see \
                                 `make verify-lint` and docs/TEST-AUDIT.md) are the most likely culprit.",
                                global_deadline.as_secs()
                            ),
                            "deadline_secs": global_deadline.as_secs(),
                        });
                        emit(format, &payload)?;
                        return Err(anyhow::Error::new(crate::ExitCode(2)));
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        // Sender thread panicked; treat as verifier failure.
                        let payload = json!({
                            "ok": false,
                            "gate": "global-deadline",
                            "milestone": milestone_summary(&current),
                            "message": "verifier thread disconnected unexpectedly (panic?)",
                        });
                        emit(format, &payload)?;
                        return Err(anyhow::Error::new(crate::ExitCode(2)));
                    }
                };

                // AC verification gate (M30): refuse to complete if any runnable AC
                // verification fails, unless --force is given (which records the bypass).
                if !report.ok && !force {
                    let failing: Vec<serde_json::Value> = report
                        .failures()
                        .iter()
                        .map(|r| {
                            json!({
                                "ac_id": r.ac_id,
                                "description": r.description,
                                "verification": r.verification,
                                "exit_code": r.exit_code,
                                "output": r.output,
                                "note": r.note,
                            })
                        })
                        .collect();
                    let payload = json!({
                        "ok": false,
                        "gate": "ac-verification",
                        "milestone": milestone_summary(&current),
                        "message": format!(
                            "{} runnable AC verification(s) failed; fix them or rerun with --force",
                            report.runnable_failed
                        ),
                        "failures": failing,
                    });
                    emit(format, &payload)?;
                    return Err(anyhow::Error::new(crate::ExitCode(2)));
                }
                if !report.ok && force {
                    let note = format!(
                        "[verification force-bypassed: {} runnable AC(s) failed]",
                        report.runnable_failed
                    );
                    ev = Some(match ev {
                        Some(existing) => format!("{existing} {note}"),
                        None => note,
                    });
                }

                // Step tests guardrail: refuse to complete if any step's runnable
                // tests command fails, unless --force (which records the bypass).
                if !step_report.ok && !force {
                    let failing: Vec<serde_json::Value> = step_report
                        .results
                        .iter()
                        .filter(|r| r.kind == ac_verify::Kind::Runnable && !r.passed)
                        .map(|r| {
                            json!({
                                "step_id": r.step_id,
                                "tests": r.tests,
                                "exit_code": r.exit_code,
                                "output": r.output,
                                "note": r.note,
                            })
                        })
                        .collect();
                    let payload = json!({
                        "ok": false,
                        "gate": "step-tests",
                        "milestone": milestone_summary(&current),
                        "message": format!(
                            "{} runnable step test(s) failed; fix them or rerun with --force",
                            step_report.runnable_failed
                        ),
                        "failures": failing,
                    });
                    emit(format, &payload)?;
                    return Err(anyhow::Error::new(crate::ExitCode(2)));
                }
                if !step_report.ok && force {
                    let note = format!(
                        "[step-tests force-bypassed: {} runnable step test(s) failed]",
                        step_report.runnable_failed
                    );
                    ev = Some(match ev {
                        Some(existing) => format!("{existing} {note}"),
                        None => note,
                    });
                }
            }

            let m = milestone::complete_milestone(ctx, &id, ev, executor.as_deref(), skip_review)?;
            let mut payload = json!({ "ok": true, "milestone": milestone_summary(&m) });
            if m.is_delta_kind() {
                if let Ok(spec) = store::load_domain_spec(ctx, &m.delta.domain) {
                    payload["domain"] = json!({
                        "id": spec.domain.id,
                        "version": spec.domain.version,
                    });
                }
            }
            let cfg = store::load_config(ctx);
            if cfg.should_git_commit_on_milestone_complete() {
                match git::git_commit(ctx, None) {
                    Ok(report) => {
                        payload["git"] = serde_json::to_value(report)?;
                    }
                    Err(err) => {
                        payload["git"] = json!({ "ok": false, "error": err.to_string() });
                    }
                }
            }
            emit(format, &payload)
        }
        MilestoneCmd::Verify { id } => {
            let path = milestone::load_milestone_path(ctx, &id)?;
            let m = store::load_milestone(&path)?;
            // M107 (S3): `mp milestone verify` is a fire-and-forget CLI
            // invocation (not wrapped in a worker thread), so the cancel
            // flag and child-pid registry use the same defaults the
            // library-level helpers provide: never-cancelled, no
            // registration. The flip side is that this command cannot
            // honor `MP_COMPLETE_GLOBAL_DEADLINE_SECS` — that's the
            // orchestrator's responsibility in the `complete` path.
            let cancelled = Arc::new(AtomicBool::new(false));
            let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
            let report = ac_verify::verify_milestone_in(
                &m,
                Some(&ctx.project_root),
                &cancelled,
                &child_pids,
                None,
            );
            emit(format, &report)?;
            if !report.ok {
                return Err(anyhow::Error::new(crate::ExitCode(1)));
            }
            Ok(())
        }
        MilestoneCmd::Criterion { cmd } => handle_criterion_cmd(ctx, cmd, format),
        MilestoneCmd::Ac { cmd } => handle_criterion_cmd(ctx, cmd, format),
        MilestoneCmd::Decompose { id, work_packages } => {
            let report = plan_gaps::decompose_milestone(ctx, &id, work_packages)?;
            emit(format, &report)
        }
        MilestoneCmd::Plan { id, work_packages } => {
            let report = plan_gaps::plan_milestone(ctx, &id, work_packages)?;
            emit(format, &report)
        }
        MilestoneCmd::Block { id, reason, by } => {
            let m = milestone::block_milestone(ctx, &id, &reason, by.as_deref())?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m) }),
            )
        }
        MilestoneCmd::Unblock { id } => {
            let m = milestone::unblock_milestone(ctx, &id)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m) }),
            )
        }
        MilestoneCmd::Defer { id, reason, by } => {
            let m = milestone::defer_milestone(ctx, &id, &reason, by.as_deref())?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m) }),
            )
        }
        MilestoneCmd::Reopen { id } => {
            let m = milestone::reopen_milestone(ctx, &id)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": milestone_summary(&m) }),
            )
        }
        MilestoneCmd::Split { id, into, titles } => {
            let payload = milestone::split_milestone(ctx, &id, into, titles)?;
            emit(format, &payload)
        }
        MilestoneCmd::Question { cmd } => match cmd {
            QuestionCmd::Add { id, text } => {
                let q = milestone::question_add(ctx, &id, &text)?;
                emit(format, &json!({ "ok": true, "question": q }))
            }
            QuestionCmd::Resolve {
                id,
                qid,
                resolution,
            } => {
                let q = milestone::question_resolve(ctx, &id, &qid, &resolution)?;
                emit(format, &json!({ "ok": true, "question": q }))
            }
        },
        MilestoneCmd::Delete { id, force } => {
            if !force {
                let cfg = store::load_config(ctx);
                if cfg.archive_on_milestone_delete() {
                    milestone::archive_milestone(ctx, &id)?;
                    return emit(
                        format,
                        &json!({ "ok": true, "archived": id, "note": "archive_on_milestone_delete is true; use --force to hard-delete" }),
                    );
                }
            }
            milestone::delete_milestone(ctx, &id, force)?;
            emit(format, &json!({ "ok": true, "deleted": id }))
        }
        MilestoneCmd::Archive { id } => {
            milestone::archive_milestone(ctx, &id)?;
            emit(format, &json!({ "ok": true, "archived": id }))
        }
        MilestoneCmd::Restore { id } => {
            milestone::restore_archived_milestone(ctx, &id)?;
            emit(format, &json!({ "ok": true, "restored": id }))
        }
        MilestoneCmd::Purge { id } => {
            milestone::purge_archived_milestone(ctx, &id)?;
            emit(format, &json!({ "ok": true, "purged": id }))
        }
        MilestoneCmd::Groom { id } => {
            let report = groom::groom_milestone(ctx, &id)?;
            emit(format, &report)
        }
        MilestoneCmd::Trace { id } => {
            let report = crate::milestone_trace::milestone_trace(ctx, &id)?;
            emit(format, &report)
        }
        MilestoneCmd::Log { id } => {
            let report = crate::milestone_trace::milestone_log(ctx, &id)?;
            emit(format, &report)
        }
        MilestoneCmd::Dependents { id } => {
            let report = crate::graph::graph_explain(ctx, &id)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": id, "dependents": report.downstream }),
            )
        }
        MilestoneCmd::Deps { id } => {
            let report = crate::graph::graph_explain(ctx, &id)?;
            emit(
                format,
                &json!({ "ok": true, "milestone": id, "deps": report.waiting_on }),
            )
        }
        MilestoneCmd::Impact { id } => {
            use std::collections::HashSet;
            let norm = crate::paths::normalize_milestone_id(&id);
            let milestones = store::load_all_milestones(ctx)?;

            // Recursive reverse dependents
            let mut visited = HashSet::new();
            let mut queue = vec![norm.clone()];
            let mut transitive_dependents = Vec::new();
            while let Some(current) = queue.pop() {
                if !visited.insert(current.clone()) {
                    continue;
                }
                for (_, m) in &milestones {
                    let mid = crate::paths::normalize_milestone_id(&m.milestone.id);
                    if m.milestone
                        .depends_on
                        .iter()
                        .any(|d| crate::paths::normalize_milestone_id(d) == current)
                        && !visited.contains(&mid)
                    {
                        transitive_dependents.push(mid.clone());
                        queue.push(mid);
                    }
                }
            }

            // Path pins involving this milestone
            let pins = crate::path_prefs::list_pins(ctx).unwrap_or_default();
            let milestone_pins: Vec<serde_json::Value> = pins.iter()
                .filter(|p| p.milestone == norm)
                .map(|p| json!({ "milestone": p.milestone, "before": p.before, "rank": p.rank, "reason": p.reason }))
                .collect();

            // Path ordering: find position in baseline order
            let order = crate::path_engine::build_path(ctx, 100)
                .map(|r| r.baseline_milestone_order)
                .unwrap_or_default();
            let pos = order.iter().position(|m| m == &norm);

            emit(
                format,
                &json!({
                    "ok": true,
                    "milestone": id,
                    "transitive_dependents": transitive_dependents,
                    "path_pins": milestone_pins,
                    "position_in_path": pos,
                }),
            )
        }
        MilestoneCmd::ListPendingReview => {
            let cfg = store::load_config(ctx);
            let milestones = store::load_all_milestones(ctx)?;
            let pending: Vec<serde_json::Value> = milestones
                .into_iter()
                .filter(|(_, m)| {
                    m.milestone.spec_status == "implemented" && cfg.code_review_enabled()
                })
                .map(|(_, m)| milestone_summary(&m))
                .collect();
            emit(format, &json!({ "ok": true, "milestones": pending }))
        }
        MilestoneCmd::Challenge(cmd) => cmd_challenge_mod::cmd_challenge(ctx, cmd, format),
        MilestoneCmd::Step(cmd) => cmd_step_mod::cmd_step(ctx, cmd, format),
        MilestoneCmd::Wp(cmd) => cmd_wp_mod::cmd_wp(ctx, cmd, format),
        MilestoneCmd::DesignDecision(cmd) => match cmd {
            DesignDecisionCmd::Add {
                id,
                area,
                decision,
                rationale,
            } => {
                let dd = milestone::design_decision_add(ctx, &id, &area, &decision, &rationale)?;
                emit(format, &json!({ "ok": true, "design_decision": dd }))
            }
            DesignDecisionCmd::Update {
                id,
                index,
                area,
                new_area,
                decision,
                rationale,
            } => {
                let dd = milestone::design_decision_update(
                    ctx, &id, index, area, new_area, decision, rationale,
                )?;
                emit(format, &json!({ "ok": true, "design_decision": dd }))
            }
            DesignDecisionCmd::Remove { id, index, area } => {
                let pos = milestone::design_decision_remove(ctx, &id, index, area)?;
                emit(format, &json!({ "ok": true, "removed_index": pos }))
            }
        },
        MilestoneCmd::Bulk(cmd) => cmd_milestone_bulk_mod::cmd_milestone_bulk(ctx, cmd, format),
        MilestoneCmd::Stage { cmd } => handle_stage_cmd(ctx, cmd, format),
    }
}

/// M202: per-stage mp-flow tracker dispatch.
///
/// `stage list <id>` prints all 12 stages as a CLI table with status
/// and timestamp. Stages with no entry yet show `pending` and `—` for
/// the timestamp — the table is canonical regardless of how many
/// stages have actually fired (AC-07).
///
/// `stage set <id> <stage> <status>` enforces two guards: only the
/// 12 canonical stage slugs are accepted, and only the 4-value enum
/// (`pending | done | in_progress | skipped`) is accepted. Exit code 2
/// on invalid input; the milestone file is unchanged on rejection
/// (AC-08). Hand-off is in the 12-stage list and accepts any of the 4
/// values explicitly — it just never auto-advances (AC-11).
fn handle_stage_cmd(ctx: &PlanContext, cmd: StageCmd, format: Fmt) -> Result<()> {
    use crate::cli::StageCmd as S;
    match cmd {
        S::List { id } => {
            let path = milestone::load_milestone_path(ctx, &id)?;
            let m = crate::store::load_milestone(&path)?;
            let stages = m
                .milestone
                .flow_stages
                .iter()
                .map(|(slug, stage)| {
                    (
                        slug.as_str(),
                        stage.status.as_str(),
                        stage.at.as_deref().unwrap_or("—"),
                    )
                })
                .collect::<Vec<_>>();
            // Stable canonical order: emit MP_FLOW_STAGE_KEYS first (in
            // order), then any unexpected keys (defensive — should not
            // happen, but don't silently drop user-written stages).
            let mut rows: Vec<(String, String, String)> = Vec::with_capacity(12);
            for slug in mp_model::MP_FLOW_STAGE_KEYS {
                let (status, at) = stages
                    .iter()
                    .find(|(s, _, _)| *s == *slug)
                    .map(|(_, status, at)| (status.to_string(), at.to_string()))
                    .unwrap_or_else(|| ("pending".to_string(), "—".to_string()));
                rows.push((slug.to_string(), status, at));
            }
            let known: std::collections::HashSet<&str> = mp_model::MP_FLOW_STAGE_KEYS
                .iter()
                .copied()
                .collect();
            for (slug, stage) in &m.milestone.flow_stages {
                if !known.contains(slug.as_str()) {
                    rows.push((
                        slug.clone(),
                        stage.status.clone(),
                        stage.at.clone().unwrap_or_else(|| "—".to_string()),
                    ));
                }
            }
            let value = json!({
                "ok": true,
                "milestone": id,
                "stages": rows
                    .iter()
                    .map(|(slug, status, at)| {
                        json!({
                            "stage": slug,
                            "status": status,
                            "at": at,
                        })
                    })
                    .collect::<Vec<_>>(),
            });
            // Format=human renders as a 12-row table. JSON emits the
            // same rows as a `stages` array. Default (human) stays the
            // primary surface so an operator can scan all 12 at a glance.
            match format {
                Fmt::Json => emit(format, &value),
                _ => {
                    let header = format!(
                        "{:<14} {:<12} {}",
                        "STAGE", "STATUS", "AT"
                    );
                    let mut out = String::new();
                    out.push_str(&header);
                    out.push('\n');
                    for (slug, status, at) in &rows {
                        out.push_str(&format!("{slug:<14} {status:<12} {at}\n"));
                    }
                    println!("{out}");
                    Ok(())
                }
            }
        }
        S::Set { id, stage, status } => {
            // AC-08: strict 12-key guard + 4-value enum guard. Exit 2
            // on either failure; no on-disk mutation.
            if !mp_model::MP_FLOW_STAGE_KEYS.contains(&stage.as_str()) {
                eprintln!(
                    "invalid stage: {stage:?} (expected one of: {})",
                    mp_model::MP_FLOW_STAGE_KEYS.join(", ")
                );
                std::process::exit(2);
            }
            if !mp_model::MP_FLOW_STAGE_STATUSES.contains(&status.as_str()) {
                eprintln!(
                    "invalid status: {status:?} (expected one of: {})",
                    mp_model::MP_FLOW_STAGE_STATUSES.join(", ")
                );
                std::process::exit(2);
            }
            // Load + apply + write. We use with_milestone_mut_unlocked
            // so the durable-writer contract (atomic write + lock
            // discipline) matches every other stage-touching site.
            let path = milestone::load_milestone_path(ctx, &id)?;
            let mut m = crate::store::load_milestone(&path)?;
            m.milestone.flow_stages.insert(
                stage.clone(),
                mp_model::FlowStage {
                    status: status.clone(),
                    at: Some(crate::store::now_rfc3339()),
                },
            );
            m.milestone.updated = crate::store::today();
            milestone::write_milestone_synced(ctx, &path, &m)?;
            emit(
                format,
                &json!({
                    "ok": true,
                    "milestone": id,
                    "stage": stage,
                    "status": status,
                    "at": crate::store::now_rfc3339(),
                }),
            )
        }
    }
}

/// Shared dispatch for the `criterion` and `ac` subcommands. Both paths return
/// the same JSON contract; `ac` is the agent-friendly short alias introduced in M93.
fn handle_criterion_cmd(ctx: &PlanContext, cmd: CriterionCmd, format: Fmt) -> Result<()> {
    match cmd {
        CriterionCmd::Pass {
            id,
            ac_id,
            evidence,
        } => {
            let ac = milestone::criterion_pass(ctx, &id, &ac_id, evidence)?;
            emit(format, &json!({ "ok": true, "acceptance_criterion": ac }))
        }
        CriterionCmd::Fail { id, ac_id, reason } => {
            let ac = milestone::criterion_fail(ctx, &id, &ac_id, reason)?;
            emit(format, &json!({ "ok": true, "acceptance_criterion": ac }))
        }
        CriterionCmd::Add {
            id,
            description,
            verification,
        } => {
            let ac = milestone::criterion_add(ctx, &id, &description, &verification)?;
            let mut payload = json!({ "ok": true, "acceptance_criterion": ac });
            if let Some(warning) = shell_parse_preflight(&verification) {
                payload["preflight_warning"] = warning;
            }
            if let Some(warning) = prose_verification_warn(&verification) {
                payload["prose_warning"] = warning;
            }
            emit(format, &payload)
        }
        CriterionCmd::Show { id, ac_id } => {
            let ac = milestone::criterion_show(ctx, &id, &ac_id)?;
            emit(format, &ac)
        }
        CriterionCmd::List { id } => {
            let acs = milestone::criterion_list(ctx, &id)?;
            emit(format, &acs)
        }
        CriterionCmd::Update {
            id,
            ac_id,
            description,
            verification,
            evidence,
        } => {
            // M111 S6: capture the verification for the post-write pre-flight
            // warning before it moves into the mutator.
            let preflight_target = verification.clone();
            let ac =
                milestone::criterion_update(ctx, &id, &ac_id, description, verification, evidence)?;
            let mut payload = json!({ "ok": true, "acceptance_criterion": ac });
            if let Some(v) = preflight_target.as_deref() {
                if let Some(warning) = shell_parse_preflight(v) {
                    payload["preflight_warning"] = warning;
                }
                if let Some(warning) = prose_verification_warn(v) {
                    payload["prose_warning"] = warning;
                }
            }
            emit(format, &payload)
        }
        CriterionCmd::Bulk { id, bulk } => {
            // M118 S1: bulk AC update. Read the JSON array from disk and
            // apply each fragment update through the same per-AC update
            // flow as `Update` (so shell-parse preflight, evidence
            // preflight, and fragment-only stdout semantics apply
            // uniformly). Empty array is a no-op. Missing id fails
            // fast with a per-id error so a typo in one element
            // doesn't half-apply.
            let payload = milestone::criterion_bulk_update(ctx, &id, &bulk)?;
            emit(format, &payload)
        }
        CriterionCmd::Remove { id, ac_id } => {
            let result = milestone::criterion_remove(ctx, &id, &ac_id)?;
            emit(format, &result)
        }
    }
}

/// M106 (S15): wall-clock cap for the verifier calls inside
/// `mp milestone complete`. Configurable via
/// `MP_COMPLETE_GLOBAL_DEADLINE_SECS`. Default 1800s (30 min) — at the
/// default per-AC `MP_VERIFY_TIMEOUT_SECS=300`, that's 17 verifications
/// × 300s = 5100s worst case; the global deadline catches runaway cases
/// (e.g., a verifier call that the bounded-join fix didn't actually
/// unblock) before they tie up a session indefinitely.
#[allow(dead_code)]
fn global_complete_deadline_secs() -> u64 {
    global_complete_deadline_dur().as_secs()
}

/// M107 (S3): same as `global_complete_deadline_secs` but returns a
/// `Duration` directly. Tests prefer this entry point so they can
/// inject a 1s deadline without mutating the process-global env var
/// (`std::env::set_var` is not safe under parallel tests).
fn global_complete_deadline_dur() -> Duration {
    std::env::var("MP_COMPLETE_GLOBAL_DEADLINE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(1800))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// M107 (S3) regression test for AC-02: confirms the
    /// defense-in-depth cancellation path — cooperative `AtomicBool`
    /// flip + `libc::killpg` on the registered child — actually
    /// reaps a wedged verifier subprocess within bounded wall-clock,
    /// and that the verifier worker can be `.join()`ed cleanly
    /// afterward (replacing the pre-fix `std::mem::forget`).
    ///
    /// Mirrors the exact sequence the orchestrator runs in
    /// `MilestoneCmd::Complete` when `rx.recv_timeout` returns
    /// `Err(Timeout)` (commands/milestone.rs ~line 245). Driving it
    /// directly avoids the cost of a full `mp milestone complete`
    /// CLI invocation with a deliberately-wedged milestone on disk.
    #[test]
    fn global_deadline_cancels_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

        // Spawn a hanging verifier worker that mirrors what
        // `ac_verify::execute` does:
        //   1. Spawn the child via `process_group(0)` so the child is
        //      its own process-group leader (pgid == pid).
        //   2. Register the child pid in the orchestrator's kill-set.
        //   3. Poll the cooperative cancel flag every 20ms; exit the
        //      poll loop when it flips (this is what `try_wait()`
        //      does between rounds in production).
        let verifier_handle = {
            let cancelled = Arc::clone(&cancelled);
            let child_pids = Arc::clone(&child_pids);
            thread::Builder::new()
                .name("test_wedged_verifier".into())
                .spawn(move || {
                    #[allow(clippy::zombie_processes)]
                    let child = std::process::Command::new("sleep")
                        .arg("30")
                        .process_group(0)
                        .spawn()
                        .expect("spawn sleep");
                    if let Ok(mut reg) = child_pids.lock() {
                        reg.push(child.id());
                    }
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        if cancelled.load(Ordering::Relaxed) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                })
                .expect("spawn test verifier")
        };

        // Wait for the verifier to register its child pid. This races
        // a sub-second setup; if the worker hasn't registered by then,
        // something is structurally wrong (not a flaky test).
        let setup_start = Instant::now();
        loop {
            let registered = child_pids.lock().map(|r| !r.is_empty()).unwrap_or(false);
            if registered {
                break;
            }
            assert!(
                setup_start.elapsed() < Duration::from_secs(1),
                "verifier never registered child pid"
            );
            thread::sleep(Duration::from_millis(10));
        }

        // Simulate the orchestrator's global-deadline abort path:
        //   (a) flip the cooperative cancel flag (cheapest signal),
        //   (b) `libc::killpg(pgid, SIGKILL)` every registered child
        //       (positive pgid; libc handles the internal negation),
        //   (c) `.join()` the verifier worker (must not block,
        //       because (a) makes the worker exit cleanly).
        let started = Instant::now();
        cancelled.store(true, Ordering::Relaxed);
        #[cfg(unix)]
        {
            let pids: Vec<u32> = child_pids.lock().map(|r| r.clone()).unwrap_or_default();
            assert!(!pids.is_empty(), "no child pid registered");
            for pid in pids {
                let pgid = pid as i32;
                // SAFETY: `killpg(pgid, sig)` is the libc wrapper for
                // the POSIX `killpg` syscall, which internally
                // translates to `kill(-pgid, sig)`. We pass the
                // positive `pgid` here; the libc binding handles the
                // negation. (Earlier versions of this test called
                // `libc::killpg(-pgid, SIGKILL)` — a double-invert
                // that sent the signal to pid=-pgid, an inert value.
                // The child appeared "still alive" on the post-kill
                // probe. Lesson recorded in the test.) ESRCH is
                // acceptable; we ignore the return code because the
                // assertion surface is the worker join and the
                // post-kill process-existence probe below.
                unsafe {
                    libc::killpg(pgid, libc::SIGKILL);
                }
            }
        }
        verifier_handle
            .join()
            .expect("verifier thread did not exit under cancellation");
        let elapsed = started.elapsed();

        // Pre-fix the worker was `mem::forget`-ed; there was no
        // observable "cleanup time" to assert. Post-fix the join
        // returns within ~1s under cancellation. A multi-second
        // hang here means the cooperative flag was not honored and
        // the worker is still spinning in its 20ms poll loop.
        assert!(
            elapsed < Duration::from_secs(1),
            "cleanup took {elapsed:?}; expected sub-second"
        );

        // Phase (1) above made the worker exit; the worker owned the
        // `sleep` child via `Command::Child` and dropped it on thread
        // exit without `wait()`-ing. The child is therefore either
        // still running (its parent thread exited but kept the pid)
        // or a zombie. The orchestrator's killpg must take it from
        // running → terminated. We then `waitpid` to reap and assert
        // it was signal-terminated.
        //
        // Why not `kill(pid, 0)` to probe liveness? Because macOS and
        // Linux both return 0 for zombies (`wait()` not yet called),
        // even though the process is no longer executing. The
        // authoritative "child is gone" signal is `waitpid` returning
        // the pid with `WIFSIGNALED(status)` true.
        #[cfg(unix)]
        {
            let pids: Vec<u32> = child_pids.lock().map(|r| r.clone()).unwrap_or_default();
            assert!(!pids.is_empty(), "no child pid registered");
            for pid in pids {
                // (a) killpg: ensure the child is signal-terminated
                // even if it's somehow still running. ESRCH here is
                // fine — it just means the child already died.
                let pgid = pid as i32;
                unsafe {
                    let _ = libc::killpg(pgid, libc::SIGKILL);
                }

                // Brief settle for signal delivery before the first
                // WNOHANG probe; on a fast box this is one scheduler
                // tick, on slow CI it's a few ms.
                thread::sleep(Duration::from_millis(50));

                // (b) waitpid: reap the child and assert it was
                // terminated by the SIGKILL we sent. WNOHANG means
                // we don't block; if the kernel hasn't delivered the
                // signal yet (e.g., it's a zombie we missed), we
                // retry until either we get a status or hit the
                // 2s ceiling (which is itself a test failure).
                let mut status: libc::c_int = 0;
                let reap_deadline = Instant::now() + Duration::from_secs(2);
                let reaped;
                loop {
                    let rc =
                        unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
                    if rc == pid as libc::pid_t {
                        reaped = true;
                        break;
                    }
                    if rc == -1 {
                        // ECHILD means the kernel has no record of
                        // this pid — it's been reaped by something
                        // else, or the test ran in a wrong sandbox.
                        // Either way the child is gone.
                        reaped = true;
                        break;
                    }
                    if Instant::now() > reap_deadline {
                        reaped = false;
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                assert!(reaped, "child pid={pid} not reaped within 2s after killpg");
                assert!(
                    libc::WIFSIGNALED(status),
                    "child pid={pid} exited but not by signal; status={status}"
                );
                assert_eq!(
                    libc::WTERMSIG(status),
                    libc::SIGKILL,
                    "child pid={pid} signaled but not by SIGKILL"
                );
            }
        }
    }
}
