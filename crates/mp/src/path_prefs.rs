use anyhow::{bail, Result};

use crate::model::{AdoptionOrder, ExecutionConfig, MilestoneFile, PlanFile};
use crate::paths::{self, PlanContext};
use crate::store;

pub fn pin_milestone(
    ctx: &PlanContext,
    milestone: &str,
    before: Option<&str>,
    rank: Option<u32>,
    reason: Option<&str>,
) -> Result<PlanFile> {
    let norm = paths::normalize_milestone_id(milestone);
    if before.is_none() && rank.is_none() {
        bail!("provide --before or --rank");
    }
    let mut plan = store::load_plan(ctx)?;
    plan.execution
        .adoption_order
        .retain(|o| paths::normalize_milestone_id(&o.milestone) != norm);
    plan.execution.adoption_order.push(AdoptionOrder {
        milestone: norm,
        before: before
            .map(paths::normalize_milestone_id)
            .unwrap_or_default(),
        rank: rank.unwrap_or(0),
        reason: reason.unwrap_or("").to_string(),
    });
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn unpin_milestone(ctx: &PlanContext, milestone: &str) -> Result<PlanFile> {
    let norm = paths::normalize_milestone_id(milestone);
    let mut plan = store::load_plan(ctx)?;
    let before = plan.execution.adoption_order.len();
    plan.execution
        .adoption_order
        .retain(|o| paths::normalize_milestone_id(&o.milestone) != norm);
    if plan.execution.adoption_order.len() == before {
        bail!("no pin found for milestone {norm}");
    }
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn list_pins(ctx: &PlanContext) -> Result<Vec<AdoptionOrder>> {
    let plan = store::load_plan(ctx)?;
    Ok(plan.execution.adoption_order.clone())
}

pub fn focus_milestone(
    ctx: &PlanContext,
    milestone: &str,
    through: Option<&str>,
) -> Result<PlanFile> {
    let norm = paths::normalize_milestone_id(milestone);
    let mut plan = store::load_plan(ctx)?;
    plan.execution.focus_milestone = norm;
    plan.execution.focus_through_step = through.unwrap_or("").to_string();
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn clear_focus(ctx: &PlanContext) -> Result<PlanFile> {
    let mut plan = store::load_plan(ctx)?;
    plan.execution.focus_milestone.clear();
    plan.execution.focus_through_step.clear();
    store::write_plan(ctx, &plan)?;
    Ok(plan)
}

pub fn sort_ready_milestones(
    ready: &mut Vec<MilestoneFile>,
    baseline: &[String],
    exec: &ExecutionConfig,
) {
    ready.sort_by(|a, b| compare_milestones(a, b, baseline, exec));
    apply_adoption_order(ready, &exec.adoption_order);
    apply_focus(ready, &exec.focus_milestone);
}

/// Sort ready milestone indices without cloning `MilestoneFile` values.
pub fn sort_ready_milestone_indices(
    ready: &mut Vec<usize>,
    milestones: &[(std::path::PathBuf, MilestoneFile)],
    baseline: &[String],
    exec: &ExecutionConfig,
) {
    ready.sort_by(|&a, &b| compare_milestones(&milestones[a].1, &milestones[b].1, baseline, exec));
    apply_adoption_order_indices(ready, milestones, &exec.adoption_order);
    apply_focus_indices(ready, milestones, &exec.focus_milestone);
}

fn compare_milestones(
    a: &MilestoneFile,
    b: &MilestoneFile,
    baseline: &[String],
    exec: &ExecutionConfig,
) -> std::cmp::Ordering {
    let a_active = a.milestone.execution_status == "in-progress";
    let b_active = b.milestone.execution_status == "in-progress";
    if a_active != b_active {
        return b_active.cmp(&a_active);
    }
    if exec.strategy == "priority_first" || exec.strategy == "resume_then_ready" {
        let a_pri = priority_rank(&a.milestone.priority);
        let b_pri = priority_rank(&b.milestone.priority);
        if a_pri != b_pri {
            return b_pri.cmp(&a_pri);
        }
    }
    let a_id = paths::normalize_milestone_id(&a.milestone.id);
    let b_id = paths::normalize_milestone_id(&b.milestone.id);
    let a_pos = baseline
        .iter()
        .position(|id| id == &a_id)
        .unwrap_or(usize::MAX);
    let b_pos = baseline
        .iter()
        .position(|id| id == &b_id)
        .unwrap_or(usize::MAX);
    a_pos.cmp(&b_pos)
}

/// M182 S1: rank a priority string for the `--sort priority`
/// comparator. Higher rank sorts first under ascending order
/// (urgent > high > normal > low). Unknown values fall through to
/// `normal` so a malformed priority doesn't sink the row to the
/// bottom — that would be a silent data-shape regression for
/// scripts that depend on priority-ranked ordering.
pub(crate) fn priority_rank(priority: &str) -> u8 {
    match priority {
        "urgent" => 4,
        "high" => 3,
        "normal" => 2,
        "low" => 1,
        _ => 2,
    }
}

fn apply_adoption_order(ready: &mut Vec<MilestoneFile>, orders: &[AdoptionOrder]) {
    let mut vec = std::mem::take(ready);
    for order in orders {
        let mid = paths::normalize_milestone_id(&order.milestone);
        let before = paths::normalize_milestone_id(&order.before);
        if before.is_empty() {
            continue;
        }
        let from_idx = vec
            .iter()
            .position(|m| paths::normalize_milestone_id(&m.milestone.id) == mid);
        let before_idx = vec
            .iter()
            .position(|m| paths::normalize_milestone_id(&m.milestone.id) == before);
        if let (Some(from), Some(to)) = (from_idx, before_idx) {
            if from == to {
                continue;
            }
            let item = vec.remove(from);
            let insert_at = if from < to { to - 1 } else { to };
            vec.insert(insert_at, item);
        }
    }
    *ready = vec;
}

fn apply_focus(ready: &mut Vec<MilestoneFile>, focus: &str) {
    if focus.is_empty() {
        return;
    }
    let focus_id = paths::normalize_milestone_id(focus);
    let mut vec = std::mem::take(ready);
    if let Some(idx) = vec
        .iter()
        .position(|m| paths::normalize_milestone_id(&m.milestone.id) == focus_id)
    {
        let item = vec.remove(idx);
        let active_count = vec
            .iter()
            .take(idx.min(vec.len()))
            .filter(|m| m.milestone.execution_status == "in-progress")
            .count();
        vec.insert(active_count, item);
        *ready = vec;
    }
}

fn apply_adoption_order_indices(
    ready: &mut Vec<usize>,
    milestones: &[(std::path::PathBuf, MilestoneFile)],
    orders: &[AdoptionOrder],
) {
    let mut vec = std::mem::take(ready);
    for order in orders {
        let mid = paths::normalize_milestone_id(&order.milestone);
        let before = paths::normalize_milestone_id(&order.before);
        if before.is_empty() {
            continue;
        }
        let from_idx = vec
            .iter()
            .position(|&i| paths::normalize_milestone_id(&milestones[i].1.milestone.id) == mid);
        let before_idx = vec
            .iter()
            .position(|&i| paths::normalize_milestone_id(&milestones[i].1.milestone.id) == before);
        if let (Some(from), Some(to)) = (from_idx, before_idx) {
            if from == to {
                continue;
            }
            let item = vec.remove(from);
            let insert_at = if from < to { to - 1 } else { to };
            vec.insert(insert_at, item);
        }
    }
    *ready = vec;
}

fn apply_focus_indices(
    ready: &mut Vec<usize>,
    milestones: &[(std::path::PathBuf, MilestoneFile)],
    focus: &str,
) {
    if focus.is_empty() {
        return;
    }
    let focus_id = paths::normalize_milestone_id(focus);
    let mut vec = std::mem::take(ready);
    if let Some(idx) = vec
        .iter()
        .position(|&i| paths::normalize_milestone_id(&milestones[i].1.milestone.id) == focus_id)
    {
        let item = vec.remove(idx);
        let active_count = vec
            .iter()
            .take(idx.min(vec.len()))
            .filter(|&&i| milestones[i].1.milestone.execution_status == "in-progress")
            .count();
        vec.insert(active_count, item);
        *ready = vec;
    }
}
