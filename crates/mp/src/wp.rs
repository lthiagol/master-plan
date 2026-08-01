use anyhow::{bail, Context, Result};

use crate::milestone::{self, load_milestone_path};
use crate::model::WorkPackage;
use crate::paths::PlanContext;
use crate::store;
use crate::validate::effective_spec_status;

pub struct AddWpInput {
    pub id: Option<String>,
    pub name: String,
    pub goal: String,
    pub rollback: String,
}

pub fn add_work_package(
    ctx: &PlanContext,
    milestone_id: &str,
    input: AddWpInput,
) -> Result<WorkPackage> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;

    if !milestone::spec_status_allows_steps(&effective_spec_status(&m)) {
        bail!(
            "work packages require spec_status ready or later (current: {})",
            effective_spec_status(&m)
        );
    }
    if input.name.is_empty() {
        bail!("--name is required");
    }

    let wp_id = input
        .id
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| next_wp_id(&m.work_packages));

    if m.work_packages.iter().any(|wp| wp.id == wp_id) {
        bail!("work package {wp_id} already exists");
    }

    let wp = WorkPackage {
        id: wp_id,
        name: input.name,
        goal: input.goal,
        rollback: input.rollback,
        steps: vec![],
    };
    m.work_packages.push(wp.clone());
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(wp)
}

fn next_wp_id(work_packages: &[WorkPackage]) -> String {
    let mut max = 0u32;
    for wp in work_packages {
        if let Some(n) = wp.id.strip_prefix("WP") {
            if let Ok(v) = n.parse::<u32>() {
                max = max.max(v);
            }
        }
    }
    format!("WP{}", max + 1)
}

pub fn wp_update(
    ctx: &PlanContext,
    milestone_id: &str,
    wp_id: &str,
    name: Option<String>,
    goal: Option<String>,
    rollback: Option<String>,
) -> Result<serde_json::Value> {
    use crate::milestone;
    let path = milestone::load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    let wp = m
        .work_packages
        .iter_mut()
        .find(|w| w.id == wp_id)
        .with_context(|| format!("work package {wp_id} not found in milestone {milestone_id}"))?;
    if let Some(n) = name {
        wp.name = n;
    }
    if let Some(g) = goal {
        wp.goal = g;
    }
    if let Some(r) = rollback {
        wp.rollback = r;
    }
    let wp_id = wp.id.clone();
    let wp_name = wp.name.clone();
    let wp_goal = wp.goal.clone();
    let wp_rollback = wp.rollback.clone();
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(
        serde_json::json!({ "id": wp_id, "name": wp_name, "goal": wp_goal, "rollback": wp_rollback }),
    )
}

/// Mutator: remove a work package. Fails when any step `work_package` references
/// the target id. (M93 AC-07.)
pub fn remove_work_package(
    ctx: &PlanContext,
    milestone_id: &str,
    wp_id: &str,
) -> Result<serde_json::Value> {
    use crate::milestone;
    let path = milestone::load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;

    // Guard: refuse if any step references this WP via work_package.
    let referencing: Vec<String> = m
        .steps
        .iter()
        .filter(|s| s.work_package == wp_id)
        .map(|s| s.id.clone())
        .collect();
    if !referencing.is_empty() {
        bail!(
            "cannot remove work package {wp_id} from milestone {milestone_id}: referenced by step(s) {}",
            referencing.join(", ")
        );
    }

    let mut m = m;
    let removed = {
        let pos = m
            .work_packages
            .iter()
            .position(|w| w.id == wp_id)
            .with_context(|| {
                format!("work package {wp_id} not found in milestone {milestone_id}")
            })?;
        m.work_packages.remove(pos);
        wp_id.to_string()
    };
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(serde_json::json!({ "ok": true, "removed": removed }))
}
