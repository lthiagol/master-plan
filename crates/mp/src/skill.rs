use anyhow::Result;
use serde::Serialize;

use crate::paths::PlanContext;
use crate::store;
use crate::validate;

#[derive(Debug, Serialize)]
pub struct SkillContextReport {
    pub project_name: String,
    pub profile: String,
    pub planning_status: String,
    pub active_milestones: Vec<MilestoneSummary>,
    pub pending_backlog: Vec<BacklogSummary>,
    pub inbox_count: usize,
}

#[derive(Debug, Serialize)]
pub struct MilestoneSummary {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct BacklogSummary {
    pub id: String,
    pub description: String,
    pub priority: String,
}

pub fn skill_context(ctx: &PlanContext) -> Result<SkillContextReport> {
    let plan = store::load_plan(ctx)?;
    let milestones = store::load_all_milestones(ctx)?;
    let backlog = store::load_backlog(ctx)?;
    let cfg = store::load_config(ctx);

    let active_milestones: Vec<MilestoneSummary> = milestones
        .into_iter()
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty are caught.
        .filter(|(_, m)| validate::effective_execution_status(m) == "in-progress")
        .map(|(_, m)| MilestoneSummary {
            id: m.milestone.id.clone(),
            title: m.milestone.title.clone(),
            // M100 ER-8 follow-up: route the displayed status
            // through `effective_execution_status` so migrated
            // milestones whose raw field is empty surface as
            // `in-progress` here, not as an empty string.
            status: validate::effective_execution_status(&m),
        })
        .collect();

    let pending_backlog: Vec<BacklogSummary> = backlog
        .items
        .iter()
        .filter(|b| b.status == "active")
        .map(|b| BacklogSummary {
            id: b.id.clone(),
            description: b.description.clone(),
            priority: b.priority.clone(),
        })
        .collect();

    Ok(SkillContextReport {
        project_name: plan.project.name,
        profile: cfg.workflow.profile.clone(),
        planning_status: plan.project.planning_status,
        active_milestones,
        pending_backlog,
        inbox_count: 0,
    })
}
