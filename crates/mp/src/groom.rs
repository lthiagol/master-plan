use serde::Serialize;

use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::plan_gaps;
use crate::store;
use crate::validate::{effective_execution_status, effective_spec_status};

#[derive(Debug, Serialize)]
pub struct GroomReport {
    pub milestone_id: String,
    pub needs_attention: bool,
    pub reasons: Vec<String>,
    pub gaps: plan_gaps::PlanGapsReport,
    pub next_commands: Vec<String>,
}

pub fn groom_milestone(ctx: &PlanContext, milestone_id: &str) -> anyhow::Result<GroomReport> {
    let gaps = plan_gaps::plan_gaps(ctx, milestone_id)?;
    let norm = paths::normalize_milestone_id(milestone_id);
    let mut reasons = Vec::new();

    if !gaps.ready {
        reasons.extend(gaps.blockers.clone());
    }

    let path = crate::milestone::load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;

    if matches!(
        effective_spec_status(&m).as_str(),
        "draft" | "interview" | "review"
    ) {
        reasons.push(format!("spec_status {}", effective_spec_status(&m)));
    }
    if effective_spec_status(&m) == "ready" && m.steps.is_empty() {
        reasons.push("approved spec with no steps".to_string());
    }
    if effective_execution_status(&m) == "blocked" {
        reasons.push(format!("blocked: {}", m.milestone.block_reason));
    }

    let mut next_commands = vec![
        format!("mp show milestone {norm} --summary"),
        format!("mp plan gaps {norm}"),
    ];
    if effective_spec_status(&m) == "ready" && m.steps.is_empty() {
        next_commands.push(format!("mp milestone decompose {norm}"));
    }
    if gaps.ready {
        next_commands.push(format!("mp milestone set-status {norm} in-progress"));
    }

    Ok(GroomReport {
        milestone_id: norm,
        needs_attention: !reasons.is_empty() || !gaps.ready,
        reasons,
        gaps,
        next_commands,
    })
}

pub fn plan_coverage(
    ctx: &PlanContext,
    milestone_id: &str,
) -> anyhow::Result<plan_gaps::CoverageReport> {
    let path = crate::milestone::load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    Ok(plan_gaps::coverage_report(&m))
}

pub fn milestone_matches_filter(
    m: &MilestoneFile,
    filter: &str,
    ctx: &PlanContext,
) -> anyhow::Result<bool> {
    // M100 ER-8: route all status comparisons through the canonical
    // helpers so migrated milestones whose raw fields are empty are
    // classified correctly.
    let spec = effective_spec_status(m);
    let exec = effective_execution_status(m);
    match filter {
        "all" => Ok(true),
        "pending" => {
            Ok(exec == "planned"
                && !m.steps.iter().any(|s| s.status == "in-progress" || s.status == "done"))
        }
        "in-progress" => {
            Ok(exec == "in-progress"
                || m.steps.iter().any(|s| s.status == "in-progress"))
        }
        "partial" => {
            Ok((m.steps.iter().any(|s| s.status == "done" || s.status == "in-progress")
                && exec != "done")
                || (spec == "ready" && m.steps.is_empty()))
        }
        "done" => Ok(exec == "done"),
        "blocked" => {
            Ok(exec == "blocked"
                || !validate_deps_done(ctx, m).unwrap_or(false))
        }
        "grooming" => {
            Ok(matches!(
                spec.as_str(),
                "draft" | "interview" | "review"
            ) || (spec == "ready" && m.steps.is_empty())
                // M100 ER-8 follow-up: route through
                // `effective_execution_status` so migrated milestones
                // whose raw field is empty register the blocked
                // overlay via the canonical helper.
                || exec == "blocked")
        }
        _ => anyhow::bail!(
            "unknown --filter preset '{filter}'; known: all, pending, in-progress, partial, done, blocked, grooming. Use --preset force-bypassed or --where <field><op><value> for other filters."
        ),
    }
}

fn validate_deps_done(ctx: &PlanContext, m: &MilestoneFile) -> anyhow::Result<bool> {
    let milestones = store::load_all_milestones(ctx)?;
    let done: std::collections::HashSet<String> = milestones
        .iter()
        // M100 ER-8 follow-up: route through
        // `effective_execution_status` so migrated milestones whose
        // raw field is empty register as done.
        .filter(|(_, m)| crate::validate::effective_execution_status(m) == "done")
        .map(|(_, m)| paths::normalize_milestone_id(&m.milestone.id))
        .collect();
    Ok(m.milestone.depends_on.iter().all(|dep| {
        dep.is_empty() || dep == "none" || done.contains(&paths::normalize_milestone_id(dep))
    }))
}
