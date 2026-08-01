use anyhow::Result;
use serde::Serialize;

use crate::ac_verify;
use crate::paths::{self, PlanContext};
use crate::store;

#[derive(Debug, Serialize)]
pub struct ExecutionReport {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
    pub execution_status: String,
    pub spec_status: String,
    pub verification: VerificationSummary,
    pub steps: Vec<StepSummary>,
    pub acceptance_criteria: Vec<AcSummary>,
    pub execution_notes: Vec<ExecutionNote>,
}

#[derive(Debug, Serialize)]
pub struct VerificationSummary {
    pub date: String,
    pub evidence: String,
    pub branch: String,
}

#[derive(Debug, Serialize)]
pub struct StepSummary {
    pub id: String,
    pub status: String,
    pub tests: String,
    pub tests_kind: String,
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct AcSummary {
    pub id: String,
    pub status: String,
    pub description: String,
    pub verification: String,
    pub evidence: String,
}

#[derive(Debug, Serialize)]
pub struct ExecutionNote {
    pub id: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub created: String,
}

pub fn build_execution_report(ctx: &PlanContext, milestone_id: &str) -> Result<ExecutionReport> {
    let id = paths::normalize_milestone_id(milestone_id);
    let path = crate::milestone::load_milestone_path(ctx, &id)?;
    let m = store::load_milestone(&path)?;

    let steps = m
        .steps
        .iter()
        .map(|s| {
            let kind = ac_verify::classify(&s.tests);
            StepSummary {
                id: s.id.clone(),
                status: s.status.clone(),
                tests: s.tests.clone(),
                tests_kind: match kind {
                    ac_verify::Kind::Runnable => "runnable",
                    ac_verify::Kind::Manual => "manual",
                    ac_verify::Kind::Empty => "empty",
                }
                .to_string(),
                action: s.action.clone(),
            }
        })
        .collect();

    let acceptance_criteria = m
        .acceptance_criteria
        .iter()
        .map(|ac| AcSummary {
            id: ac.id.clone(),
            status: ac.status.clone(),
            description: ac.description.clone(),
            verification: ac.verification.clone(),
            evidence: ac.evidence.clone(),
        })
        .collect();

    let ideas = store::load_ideas(ctx).unwrap_or_default();
    let needle_display = paths::display_milestone_id(&id);
    let execution_notes: Vec<ExecutionNote> = ideas
        .ideas
        .iter()
        .filter(|idea| {
            idea.title.contains(&needle_display)
                || idea.title.contains(&id)
                || idea.body.contains(&needle_display)
                || idea.body.contains(&id)
                || idea.tags.iter().any(|t| t == &id || t == &needle_display)
        })
        .map(|idea| ExecutionNote {
            id: idea.id.clone(),
            title: idea.title.clone(),
            body: idea.body.clone(),
            source: idea.source.clone(),
            created: idea.created.clone(),
        })
        .collect();

    Ok(ExecutionReport {
        milestone_id: id.clone(),
        display: paths::display_milestone_id(&id),
        title: m.milestone.title.clone(),
        execution_status: m.milestone.execution_status.clone(),
        spec_status: m.milestone.spec_status.clone(),
        verification: VerificationSummary {
            date: m.verification.date.clone(),
            evidence: m.verification.evidence.clone(),
            branch: m.verification.branch.clone(),
        },
        steps,
        acceptance_criteria,
        execution_notes,
    })
}
