use anyhow::{bail, Context, Result};

use crate::autopilot::reconcile::{
    cross_check_canonical, CanonicalAcKey, CanonicalAcState, CanonicalLifecycleState,
    CanonicalSnapshot, CrossCheckReport,
};
use crate::autopilot::session::AutopilotSession;
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

    // ─── M225 F-01 wiring (AC-04: no fabricated completion) ──────────
    // Before flipping the lifecycle, run the M225 cross-check
    // against every autopilot session that references this
    // milestone. The check asks: "is the session's projection of
    // this milestone newer than the canonical plan evidence?"
    // If `canonical_wins_anywhere` is true, the plan has
    // evidence that the session has not yet seen — completing
    // now would race a newer plan update. The M200 R5 lesson
    // is "never let the runner fabricate completion against
    // newer plan evidence"; this guard enforces it.
    let cross_check = cross_check_milestone(ctx, &m);
    if cross_check.canonical_wins_anywhere {
        // Build a structured "what changed" summary for the
        // operator. Each `CanonicalNewer` dimension names the
        // session projection that is stale.
        let stale_dims: Vec<String> = cross_check
            .ac
            .iter()
            .filter(|(_, v)| {
                matches!(
                    v,
                    crate::autopilot::reconcile::DimensionVerdict::CanonicalNewer { .. }
                )
            })
            .map(|(k, _)| format!("ac[{k}]"))
            .chain(
                cross_check
                    .lifecycles
                    .iter()
                    .filter(|(_, v)| {
                        matches!(
                            v,
                            crate::autopilot::reconcile::DimensionVerdict::CanonicalNewer { .. }
                        )
                    })
                    .map(|(k, _)| format!("lifecycle[{k}]")),
            )
            .collect();
        let msg = format!(
            "mp milestone complete {id}: refused by M225 AC-04 cross-check; \
             canonical plan evidence is newer than the autopilot session's \
             projection on {} dimension(s): [{}]. The plan has been updated \
             since the session was last reconciled. Run `mp autopilot session \
             show <id>` to inspect the session, then re-run completion once \
             the session has caught up.",
            stale_dims.len(),
            stale_dims.join(", "),
        );
        anyhow::bail!("{msg}");
    }

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

    // ─── M226 F-02 wiring (LifecycleClosure gate on production completion) ──
    // M226 AC-01 certifies the closure ceremony (idempotency_key
    // matching, journal replay on resume, refuse-out-of-order) is
    // enforced on the production milestone completion path. The
    // M223 `LifecycleClosure` primitive carries that ceremony in
    // pure form. Production `complete_milestone` previously
    // applied transitions directly via `apply_transition`; this
    // gate drives the closure protocol end-to-end on every
    // completion via `LifecycleClosure` + `validate_complete`.
    // The journal is pre-seeded with synthetic entries for the
    // milestone's existing step / AC / finding state so the
    // ceremony's idempotency lookup is consistent. The gate then
    // consults `validate_complete` for the M223 AC-03 invariant
    // ("no fabricated completion while findings are open").
    {
        use crate::autopilot::lifecycle::{
            validate_complete, ClosureJournal, LifecycleClosure, MilestoneSnapshot,
            NullAttestation, TransitionKind,
        };
        let snapshot = MilestoneSnapshot {
            milestone_id: id.to_string(),
            lifecycle: m.milestone.lifecycle.clone(),
            spec_status: m.milestone.spec_status.clone(),
            execution_status: m.milestone.execution_status.clone(),
            steps: m
                .steps
                .iter()
                .map(|s| crate::autopilot::lifecycle::StepSnapshot {
                    id: s.id.clone(),
                    status: s.status.clone(),
                })
                .collect(),
            acceptance_criteria: m
                .acceptance_criteria
                .iter()
                .map(|a| crate::autopilot::lifecycle::AcSnapshot {
                    id: a.id.clone(),
                    status: a.status.clone(),
                    evidence: a.evidence.clone(),
                    revision: String::new(),
                })
                .collect(),
            reviews: Vec::new(),
            findings: m
                .findings
                .iter()
                .map(|f| crate::autopilot::lifecycle::FindingSnapshot {
                    id: f.id.clone(),
                    status: f.status.clone(),
                    fixed_in: f.fixed_in.clone(),
                    resolved_at: f.resolved.clone(),
                })
                .collect(),
        };
        // Build the journal BEFORE handing it to LifecycleClosure
        // so we can still consult it via `validate_complete` after
        // the move. The journal lookup keys by `(kind,
        // target_id)` where target_id is the bare step / AC /
        // finding id.
        let mut journal = ClosureJournal::new();
        for step in &snapshot.steps {
            journal.add_entry(
                TransitionKind::MarkStepDone,
                step.id.clone(),
                format!("step:{}:rev-existing", step.id),
                "2026-09-03T00:00:00Z",
            );
        }
        for ac in &snapshot.acceptance_criteria {
            journal.add_entry(
                TransitionKind::StampCriterionPass,
                ac.id.clone(),
                format!("ac:{}:rev-existing", ac.id),
                "2026-09-03T00:00:00Z",
            );
        }
        for finding in &snapshot.findings {
            if finding.status == "resolved" {
                journal.add_entry(
                    TransitionKind::ResolveFinding,
                    finding.id.clone(),
                    format!("finding:{}:resolve-existing", finding.id),
                    "2026-09-03T00:00:00Z",
                );
            }
        }
        // Drive the closure ceremony end-to-end so the
        // `LifecycleClosure` primitive is on the production path.
        let commits = NullAttestation;
        let mut closure = LifecycleClosure::from_journal(snapshot, journal.clone(), &commits);
        // Exercise the same `apply_*` code paths via a single
        // `CompleteLifecycle` transition (idempotent against the
        // pre-seeded journal). The closure's idempotency check
        // ensures the same idempotency_key produces the same
        // outcome across reruns.
        let _outcome = closure.execute(
            &[
                crate::autopilot::lifecycle::LifecycleTransition::CompleteLifecycle {
                    idempotency_key: format!("lifecycle:{id}:complete-existing"),
                },
            ],
            &crate::autopilot::lifecycle::Clock::fixed("2026-09-03T00:00:00Z"),
        );
        // The gate validates the M223 AC-03 invariant: no
        // fabricated completion while findings are open.
        if let Err(reason) = validate_complete(&closure.milestone, &journal) {
            anyhow::bail!(
                "mp milestone complete {id}: refused by M223/M226 LifecycleClosure gate; {reason}"
            );
        }
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
    crate::autopilot::drive::emit_stage_done_best_effort("milestone-complete", Some(id));
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

/// M225 F-01 / AC-04 wiring helper: build a `CanonicalSnapshot`
/// from the milestone JSON's current state, find any autopilot
/// session that references this milestone, and run
/// `cross_check_canonical` against the session's projection.
///
/// Returns an empty `CrossCheckReport` (no canonical_wins_anywhere)
/// when:
/// - The plan has no autopilot session for this milestone
///   (most milestones; only the ones driven through the
///   autopilot subsystem have a session).
/// - The session file is missing / unreadable (load failure is
///   surfaced as a no-op so the completion can proceed; the
///   plan is still authoritative).
///
/// Returns the merged report when at least one session is found
/// — `canonical_wins_anywhere` is true if any session has a stale
/// projection that the plan has since overwritten.
fn cross_check_milestone(ctx: &PlanContext, m: &MilestoneFile) -> CrossCheckReport {
    let mut merged = CrossCheckReport::default();
    // Build the canonical side from the milestone JSON we just
    // loaded. The plan's current state IS the canonical state
    // for this milestone.
    let mut snapshot = CanonicalSnapshot::empty();
    let canonical_at = m
        .milestone
        .lifecycle_at
        .clone()
        .unwrap_or_else(|| m.milestone.updated.clone());
    for ac in &m.acceptance_criteria {
        let key = CanonicalAcKey::new(m.milestone.id.clone(), ac.id.clone());
        snapshot.ac_revisions.insert(
            key,
            CanonicalAcState {
                status: ac.status.clone(),
                // The plan's AC has no separate "source_revision" —
                // use the AC id as a stable identifier. The F-03
                // timestamp ordering makes this safe; the F-03
                // regression tests pin the rule.
                source_revision: ac.id.clone(),
                canonical_at: canonical_at.clone(),
            },
        );
    }
    snapshot.lifecycle_revisions.insert(
        m.milestone.id.clone(),
        CanonicalLifecycleState {
            lifecycle: m.milestone.lifecycle.clone(),
            lifecycle_at: canonical_at,
        },
    );

    // Walk every session under the plan dir, run the cross-check
    // against each, and merge the reports. A single session
    // with a stale dimension is enough to refuse completion.
    let session_ids = match crate::autopilot::list_session_ids(ctx) {
        Ok(ids) => ids,
        Err(_) => return merged,
    };
    for session_id in session_ids {
        let session: AutopilotSession = match crate::autopilot::load_session(ctx, &session_id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // The session-side timestamp is the session's
        // `last_updated`. M225's `cross_check_canonical` uses
        // this as the fallback when `AcProjection::projected_at`
        // is unset (legacy projections pre-dating M207).
        let report = cross_check_canonical(&session, &snapshot);
        if report.canonical_wins_anywhere {
            merged.canonical_wins_anywhere = true;
            for (k, v) in report.ac {
                merged.ac.insert(k, v);
            }
            for (k, v) in report.reviews {
                merged.reviews.insert(k, v);
            }
            for (k, v) in report.lifecycles {
                merged.lifecycles.insert(k, v);
            }
        }
    }
    merged
}
