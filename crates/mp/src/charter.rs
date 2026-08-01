use anyhow::Result;
use serde::Serialize;

use crate::json_input;
use crate::model::PlanFile;
use crate::paths::PlanContext;
use crate::step;
use crate::store;

#[derive(Debug, Serialize)]
pub struct PlanShowReport {
    pub ok: bool,
    pub plan: PlanFile,
}

pub fn plan_show(ctx: &PlanContext) -> Result<PlanShowReport> {
    let plan = store::load_plan(ctx)?;
    Ok(PlanShowReport { ok: true, plan })
}

pub struct PlanSetInput {
    pub planning_status: Option<String>,
    pub planning_phase: Option<String>,
    pub target_version: Option<String>,
    pub stack: Option<Vec<String>>,
    pub description: Option<String>,
    pub name: Option<String>,
}

pub fn plan_set(ctx: &PlanContext, input: PlanSetInput) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    if let Some(v) = input.planning_status {
        plan.project.planning_status = v;
    }
    if let Some(v) = input.planning_phase {
        plan.project.planning_phase = v;
    }
    if let Some(v) = input.target_version {
        plan.project.target_version = v;
    }
    if let Some(v) = input.stack {
        plan.project.stack = v;
    }
    if let Some(v) = input.description {
        plan.project.description = v;
    }
    if let Some(v) = input.name {
        plan.project.name = v;
    }
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_goals_add(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    plan.charter.goals.push(text.to_string());
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

fn remove_by_text_or_index(list: &mut Vec<String>, text: &str) -> Result<bool> {
    if let Ok(idx) = text.parse::<usize>() {
        if idx == 0 || idx > list.len() {
            anyhow::bail!("index {} out of range (1..{})", idx, list.len());
        }
        list.remove(idx - 1);
        Ok(true)
    } else {
        let before = list.len();
        list.retain(|x| x != text);
        if list.len() == before {
            anyhow::bail!("text not found: {}", text);
        }
        Ok(true)
    }
}

fn set_from_json(list: &mut Vec<String>, json: &str) -> Result<()> {
    let raw = json_input::read_json_arg(json)?;
    let items: Vec<String> = serde_json::from_str(&raw)?;
    *list = items;
    Ok(())
}

pub fn plan_goals_remove(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    remove_by_text_or_index(&mut plan.charter.goals, text)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_goals_set(ctx: &PlanContext, json: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    set_from_json(&mut plan.charter.goals, json)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_nongoals_add(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    plan.charter.non_goals.push(text.to_string());
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_nongoals_remove(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    remove_by_text_or_index(&mut plan.charter.non_goals, text)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_nongoals_set(ctx: &PlanContext, json: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    set_from_json(&mut plan.charter.non_goals, json)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_principles_add(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    plan.charter.principles.push(text.to_string());
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_principles_remove(ctx: &PlanContext, text: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    remove_by_text_or_index(&mut plan.charter.principles, text)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn plan_principles_set(ctx: &PlanContext, json: &str) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    set_from_json(&mut plan.charter.principles, json)?;
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub struct MetricsSetInput {
    pub lines_of_code: Option<u64>,
    pub unit_tests: Option<u64>,
    pub integration_tests: Option<u64>,
    pub coverage_percent: Option<f64>,
}

pub fn metrics_show(ctx: &PlanContext) -> Result<crate::model::Metrics> {
    Ok(store::load_plan(ctx)?.metrics)
}

pub fn metrics_set(ctx: &PlanContext, input: MetricsSetInput) -> Result<crate::model::Metrics> {
    let mut plan = store::load_plan(ctx)?;
    if let Some(v) = input.lines_of_code {
        plan.metrics.lines_of_code = v;
    }
    if let Some(v) = input.unit_tests {
        plan.metrics.unit_tests = v;
    }
    if let Some(v) = input.integration_tests {
        plan.metrics.integration_tests = v;
    }
    if let Some(v) = input.coverage_percent {
        plan.metrics.coverage_percent = v;
    }
    plan.metrics.checked_at = store::today();
    store::write_plan(ctx, &plan)?;
    Ok(plan.metrics.clone())
}

pub fn parse_stack_csv(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|s| step::parse_csv_list(Some(s)))
}
