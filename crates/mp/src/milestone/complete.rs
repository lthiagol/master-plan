use anyhow::{bail, Context, Result};

use crate::model::MilestoneFile;
use crate::paths::PlanContext;
use crate::{store, validate};

use super::{load_milestone_path, with_milestone_mut_unlocked, write_milestone_synced};

const EXEC_STATUSES: &[&str] = &[
    "planned",
    "in-progress",
    "done",
    "blocked",
    "deferred",
    "cancelled",
];

pub fn set_execution_status(ctx: &PlanContext, id: &str, status: &str) -> Result<MilestoneFile> {
    if !EXEC_STATUSES.contains(&status) {
        anyhow::bail!(
            "invalid execution_status: {status} (expected one of: {})",
            EXEC_STATUSES.join(", ")
        );
    }
    let updated = with_milestone_mut_unlocked(ctx, id, |m| {
        // M166 (F-03 follow-up): for terminal milestones
        // (lifecycle=complete or cancelled=true), the only consistent
        // transitions are between done and cancelled — both are within
        // the terminal set. Any other `set_execution_status` call would
        // produce a state where execution_status (e.g. "blocked",
        // "planned", "deferred") contradicts the terminal lifecycle, and
        // `mp validate` would not detect the drift (W-LC-TERMINAL only
        // fires for the `done` raw spec_status). Bail before the field
        // write. Bail outlives M166 itself: callers that need to amend
        // a terminal milestone's evidence post-completion should use
        // `mp milestone update --verification` (M165), not this
        // primitive.
        if m.is_terminal() {
            match status {
                "done" | "cancelled" => {
                    // Both are terminal; the writes below align execution_status
                    // with lifecycle ("done" → lifecycle=complete + execution_status=done;
                    // "cancelled" → execution_status=cancelled + cancelled=true). Allowed.
                }
                _ => {
                    bail!(
                        "milestone {} is terminal (lifecycle={}, cancelled={}); \
                         refusing to set execution_status='{}' which would diverge from \
                         the terminal state. Use `mp milestone update --verification` \
                         to amend post-completion evidence, or reopen the milestone \
                         first (`mp milestone reopen`).",
                        id,
                        m.milestone.lifecycle,
                        m.milestone.cancelled,
                        status
                    );
                }
            }
        }
        let gate_errors = collect_set_execution_status_gates(ctx, m, status);
        if !gate_errors.is_empty() {
            anyhow::bail!(format_gate_errors(&gate_errors));
        }
        let lc_was = m.milestone.lifecycle.clone();
        let event = event_for_execution_status(m, status);
        super::spec::apply_transition(m, event)?;
        Ok((m.clone(), lc_was, m.milestone.lifecycle.clone()))
    })?;
    // M180 S3: emit a lifecycle-transition event when the canonical
    // lifecycle actually changed. The pre-mutation value is captured
    // here so the no-op write path doesn't double-emit.
    let (m, lc_was, lc_now) = updated;
    crate::activity::record_lifecycle_transition(ctx, &m.milestone.id, &lc_was, &lc_now)?;
    Ok(m)
}

/// Gates shared by live `set_execution_status` and dry-run preview.
pub(crate) fn collect_set_execution_status_gates(
    ctx: &PlanContext,
    m: &MilestoneFile,
    status: &str,
) -> Vec<validate::ValidationIssue> {
    let mut errors = Vec::new();
    if status == "in-progress" {
        errors.extend(validate::validate_milestone_start_execution(ctx, m));
    }
    if status == "done" && validate::effective_spec_status(m) != "verified" {
        errors.push(validate::issue(
            "verified-required",
            "execution_status done requires spec_status verified (use milestone complete)",
            Some(m.milestone.id.clone()),
        ));
    }
    errors
}

pub(crate) fn event_for_execution_status(
    m: &MilestoneFile,
    status: &str,
) -> crate::model::MilestoneEvent {
    match status {
        "planned" => {
            if m.milestone.deferred {
                crate::model::MilestoneEvent::Resume
            } else {
                crate::model::MilestoneEvent::Sync
            }
        }
        "in-progress" => crate::model::MilestoneEvent::Start,
        "done" => crate::model::MilestoneEvent::Complete,
        "blocked" => crate::model::MilestoneEvent::Block,
        "deferred" => crate::model::MilestoneEvent::Defer,
        "cancelled" => crate::model::MilestoneEvent::Cancel,
        _ => unreachable!("validated by caller"),
    }
}
pub fn criterion_pass(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    evidence: Option<String>,
) -> Result<crate::model::AcceptanceCriterion> {
    with_milestone_mut_unlocked(ctx, id, |m| {
        let ac = m
            .acceptance_criteria
            .iter_mut()
            .find(|ac| ac.id == ac_id)
            .with_context(|| format!("acceptance criterion {ac_id} not found"))?;
        ac.status = "passed".to_string();
        if let Some(e) = evidence {
            ac.evidence = e;
        }
        Ok(ac.clone())
    })
}

pub fn criterion_fail(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    reason: Option<String>,
) -> Result<crate::model::AcceptanceCriterion> {
    with_milestone_mut_unlocked(ctx, id, |m| {
        let ac = m
            .acceptance_criteria
            .iter_mut()
            .find(|ac| ac.id == ac_id)
            .with_context(|| format!("acceptance criterion {ac_id} not found"))?;
        ac.status = "failed".to_string();
        if let Some(r) = reason {
            ac.evidence = r;
        }
        Ok(ac.clone())
    })
}
// M118 CR (F-3): the audit-trail helpers used by `complete_milestone`'s
// three-phase block-annotation logic. The bracket pair
// `[block-cleared-on-complete:` ... `]` is parsed by content-bounded
// scanning: find the prefix, then the FIRST `]` after it. The dedup
// contract across M118/B-58/M118.5 is that `prior_block_reason` may
// legitimately contain the prefix string (e.g., a recursive block
// reason like "block because [block-cleared-on-complete: emergency]"),
// so we cannot rely on the prefix alone to delimit the annotation;
// the `]` is the anchor.
///
/// Extract the first `[block-cleared-on-complete: ...]` substring.
/// Returns `None` if the prefix isn't present. Single-pass; the dedup
/// contract guarantees at most one annotation in `verification.evidence`
/// at any time (the F-3 logic replaces rather than appends, so two
/// can never coexist).
fn extract_block_cleared_annotation(evidence: &str) -> Option<String> {
    let start = evidence.find("[block-cleared-on-complete:")?;
    let after = &evidence[start..];
    let rel_end = after.find(']')?;
    Some(evidence[start..start + rel_end + 1].to_string())
}

/// Strip any `[block-cleared-on-complete: ...]` substring from the
/// input. F-3's re-block + re-complete path uses this to remove the
/// stale annotation from `evidence_text` before applying the fresh
/// one. Mutates in place to keep the call site terse; copies are
/// cheap on small strings (evidence is short).
fn strip_block_cleared_annotation(evidence: &mut String) {
    if let Some(annotation) = extract_block_cleared_annotation(evidence) {
        // Replace the annotation (plus trailing separator if any)
        // with empty. The annotation lives at the start (we always
        // prefix it during the complete flow), so the result is
        // either empty string or `<rest>` trimmed of leading whitespace.
        let after_start = annotation.len();
        let rest = evidence[after_start..].trim_start().to_string();
        *evidence = rest;
    }
}
pub fn block_milestone(
    ctx: &PlanContext,
    id: &str,
    reason: &str,
    by: Option<&str>,
) -> Result<MilestoneFile> {
    let updated = with_milestone_mut_unlocked(ctx, id, |m| {
        super::spec::apply_transition(m, crate::model::MilestoneEvent::Block)?;
        m.milestone.block_reason = reason.to_string();
        m.milestone.blocked_at = store::now_rfc3339();
        m.milestone.blocked_by = by.unwrap_or("user").to_string();
        Ok(m.clone())
    })?;
    // M180 S4: emit one block event after the primary mutation
    // commits. Inside cmd_milestone's plan-write lock so the
    // in-lock best-effort primitive is correct (AC-04 / F-02: a
    // journal failure must not bubble up as a command failure).
    let _ = crate::activity::append_event_best_effort_unlocked(
        ctx,
        crate::activity::milestone_blocked_event(&updated.milestone.id, reason),
    )?;
    Ok(updated)
}

pub fn unblock_milestone(ctx: &PlanContext, id: &str) -> Result<MilestoneFile> {
    let updated = with_milestone_mut_unlocked(ctx, id, |m| {
        if !m.milestone.blocked {
            bail!("milestone is not blocked");
        }
        super::spec::apply_transition(m, crate::model::MilestoneEvent::Unblock)?;
        m.milestone.block_reason.clear();
        m.milestone.blocked_at.clear();
        m.milestone.blocked_by.clear();
        Ok(m.clone())
    })?;
    // M180 S4: emit one unblock event after the primary mutation
    // commits. AC-04 / F-02: in-lock best-effort variant; a journal
    // failure is swallowed + warned rather than propagated.
    let _ = crate::activity::append_event_best_effort_unlocked(
        ctx,
        crate::activity::milestone_unblocked_event(&updated.milestone.id),
    )?;
    Ok(updated)
}

pub fn defer_milestone(
    ctx: &PlanContext,
    id: &str,
    reason: &str,
    by: Option<&str>,
) -> Result<MilestoneFile> {
    with_milestone_mut_unlocked(ctx, id, |m| {
        super::spec::apply_transition(m, crate::model::MilestoneEvent::Defer)?;
        // M100 H2-remediation (M2): record the reason and actor in
        // the DEDICATED deferred fields only. Previously this
        // function also wrote `block_reason` / `blocked_at` /
        // Deferred and blocked are distinct overlays. Keep defer context
        // in `deferred_reason`; writing block fields would misclassify the
        // milestone and let a later block overwrite unrelated context.
        m.milestone.deferred_reason = reason.to_string();
        let _ = by; // accepted for CLI parity; the model has no defer-actor field
        Ok(m.clone())
    })
}

pub fn reopen_milestone(ctx: &PlanContext, id: &str) -> Result<MilestoneFile> {
    let updated = with_milestone_mut_unlocked(ctx, id, |m| {
        // Reopen uses the effective status because migrated milestones may
        // derive `done` from lifecycle while the legacy status is empty.
        if validate::effective_execution_status(m) != "done" {
            bail!("reopen requires execution_status done");
        }
        let lc_was = m.milestone.lifecycle.clone();
        super::spec::apply_transition(m, crate::model::MilestoneEvent::Reopen)?;
        Ok((m.clone(), lc_was))
    })?;
    let (m, lc_was) = updated;
    crate::activity::record_lifecycle_transition(
        ctx,
        &m.milestone.id,
        &lc_was,
        &m.milestone.lifecycle,
    )?;
    Ok(m)
}

pub fn complete_milestone(
    ctx: &PlanContext,
    id: &str,
    evidence: Option<String>,
    executor: Option<&str>,
    skip_review: bool,
) -> Result<MilestoneFile> {
    let path = load_milestone_path(ctx, id)?;
    let mut m = store::load_milestone(&path)?;
    // Capture the prior lifecycle so the committed mutation emits exactly
    // one transition event. Same-state evidence refreshes emit no event.
    let lc_was = m.milestone.lifecycle.clone();

    // Completion requires all self-review work to be resolved. Legacy
    // empty-phase findings count as self-review work.
    if m.has_open_self_findings() {
        let open_count = m
            .findings
            .iter()
            .filter(|f| {
                f.status == "open"
                    && (f.phase == crate::model::FINDING_PHASE_SELF || f.phase.is_empty())
            })
            .count();
        anyhow::bail!(
            "cannot complete {id}: {open_count} open self-phase finding(s) remain. \
             resolve them via `mp reviews finding resolve <finding-id>` first"
        );
    }

    let mut evidence_text = evidence
        .clone()
        .unwrap_or_else(|| "milestone complete".to_string());

    // M196: the review gate. A non-track milestone with no
    // `mp reviews pass --verdict ok` row reaches `executed` (the
    // executor's end-state), NOT terminal `complete`. Tracks bypass
    // the gate (their work is intentionally short-lived and
    // review-free). `--skip-review` is the recorded-debt escape
    // hatch — it does bypass the gate but writes `[skip-review]`
    // into evidence so the bypass is auditable.
    //
    // Note: `--force` is unrelated to this gate. Per F-01, `--force`
    // bypasses only the AC verification gate; reaching terminal
    // `complete` still requires either a review or `--skip-review`.
    let needs_review = !is_track_kind(&m) && !skip_review;
    let reviews = crate::reviews::load_reviews_for_validate(ctx).unwrap_or_default();
    let has_passing_review = reviews.iter().any(|r| {
        r.milestone_id == m.milestone.id
            && r.reviewed_at == m.milestone.updated  // legacy marker check
            && r.verdict == "ok"
    }) || reviews
        .iter()
        .any(|r| r.milestone_id == m.milestone.id && r.verdict == "ok");
    let promote_to_complete = !needs_review || has_passing_review || skip_review;
    if skip_review && !is_track_kind(&m) {
        let note = "[skip-review: review gate bypassed for terminal complete; recorded as debt]";
        evidence_text = match evidence.as_ref() {
            Some(_) => format!("{evidence_text} {note}"),
            None => note.to_string(),
        };
    }

    // Preserve prior block context in verification evidence before the
    // completion transition clears the block overlay.
    let prior_block_reason = m.milestone.block_reason.clone();

    for ac in &mut m.acceptance_criteria {
        if ac.status != "passed" {
            ac.status = "passed".to_string();
            if ac.evidence.is_empty() {
                ac.evidence = evidence_text.clone();
            }
        } else if evidence.is_some() {
            ac.evidence = evidence_text.clone();
        }
    }

    if m.is_delta_kind() {
        crate::delta::merge_delta_into_domain(ctx, &m)?;
    }

    if let Some(exec) = executor {
        if !exec.is_empty() {
            m.milestone.executed_by = exec.to_string();
        }
    }

    // M196: split the terminal-complete transition from the
    // executor's end-state. The ceremony chooses between the two
    // events based on the review gate decision above:
    //   * `MilestoneEvent::Complete`      → terminal `complete`
    //   * `MilestoneEvent::FinishExecution` → executor end-state `executed`
    // Tracks always take the terminal path; non-tracks that lack a
    // passing review take the `executed` end-state.
    let event = if promote_to_complete {
        crate::model::MilestoneEvent::Complete
    } else {
        crate::model::MilestoneEvent::FinishExecution
    };
    super::spec::apply_transition(&mut m, event)?;
    m.milestone.updated = store::today();
    m.verification.date = store::today();

    // Evidence carries exactly one block annotation. Current block context
    // wins; otherwise an existing annotation is retained.
    let new_annotation = if !prior_block_reason.is_empty() {
        Some(format!("[block-cleared-on-complete: {prior_block_reason}]"))
    } else {
        None
    };
    let carried_annotation = extract_block_cleared_annotation(&m.verification.evidence);
    let annotation_to_keep = match (new_annotation, carried_annotation) {
        (Some(new), _) => Some(new),
        (None, Some(carried)) => Some(carried),
        (None, None) => None,
    };

    // Only overwrite evidence when explicitly supplied or currently empty.
    if evidence.is_some() || m.verification.evidence.is_empty() {
        if evidence.is_some() {
            let mut base = evidence_text.clone();
            strip_block_cleared_annotation(&mut base);
            m.verification.evidence = match annotation_to_keep.as_ref() {
                None => base,
                Some(annotation) if base.is_empty() => annotation.clone(),
                Some(annotation) => format!("{annotation} {base}"),
            };
        } else {
            m.verification.evidence = annotation_to_keep.unwrap_or_default();
        }
    }

    // Terminal milestones cannot retain an active blocked overlay. The prior
    // reason remains in verification evidence through the annotation above.
    if !prior_block_reason.is_empty() {
        m.milestone.block_reason = String::new();
        m.milestone.blocked_by = String::new();
    }

    write_milestone_synced(ctx, &path, &m)?;
    // M150 S2: emit the stage-done sentinel to the herdr pane if
    // running inside one. Best-effort — a failure must not roll back
    // the just-completed write (that would punish agents running
    // `mp` from a non-herdr shell). The sentinel is the latency
    // optimization for `mp watch`; `plan.json` lifecycle=complete
    // remains the source of truth.
    crate::watch::emit_stage_done_best_effort("milestone-complete", Some(id));
    // M180 S3: emit one lifecycle-transition event. Same-state calls
    // (re-complete) emit nothing; the primary mutation (evidence
    // refresh) is the only side effect.
    crate::activity::record_lifecycle_transition(
        ctx,
        &m.milestone.id,
        &lc_was,
        &m.milestone.lifecycle,
    )?;
    Ok(m)
}

/// M196: true when the milestone opts into the track fast-path via
/// `change_kind: track`. Tracks bypass the review gate (their work
/// is short-lived and review-free; see docs/milestone-lifecycle/review.md).
///
/// Q-02: the field must be the exact string `"track"`. Empty / missing
/// / any other value is treated as a non-track milestone (fail closed).
fn is_track_kind(m: &MilestoneFile) -> bool {
    m.milestone.change_kind == "track"
}
pub(crate) fn format_gate_errors(errors: &[validate::ValidationIssue]) -> String {
    errors
        .iter()
        .map(|e| format!("{}: {}", e.code, e.message))
        .collect::<Vec<_>>()
        .join("; ")
}
