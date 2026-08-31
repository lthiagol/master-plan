use std::collections::HashMap;

use crate::paths::PlanContext;
use crate::store;
use crate::track_kind;

use super::gates::{check_gate_g1, check_gate_g14, check_gate_g4, check_gate_g8};
use super::milestone_warnings::{
    step_deps_satisfied, uncovered_acceptance_criteria, validate_cross_refs, validate_milestone,
    validate_verification_field,
};
use super::report::{issue, report, ValidationReport};
use super::tracks::{validate_annotations, validate_track_drift, validate_track_item};

pub fn validate_plan(ctx: &PlanContext) -> anyhow::Result<ValidationReport> {
    if !ctx.plan_dir.is_dir() {
        return Ok(report(
            vec![issue("E00", "master-plan directory does not exist", None)],
            vec![],
        ));
    }
    let milestones = match store::load_all_milestones(ctx) {
        Ok(ms) => ms,
        Err(e) => {
            return Ok(report(
                vec![issue(
                    "E02",
                    &format!("failed to load milestones: {e:#}"),
                    None,
                )],
                vec![],
            ));
        }
    };
    validate_plan_with_milestones(ctx, &milestones)
}

/// Validate using a pre-loaded milestone snapshot (avoids a second directory scan).
pub fn validate_plan_with_milestones(
    ctx: &PlanContext,
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
) -> anyhow::Result<ValidationReport> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let cfg = match store::try_load_config(ctx) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(issue(
                "W50",
                &format!("config.json invalid or unreadable: {e:#}"),
                None,
            ));
            crate::config::ProjectConfig::default()
        }
    };
    let min_out_of_scope = cfg.min_out_of_scope();
    let strict_g10 = cfg.strictness() == "full";

    if !ctx.plan_dir.is_dir() {
        errors.push(issue("E00", "master-plan directory does not exist", None));
        return Ok(report(errors, warnings));
    }
    if !ctx.plan_dir.join("plan.json").exists() {
        errors.push(issue("E01", "plan.json is missing", None));
    }

    // Continue with the existing body — milestones already loaded.
    let exec_by_id: HashMap<String, String> = milestones
        .iter()
        .map(|(_, m)| {
            (
                crate::paths::normalize_milestone_id(&m.milestone.id),
                effective_execution_status(m),
            )
        })
        .collect();
    let mut all_milestone_ids: std::collections::HashSet<String> = milestones
        .iter()
        .map(|(_, m)| format!("M{}", crate::paths::normalize_milestone_id(&m.milestone.id)))
        .collect();
    // W43 fix: include archived milestone IDs so cross-references like
    // `M98` or `M99` (archived in commit 00a66a3) don't trip the
    // cross-ref validator. `load_all_milestones` excludes archive by
    // design (archived = soft-deleted), but cross-ref text describing a
    // historical predecessor is a legitimate author pattern, not a bug.
    //
    // Remediation 2026-07-05: count archive-load failures and surface them
    // as W51. Pre-fix these errors were silently swallowed, which meant
    // a corrupted archive entry could leave its ID out of the set and
    // cause every cross-reference to that milestone to mis-fire. Counting
    // + reporting gives the operator visibility without flipping the
    // outcome from warning to error.
    let mut archive_load_failures: Vec<(String, String)> = Vec::new();
    match store::list_archived_milestones(ctx) {
        Ok(archived) => {
            for path in archived {
                match store::load_milestone(&path) {
                    Ok(m) => {
                        all_milestone_ids.insert(format!(
                            "M{}",
                            crate::paths::normalize_milestone_id(&m.milestone.id)
                        ));
                    }
                    Err(e) => {
                        archive_load_failures.push((path.display().to_string(), format!("{e:#}")));
                    }
                }
            }
        }
        Err(e) => {
            archive_load_failures.push((
                ctx.archive_dir().join("milestones").display().to_string(),
                format!("{e:#}"),
            ));
        }
    }
    if !archive_load_failures.is_empty() {
        // W51: emitting one warning per failed load would be noisy
        // (a 200-file archive directory with bad perms would yield 200
        // warnings). Aggregate: one warning carries the count + the first
        // sample failure path for diagnostics.
        let count = archive_load_failures.len();
        let (sample_path, _) = &archive_load_failures[0];
        warnings.push(issue(
            "W51",
            &format!(
                "archived milestones: {count} file(s) failed to load; cross-ref check may miss references (first failure: {sample_path})"
            ),
            None,
        ));
    }

    // W43 remediation 2026-07-05: pre-build a milestone-id → step-ids map
    // so the cross-ref validator can resolve cross-milestone pairs like
    // "M106 S11" against the referenced milestone's step list. Built once
    // here, used per-milestone inside the loop. Includes archived milestones
    // so an AC referencing "see M98 (archived) S5" still resolves.
    let mut step_ids_by_milestone: std::collections::HashMap<
        String,
        std::collections::HashSet<String>,
    > = milestones
        .iter()
        .map(|(_, m)| {
            (
                format!("M{}", crate::paths::normalize_milestone_id(&m.milestone.id)),
                m.steps.iter().map(|s| s.id.clone()).collect(),
            )
        })
        .collect();
    // Include archive so cross-ref text with "M98 S5" resolves against
    // M98's step list. Archive load errors are already surfaced as W51
    // above; we continue populating here on a best-effort basis.
    if let Ok(archived) = store::list_archived_milestones(ctx) {
        for path in archived {
            if let Ok(m) = store::load_milestone(&path) {
                let key = format!("M{}", crate::paths::normalize_milestone_id(&m.milestone.id));
                let ids: std::collections::HashSet<String> =
                    m.steps.iter().map(|s| s.id.clone()).collect();
                step_ids_by_milestone.entry(key).or_insert(ids);
            }
        }
    }

    for (path, m) in milestones {
        let id = m.milestone.id.clone();
        validate_milestone(m, &mut warnings);
        if m.is_delta_kind() {
            errors.extend(crate::delta::validate_delta_milestone(ctx, m));
        }

        // W30: track-drift — a done step references a TW-/BF- item still pending/in-progress
        validate_track_drift(ctx, m, &mut warnings);

        // M100: gate evaluation against the unified lifecycle. Legacy
        // spec_status/execution_status still apply during the migration window
        // (via `effective_lifecycle` and the legacy field checks below).
        //
        // M104 (B-44): every gate read routes through `effective_spec_status`
        // / `effective_execution_status`, which prefer the legacy field if
        // set and otherwise derive the equivalent from the unified lifecycle.
        // This makes gates fire the same before and after the bulk legacy →
        // lifecycle migration: clearing the legacy fields is no longer a
        // load-bearing precondition for any gate.
        let lc = m.effective_lifecycle();
        let spec = effective_spec_status(m);
        let exec = effective_execution_status(m);
        if exec == "in-progress" {
            errors.extend(check_gate_g1(m));
        }
        if spec == "ready" {
            for q in &m.open_questions {
                if q.status == "open" {
                    errors.push(issue(
                        "G2",
                        &format!("open question {} unresolved at ready", q.id),
                        Some(id.clone()),
                    ));
                }
            }
        }
        if matches!(
            spec.as_str(),
            "review" | "ready" | "implemented" | "verified"
        ) {
            if m.acceptance_criteria.is_empty() {
                errors.push(issue(
                    "G3",
                    "acceptance criteria required for review",
                    Some(id.clone()),
                ));
            }
            errors.extend(check_gate_g4(m, min_out_of_scope));
        }
        if spec == "verified" {
            for ac in &m.acceptance_criteria {
                if ac.status != "passed" {
                    errors.push(issue(
                        "G6",
                        &format!("acceptance criterion {} not passed at verified", ac.id),
                        Some(id.clone()),
                    ));
                } else if ac.evidence.is_empty() {
                    errors.push(issue(
                        "G6",
                        &format!(
                            "acceptance criterion {} missing evidence at verified",
                            ac.id
                        ),
                        Some(id.clone()),
                    ));
                }
            }
        }
        if exec == "done" {
            // M200 F-13: the legacy triple invariant (exec=done requires
            // spec=verified) was tightened before M196 introduced the
            // `lifecycle=executed` state. M196 lets a milestone reach
            // exec=done via the new pipeline (lifecycle=executed,
            // spec_status=implemented) without ever passing through
            // spec=verified. The gate now accepts EITHER the legacy
            // triple OR the M196 executed+implemented combination.
            let legacy_ok = spec == "verified";
            let m196_executed_ok = lc == "executed" && spec == "implemented";
            if !legacy_ok && !m196_executed_ok {
                errors.push(issue(
                    "G7",
                    "execution_status done requires spec_status verified (or lifecycle=executed with spec_status implemented)",
                    Some(id.clone()),
                ));
            }
        }
        if (spec == "draft" || spec == "interview" || spec == "review")
            && m.has_implementation_plan()
        {
            errors.push(issue(
                "G5",
                "implementation plan before spec ready",
                Some(id.clone()),
            ));
        }
        // M100: same checks against lifecycle, for milestones that have been
        // migrated to the new field.
        if matches!(
            lc.as_str(),
            "groomed"
                | "approved"
                | "in-progress"
                | "done"
                | "self-reviewed"
                | "reviewed"
                | "complete"
                | "remediation"
        ) {
            if m.acceptance_criteria.is_empty() {
                errors.push(issue(
                    "G3",
                    "acceptance criteria required for review",
                    Some(id.clone()),
                ));
            }
            errors.extend(check_gate_g4(m, min_out_of_scope));
        }
        if lc == "complete" {
            for ac in &m.acceptance_criteria {
                if ac.status != "passed" {
                    errors.push(issue(
                        "G6",
                        &format!("acceptance criterion {} not passed at verified", ac.id),
                        Some(id.clone()),
                    ));
                } else if ac.evidence.is_empty() {
                    errors.push(issue(
                        "G6",
                        &format!(
                            "acceptance criterion {} missing evidence at verified",
                            ac.id
                        ),
                        Some(id.clone()),
                    ));
                }
            }
        }
        if matches!(lc.as_str(), "draft" | "groomed") && m.has_implementation_plan() {
            errors.push(issue(
                "G5",
                "implementation plan before spec ready",
                Some(id.clone()),
            ));
        }
        if lc == "in-progress" {
            errors.extend(check_gate_g8(m, &exec_by_id));
            for step in &m.steps {
                if step.status == "in-progress" && !step_deps_satisfied(step, &m.steps) {
                    errors.push(issue(
                        "G9",
                        &format!("step {} has unfinished depends_on_steps", step.id),
                        Some(id.clone()),
                    ));
                }
            }
            for ac_id in uncovered_acceptance_criteria(m) {
                let msg = issue(
                    "G10",
                    &format!("acceptance criterion {ac_id} has no covering step"),
                    Some(id.clone()),
                );
                if strict_g10 {
                    errors.push(msg);
                } else {
                    warnings.push(msg);
                }
            }
        }
        // M100: also fire when the lifecycle field is at in-progress for an
        // already-migrated milestone that may not have an execution_status
        // set, OR when execution_status=in-progress is set but the lifecycle
        // is at approved (legacy-shape inconsistency).
        //
        // M104 (B-44): `exec` is the effective execution_status (legacy or
        // lifecycle-derived), so this branch correctly de-duplicates against
        // the lifecycle-based block above instead of against the legacy field.
        let exec_is_ip = exec == "in-progress";
        let lc_is_ip = m.effective_lifecycle() == "in-progress";
        if (exec_is_ip || lc_is_ip) && !(exec_is_ip && lc == "in-progress") {
            errors.extend(check_gate_g8(m, &exec_by_id));
            for step in &m.steps {
                if step.status == "in-progress" && !step_deps_satisfied(step, &m.steps) {
                    errors.push(issue(
                        "G9",
                        &format!("step {} has unfinished depends_on_steps", step.id),
                        Some(id.clone()),
                    ));
                }
            }
            for ac_id in uncovered_acceptance_criteria(m) {
                let msg = issue(
                    "G10",
                    &format!("acceptance criterion {ac_id} has no covering step"),
                    Some(id.clone()),
                );
                if strict_g10 {
                    errors.push(msg);
                } else {
                    warnings.push(msg);
                }
            }
        }
        if m.has_implementation_plan() && strict_g10 {
            for step in &m.steps {
                if step.tests.is_empty() {
                    errors.push(issue(
                        "G10",
                        &format!("step {}.tests is empty", step.id),
                        Some(id.clone()),
                    ));
                }
            }
        }
        for ac in &m.acceptance_criteria {
            if !ac.verification.is_empty() {
                validate_verification_field(
                    &ac.verification,
                    &mut warnings,
                    Some(id.clone()),
                    &format!("AC {}", ac.id),
                );
            }
        }
        for step in &m.steps {
            if !step.tests.is_empty() {
                validate_verification_field(
                    &step.tests,
                    &mut warnings,
                    Some(id.clone()),
                    &format!("step {}", step.id),
                );
            }
        }
        if path.file_name().is_none() {
            warnings.push(issue("W02", "invalid milestone path", Some(id.clone())));
        }

        validate_cross_refs(m, &all_milestone_ids, &step_ids_by_milestone, &mut warnings);

        // BF-13 (M131): validate thread entries on every finding so an
        // invalid `at` timestamp is rejected on load/validate regardless
        // of how it was added. Previously this check lived only in
        // `add_finding_with_phase` (reviews.rs), so a thread entry
        // written any other way persisted unvalidated. Emit one warning
        // per offending finding (W53) so the operator can locate it.
        for finding in &m.findings {
            for entry in &finding.thread {
                if let Err(msg) = entry.validate() {
                    warnings.push(issue(
                        "W53",
                        &format!("finding {} has invalid thread entry: {}", finding.id, msg),
                        Some(id.clone()),
                    ));
                }
            }
        }
    }

    if let Ok(plan) = store::load_plan(ctx) {
        let index_ids: std::collections::HashSet<String> = plan
            .milestones
            .iter()
            .map(|e| crate::paths::normalize_milestone_id(&e.id))
            .collect();
        let file_ids: std::collections::HashSet<String> = milestones
            .iter()
            .map(|(_, m)| crate::paths::normalize_milestone_id(&m.milestone.id))
            .collect();

        if plan.milestones.is_empty() {
            // Index not built yet — mp sync populates it; no W01 until then.
        } else {
            for (_, m) in milestones {
                let id = crate::paths::normalize_milestone_id(&m.milestone.id);
                if !index_ids.contains(&id) {
                    warnings.push(issue(
                        "W01",
                        &format!("milestone {id} not listed in plan.json index"),
                        Some(id),
                    ));
                }
            }
            for entry in &plan.milestones {
                let id = crate::paths::normalize_milestone_id(&entry.id);
                if !file_ids.contains(&id) {
                    warnings.push(issue(
                        "W01",
                        &format!("plan.json index entry {id} has no milestone file"),
                        Some(id),
                    ));
                }
            }

            // W03: stale-value drift between index and milestone file
            for entry in &plan.milestones {
                let id = crate::paths::normalize_milestone_id(&entry.id);
                if let Some((_, m)) = milestones
                    .iter()
                    .find(|(_, m)| crate::paths::normalize_milestone_id(&m.milestone.id) == id)
                {
                    // M100: derive legacy status from lifecycle for drift
                    // comparison so migrated milestones don't false-positive.
                    let (file_spec, file_exec) = derive_legacy_status_for_w03(m);
                    if file_spec != entry.spec_status {
                        warnings.push(issue(
                            "W03",
                            &format!(
                                "milestone {id} index spec_status=\"{}\" does not match file spec_status=\"{}\"",
                                entry.spec_status, file_spec
                            ),
                            Some(id.clone()),
                        ));
                    }
                    if file_exec != entry.execution_status {
                        warnings.push(issue(
                            "W03",
                            &format!(
                                "milestone {id} index execution_status=\"{}\" does not match file execution_status=\"{}\"",
                                entry.execution_status, file_exec
                            ),
                            Some(id.clone()),
                        ));
                    }
                    if m.milestone.title != entry.title {
                        warnings.push(issue(
                            "W03",
                            &format!(
                                "milestone {id} index title=\"{}\" does not match file title=\"{}\"",
                                entry.title, m.milestone.title
                            ),
                            Some(id.clone()),
                        ));
                    }
                }
            }
        }
    }

    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(track) = store::load_track(ctx, kind) {
            for item in &track.items {
                if item.status == "archived" {
                    continue;
                }
                validate_track_item(item, kind, &mut errors);
            }
        }
    }

    // R1: Validate annotations; G14: approval-request gate
    let annotations = match store::load_annotations(ctx) {
        Ok(a) => Some(a),
        Err(e) => {
            errors.push(issue(
                "E03",
                &format!("failed to load annotations: {e:#}"),
                None,
            ));
            None
        }
    };
    if let Some(ref annotations) = annotations {
        validate_annotations(annotations, &mut errors);
        for (_, m) in milestones {
            errors.extend(check_gate_g14(
                &annotations.annotations,
                &m.milestone.id,
                Some(m.milestone.id.clone()),
            ));
        }
    }

    // Review hygiene: warn on done-but-unreviewed milestones
    let reviews = crate::reviews::load_reviews_for_validate(ctx).unwrap_or_default();
    for (_, m) in milestones {
        // M104 (B-44): route through `effective_execution_status` so the
        // warning fires after the legacy-shape → lifecycle migration.
        if effective_execution_status(m) != "done" {
            continue;
        }
        let norm = crate::paths::normalize_milestone_id(&m.milestone.id);
        let has_review = reviews
            .iter()
            .any(|r| crate::paths::normalize_milestone_id(&r.milestone_id) == norm);
        if !has_review {
            warnings.push(issue(
                "W44",
                "milestone is done but has never been reviewed (run mp reviews pass)",
                Some(m.milestone.id.clone()),
            ));
        }
    }

    // M148 S3 (AC-04): W-LC-STUCK-EXEC — all steps done/skipped but
    // lifecycle still `in-progress`. By design step-done does not promote
    // execution_status or lifecycle (G7); agents must run
    // `mp milestone complete`. Without this nudge, the stuck state is silent.
    //
    // M148 ext-review F-02: if open self-phase findings are present,
    // `mp milestone complete` itself bails (see complete.rs gate on
    // has_open_self_findings). The stage-6 self-review window — all
    // steps done, lifecycle in-progress, findings filed with
    // `--phase self`, not yet resolved — is the normal pre-complete
    // state and the suggested command would fail. Branch the
    // message to surface the resolver order so the user gets an
    // actionable instruction, not a misleading "run complete".
    for (_, m) in milestones {
        if m.milestone.cancelled {
            continue;
        }
        if m.milestone.lifecycle != "in-progress" {
            continue;
        }
        if m.steps.is_empty() {
            continue;
        }
        let all_steps_closed = m
            .steps
            .iter()
            .all(|s| s.status == "done" || s.status == "skipped");
        if !all_steps_closed {
            continue;
        }
        if m.has_open_self_findings() {
            let open_count = m
                .findings
                .iter()
                .filter(|f| {
                    f.status == "open"
                        && (f.phase == crate::model::FINDING_PHASE_SELF || f.phase.is_empty())
                })
                .count();
            warnings.push(issue(
                "W-LC-STUCK-EXEC",
                &format!(
                    "milestone {} has all steps done/skipped but lifecycle is still \"in-progress\"; \
                     resolve the {open_count} open self-phase finding(s) via `mp reviews finding resolve {} <F-XX>`, \
                     then run `mp milestone complete {} --evidence \"...\"`",
                    m.milestone.id, m.milestone.id, m.milestone.id
                ),
                Some(m.milestone.id.clone()),
            ));
            continue;
        }
        warnings.push(issue(
            "W-LC-STUCK-EXEC",
            &format!(
                "milestone {} has all steps done/skipped but lifecycle is still \"in-progress\"; \
                 run `mp milestone complete {} --evidence \"...\"` to transition to complete",
                m.milestone.id, m.milestone.id
            ),
            Some(m.milestone.id.clone()),
        ));
    }

    // M145 S2 (AC-03): W-LC-TERMINAL — legacy-shape triple
    // (exec=done, spec=verified) but lifecycle is `done` (the executor's
    // "I'm done" write) without the ceremonial `mp milestone complete`
    // (or M145's auto-promote on `mp reviews pass`) having flipped it
    // to `complete`. Without this warning, downstream tooling that keys
    // on LIFECYCLE_TERMINAL (`complete`) reads the milestone as in-flight
    // and the human reader of `raul status` sees a contradiction between
    // lifecycle=done and the rest of the terminal-looking fields.
    //
    // M145 F-02 (external review): narrow the trigger to `lifecycle == "done"`
    // so the warning's auto-promote advice is always actionable. The prior
    // broader condition (any non-terminal lifecycle) fired for mid-review
    // states (self-reviewed/reviewed) where `mp reviews pass --verdict ok`
    // succeeds but does NOT promote — leaving the user with an unactionable
    // warning. Healthy `complete` and `cancelled` milestones stay silent.
    //
    // M166 ext-review F-09: widened the trigger to also fire on
    // `lifecycle == "complete"` when `effective_execution_status` is not
    // `{done, cancelled}`. Pre-M166 the surface was silent on this
    // regression (M166's reproduce via `mp milestone set-status <id>
    // blocked ; set-status <id> planned` lands a complete milestone at
    // execution_status='planned' with 0 warnings). Cancelled milestones
    // stay silent (the gate at plan.rs:627 keeps them skipped).
    for (_, m) in milestones {
        if m.milestone.cancelled {
            continue;
        }
        match m.milestone.lifecycle.as_str() {
            "done" => {
                // Legacy triple PRESENT (exec=done + spec=verified) but
                // lifecycle stuck at 'done' instead of 'complete'. Surface
                // so an operator (or `mp reviews pass --verdict ok`) can
                // flip the milestone to `complete`.
                if effective_execution_status(m) == "done" && effective_spec_status(m) == "verified"
                {
                    warnings.push(issue(
                        "W-LC-TERMINAL",
                        &format!(
                            "milestone {} has execution_status=done + spec_status=verified but lifecycle=\"done\"; \
                             run `mp reviews pass --verdict ok` (M145 auto-promote) or `mp milestone complete` to flip to complete",
                            m.milestone.id
                        ),
                        Some(m.milestone.id.clone()),
                    ));
                }
            }
            "complete" => {
                // M166 ext-review F-09: complete milestones must carry
                // execution_status in {done, cancelled}. Anything else
                // (planned, blocked, deferred, in-progress) signals a
                // write path that regressed the terminal execution_status
                // — the same shape the M166 block+unblock fix closed,
                // except reachable here through `mp milestone set-status`.
                let exec = effective_execution_status(m);
                if exec != "done" && exec != "cancelled" {
                    warnings.push(issue(
                        "W-LC-TERMINAL",
                        &format!(
                            "milestone {} has lifecycle=\"complete\" but execution_status=\"{}\"; \
                             complete milestones must carry execution_status in {{done, cancelled}}. \
                             Use `mp milestone reopen` then set the correct status, or fix the underlying \
                             write path that produced this state",
                            m.milestone.id, exec
                        ),
                        Some(m.milestone.id.clone()),
                    ));
                }
            }
            _ => {}
        }
    }

    // M142 AC-06: integrate the L5 evidence audit as an advisory
    // sub-check. L5 violations never gate `ok`; they surface in the
    // `l5_audit` section under each milestone. `mp validate --summary`
    // does not count L5 violations toward `error_count` and the exit
    // code stays 0 when only advisory L5 violations exist.
    let (l5_audit, l5_warnings) = build_l5_audit_section(ctx, milestones);
    warnings.extend(l5_warnings);
    let mut report = report(errors, warnings);
    report.l5_audit = Some(l5_audit);

    Ok(report)
}

/// M142: walk every milestone and run `l5_check` to surface
/// same-session cross-role hand-offs. Returns the aggregate audit
/// section + per-milestone warnings for milestones where the audit
/// itself errored (corrupt `reviews.json`, missing milestone, etc.).
/// The warnings are advisory — they surface in `mp validate --summary`
/// so the user sees the gap, not just silent omission.
fn build_l5_audit_section(
    ctx: &PlanContext,
    store: &[(std::path::PathBuf, crate::model::MilestoneFile)],
) -> (
    super::report::L5AuditSection,
    Vec<super::report::ValidationIssue>,
) {
    use super::report::{issue, L5AuditSection, L5MilestoneAudit};
    let mut milestones = Vec::new();
    let mut total_violations = 0usize;
    let mut all_ok = true;
    let mut warnings = Vec::new();

    for (_, m) in store {
        let norm = crate::paths::normalize_milestone_id(&m.milestone.id);
        match crate::reviews::l5_check(ctx, &norm) {
            Ok(audit) => {
                let count = audit.violations.len();
                total_violations += count;
                if !audit.ok {
                    all_ok = false;
                }
                milestones.push(L5MilestoneAudit {
                    milestone_id: norm,
                    ok: audit.ok,
                    violation_count: count,
                    total_handoffs: audit.summary.total_handoffs,
                    cross_role_handoffs: audit.summary.cross_role_handoffs,
                });
            }
            Err(e) => {
                // M142 L2 (review): surface the error as a warning
                // rather than silently dropping the milestone. The
                // user sees the audit gap in `mp validate --summary`
                // and can investigate the corrupt reviews.json or
                // missing milestone.
                warnings.push(issue(
                    "W-L5",
                    &format!("L5 audit skipped for milestone {norm}: {e:#}"),
                    Some(norm),
                ));
            }
        }
    }

    (
        L5AuditSection {
            ok: all_ok,
            violation_count: total_violations,
            milestones,
        },
        warnings,
    )
}

/// M104 (B-44): derive `spec_status` for gate reads, preferring the legacy
/// field if set and otherwise mapping `effective_lifecycle()` to its legacy
/// equivalent. Routes gate reads through a single helper so the gates fire
/// consistently before and after the legacy-shape → new-shape migration.
///
/// **M109 (C-3): asymmetry is intentional.** Legacy `spec_status` is authoritative
/// when set; otherwise derive from `effective_lifecycle()`. `execution_status`
/// is intentionally NOT consulted, even when the legacy `spec_status` and the
/// lifecycle-derived state disagree. This mirrors `derive_legacy_status_for_w03`
/// and `effective_execution_status` and preserves the pre-M104 gate semantics
/// for the inconsistent-legacy-shape case (e.g., `spec_status="draft" +
/// execution_status="in-progress"` keeps `spec_status="draft"`).
pub fn effective_spec_status(m: &crate::model::MilestoneFile) -> String {
    if !m.milestone.spec_status.is_empty() {
        return m.milestone.spec_status.clone();
    }
    match m.effective_lifecycle().as_str() {
        "draft" => "draft".to_string(),
        "groomed" => "review".to_string(),
        "approved" => "ready".to_string(),
        "in-progress" => "ready".to_string(),
        "done" | "self-reviewed" | "reviewed" | "remediation" => "implemented".to_string(),
        "complete" => "verified".to_string(),
        other => other.to_string(),
    }
}

/// M104 (B-44): like `effective_spec_status`, for `execution_status`. Overlay
/// flags (blocked/deferred/cancelled) win over the lifecycle mapping, mirroring
/// `derive_legacy_status_for_w03` and the helpers in sync.rs / plan_diff.rs.
pub fn effective_execution_status(m: &crate::model::MilestoneFile) -> String {
    if !m.milestone.execution_status.is_empty() {
        return m.milestone.execution_status.clone();
    }
    if m.milestone.blocked {
        return "blocked".to_string();
    }
    if m.milestone.deferred {
        return "deferred".to_string();
    }
    if m.milestone.cancelled {
        return "cancelled".to_string();
    }
    match m.effective_lifecycle().as_str() {
        "draft" | "groomed" | "approved" => "planned".to_string(),
        "in-progress" => "in-progress".to_string(),
        "done" | "self-reviewed" | "reviewed" | "complete" | "remediation" => "done".to_string(),
        _ => "planned".to_string(),
    }
}

/// M100: derive legacy `spec_status` / `execution_status` from the unified
/// lifecycle for drift comparison so migrated milestones don't false-positive.
/// Reuses `effective_spec_status` / `effective_execution_status` (M104).
/// Mirrors the helpers in sync.rs / plan_diff.rs.
fn derive_legacy_status_for_w03(m: &crate::model::MilestoneFile) -> (String, String) {
    (effective_spec_status(m), effective_execution_status(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MilestoneFile;

    fn m_with_legacy(spec: &str, exec: &str) -> MilestoneFile {
        let mut m = MilestoneFile::default();
        m.milestone.spec_status = spec.to_string();
        m.milestone.execution_status = exec.to_string();
        m
    }

    fn m_migrated(lifecycle: &str, blocked: bool, deferred: bool) -> MilestoneFile {
        let mut m = MilestoneFile::default();
        m.milestone.lifecycle = lifecycle.to_string();
        m.milestone.blocked = blocked;
        m.milestone.deferred = deferred;
        m
    }

    #[test]
    fn effective_spec_status_prefers_legacy_field_when_set() {
        // M104 (B-44): the legacy field is authoritative when populated. This
        // preserves pre-migration gate semantics for the inconsistent-shape
        // case (e.g. spec_status="draft" + execution_status="in-progress"
        // keeps spec_status="draft" so G1 still fires).
        let m = m_with_legacy("draft", "in-progress");
        assert_eq!(effective_spec_status(&m), "draft");
    }

    #[test]
    fn effective_spec_status_derives_from_lifecycle_when_legacy_empty() {
        // M124 (M104 ER-3): on a migrated milestone the raw `spec_status`
        // field is empty; the gate read must derive from lifecycle. Pinning
        // the lifecycle→spec_status map so the `done`-arm of
        // `set_execution_status` (which checks `effective_spec_status ==
        // "verified"`) keeps passing on migrated milestones.
        assert_eq!(
            effective_spec_status(&m_migrated("approved", false, false)),
            "ready"
        );
        assert_eq!(
            effective_spec_status(&m_migrated("in-progress", false, false)),
            "ready"
        );
        assert_eq!(
            effective_spec_status(&m_migrated("complete", false, false)),
            "verified",
            "complete lifecycle must map to verified so set_execution_status(done) gate passes"
        );
        assert_eq!(
            effective_spec_status(&m_migrated("draft", false, false)),
            "draft"
        );
    }

    #[test]
    fn effective_execution_status_prefers_legacy_field_when_set() {
        let m = m_with_legacy("ready", "in-progress");
        assert_eq!(effective_execution_status(&m), "in-progress");
    }

    #[test]
    fn effective_execution_status_overlay_wins_over_lifecycle() {
        // M104 (B-44): blocked/deferred/cancelled are overlays; even a
        // "complete" lifecycle surfaces as the overlay so consumers
        // (graph.rs blocked detection, digest.rs) read the active state.
        assert_eq!(
            effective_execution_status(&m_migrated("complete", true, false)),
            "blocked"
        );
        assert_eq!(
            effective_execution_status(&m_migrated("complete", false, true)),
            "deferred"
        );
    }

    #[test]
    fn effective_execution_status_derives_from_lifecycle_when_legacy_empty() {
        // M124 (M104 ER-3): `validate_milestone_start_execution` builds
        // `done_ids` from this helper; on a migrated milestone the legacy
        // `execution_status` is empty, so the helper MUST return "done"
        // for lifecycle=complete to keep G8 dependency checks from
        // false-firing.
        assert_eq!(
            effective_execution_status(&m_migrated("complete", false, false)),
            "done",
            "complete lifecycle must map to done so G8 sees the dep as done"
        );
        assert_eq!(
            effective_execution_status(&m_migrated("in-progress", false, false)),
            "in-progress"
        );
        assert_eq!(
            effective_execution_status(&m_migrated("approved", false, false)),
            "planned"
        );
    }
}
