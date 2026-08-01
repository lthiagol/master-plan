use std::collections::HashMap;

use crate::model::{AnnotationItem, MilestoneFile};
use crate::paths::PlanContext;
use crate::store;

use super::report::issue;
use super::report::ValidationIssue;

/// G1: `in-progress` requires `spec_status` ready or later (legacy read).
/// For already-migrated milestones, the equivalent is `lifecycle` >= approved
/// AND the overlay execution status is in-progress (via `effective_lifecycle`).
/// The legacy field is checked directly so a legacy-shape milestone with
/// `spec_status=draft + execution_status=in-progress` (an inconsistent state)
/// still trips G1.
///
/// M104 (B-44): route the early-return through `effective_spec_status` so the
/// gate fires the same before and after the legacy-shape → lifecycle
/// migration. Without this, a fully-migrated in-progress milestone (legacy
/// fields cleared, lifecycle="in-progress") would silently skip G1 even when
/// the derived `spec_status` is below `ready`.
pub(crate) fn check_gate_g1(m: &MilestoneFile) -> Vec<ValidationIssue> {
    use super::plan::effective_spec_status;
    if matches!(
        effective_spec_status(m).as_str(),
        "ready" | "implemented" | "verified"
    ) {
        return vec![];
    }
    vec![issue(
        "G1",
        "in-progress requires spec_status ready or later",
        Some(m.milestone.id.clone()),
    )]
}

/// G4: at least `min_out_of_scope` scope exclusions required.
pub(crate) fn check_gate_g4(m: &MilestoneFile, min_out_of_scope: usize) -> Vec<ValidationIssue> {
    if m.scope.out_of_scope.len() < min_out_of_scope {
        vec![issue(
            "G4",
            &format!("at least {min_out_of_scope} out-of-scope items required"),
            Some(m.milestone.id.clone()),
        )]
    } else {
        vec![]
    }
}

/// G8: milestone dependencies must be `done` before execution starts.
pub(crate) fn check_gate_g8(
    m: &MilestoneFile,
    exec_by_id: &HashMap<String, String>,
) -> Vec<ValidationIssue> {
    let id = m.milestone.id.clone();
    let mut errors = Vec::new();
    for dep in &m.milestone.depends_on {
        if dep.is_empty() || dep == "none" {
            continue;
        }
        let dep_id = crate::paths::normalize_milestone_id(dep);
        let dep_done = exec_by_id
            .get(&dep_id)
            .map(|s| s == "done")
            .unwrap_or(false);
        if !dep_done {
            errors.push(issue(
                "G8",
                &format!("dependency {dep_id} is not done"),
                Some(id.clone()),
            ));
        }
    }
    errors
}

/// G14: pending `approval-request` annotations block the milestone.
pub(crate) fn check_gate_g14(
    annotations: &[AnnotationItem],
    milestone_id: &str,
    issue_milestone: Option<String>,
) -> Vec<ValidationIssue> {
    let norm = crate::paths::normalize_milestone_id(milestone_id);
    let prefix = format!("M{}", norm);
    let open_approvals: Vec<&AnnotationItem> = annotations
        .iter()
        .filter(|a| {
            a.kind == "approval-request"
                && a.status != "resolved"
                && (a.target == prefix || a.target.starts_with(&format!("{}/", prefix)))
        })
        .collect();
    if open_approvals.is_empty() {
        vec![]
    } else {
        vec![issue(
            "G14",
            &format!(
                "milestone {norm} has {} pending approval annotation(s): {}",
                open_approvals.len(),
                open_approvals
                    .iter()
                    .map(|a| a.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            issue_milestone,
        )]
    }
}

/// Gates required before `review` or later spec promotion (G3–G4).
pub fn validate_milestone_review(
    m: &MilestoneFile,
    min_out_of_scope: usize,
) -> Vec<ValidationIssue> {
    let id = m.milestone.id.clone();
    let mut errors = Vec::new();

    if m.acceptance_criteria.is_empty() {
        errors.push(issue(
            "G3",
            "acceptance criteria required for review",
            Some(id.clone()),
        ));
    }
    errors.extend(check_gate_g4(m, min_out_of_scope));

    errors
}

/// Gates required before `mp milestone approve` (G2–G4).
pub fn validate_milestone_ready(
    m: &MilestoneFile,
    min_out_of_scope: usize,
) -> Vec<ValidationIssue> {
    let mut errors = validate_milestone_review(m, min_out_of_scope);
    let id = m.milestone.id.clone();

    for q in &m.open_questions {
        if q.status == "open" {
            errors.push(issue(
                "G2",
                &format!("open question {} unresolved at ready", q.id),
                Some(id.clone()),
            ));
        }
    }
    if m.intent.outcome.is_empty() {
        errors.push(issue(
            "G3",
            "intent.outcome is required for ready",
            Some(id.clone()),
        ));
    }
    if m.problem.description.is_empty() {
        errors.push(issue(
            "G3",
            "problem.description is required for ready",
            Some(id.clone()),
        ));
    }

    errors
}

pub fn validate_delta_complete(ctx: &PlanContext, m: &MilestoneFile) -> Vec<ValidationIssue> {
    crate::delta::validate_delta_milestone(ctx, m)
}

pub fn check_g14_approval_requests(ctx: &PlanContext, milestone_id: &str) -> Vec<ValidationIssue> {
    match store::load_annotations(ctx) {
        Ok(annotations) => check_gate_g14(
            &annotations.annotations,
            milestone_id,
            Some(crate::paths::normalize_milestone_id(milestone_id)),
        ),
        Err(_) => vec![],
    }
}

pub fn validate_milestone_start_execution(
    ctx: &PlanContext,
    m: &MilestoneFile,
) -> Vec<ValidationIssue> {
    let id = m.milestone.id.clone();
    let mut errors = check_gate_g1(m);

    let milestones = match store::load_all_milestones(ctx) {
        Ok(ms) => ms,
        Err(e) => {
            errors.push(issue(
                "E02",
                &format!("failed to load milestones for dependency check: {e:#}"),
                Some(id.clone()),
            ));
            return errors;
        }
    };
    let done_ids: HashMap<String, String> = milestones
        .iter()
        .map(|(_, m)| {
            (
                crate::paths::normalize_milestone_id(&m.milestone.id),
                // M124 (M104 ER-3): route through `effective_execution_status`
                // so dependency-done checks (G8) survive the lifecycle
                // migration — raw `execution_status` is empty on migrated
                // milestones, which previously produced false G8 "dependency
                // not done" errors on `mp milestone set-status <id> in-progress`.
                super::plan::effective_execution_status(m),
            )
        })
        .collect();

    errors.extend(check_gate_g8(m, &done_ids));

    errors
}
