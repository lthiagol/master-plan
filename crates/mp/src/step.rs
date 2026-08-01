use anyhow::{bail, Context, Result};

use crate::milestone::{self, load_milestone_path};
use crate::model::Step;
use crate::paths::PlanContext;
use crate::store;
use crate::validate::effective_spec_status;

const STEP_STATUSES: &[&str] = &["pending", "in-progress", "done", "skipped", "failed"];

pub struct AddStepInput {
    pub wp: String,
    pub id: Option<String>,
    pub after: Option<String>,
    pub action: String,
    pub files: Vec<String>,
    pub tests: String,
    pub done_when: String,
    pub covers_ac: Vec<String>,
}

pub fn add_step(ctx: &PlanContext, milestone_id: &str, input: AddStepInput) -> Result<Step> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;

    if !milestone::spec_status_allows_steps(&effective_spec_status(&m)) {
        bail!(
            "steps require spec_status ready or later (current: {})",
            effective_spec_status(&m)
        );
    }
    if !m.work_packages.iter().any(|wp| wp.id == input.wp) {
        bail!("work package {} not found", input.wp);
    }
    if input.action.is_empty() {
        bail!("--action is required");
    }

    if let Some(after_id) = &input.after {
        if !m.steps.iter().any(|s| s.id == *after_id) {
            bail!("step {after_id} not found");
        }
    }

    let step_id = match (&input.after, &input.id) {
        (Some(after_id), None) => {
            let suffix = next_split_child_suffix(&m.steps, after_id);
            format!("{after_id}.{suffix}")
        }
        (Some(_), Some(_)) => bail!("cannot use both --after and --id together"),
        (None, id) => id
            .clone()
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| next_step_id(&m.steps)),
    };

    if m.steps.iter().any(|s| s.id == step_id) {
        bail!("step {step_id} already exists");
    }

    let after_order = input
        .after
        .as_ref()
        .and_then(|after_id| m.steps.iter().find(|s| s.id == *after_id).map(|s| s.order));

    let order = match after_order {
        Some(ao) => {
            m.steps.iter_mut().for_each(|s| {
                if s.order > ao {
                    s.order += 1;
                }
            });
            ao + 1
        }
        None => next_step_order(&m.steps),
    };

    let step = Step {
        id: step_id.clone(),
        work_package: input.wp,
        order,
        action: input.action,
        files: input.files,
        tests: input.tests,
        done_when: input.done_when,
        status: "pending".to_string(),
        covers_ac: input.covers_ac,
        depends_on_steps: vec![],
        ..Default::default()
    };

    m.steps.push(step.clone());
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(step)
}

/// Read-only: return a single step as a JSON value. No milestone write — agents
/// should never need to load the whole document to inspect one step.
/// (M93 AC-02.)
pub fn show_step(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
) -> Result<serde_json::Value> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    let step = m
        .steps
        .iter()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found in milestone {milestone_id}"))?;
    Ok(serde_json::to_value(step)?)
}

/// Mutator: remove a step. Refuses when another step's `depends_on_steps`
/// includes the target id. (M93 AC-06.)
pub fn remove_step(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
) -> Result<serde_json::Value> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;

    // Guard 1: another step's depends_on_steps includes the target.
    let dependents: Vec<String> = m
        .steps
        .iter()
        .filter(|s| s.id != step_id && s.depends_on_steps.iter().any(|d| d == step_id))
        .map(|s| s.id.clone())
        .collect();
    if !dependents.is_empty() {
        bail!(
            "cannot remove step {step_id} from milestone {milestone_id}: depended on by step(s) {}",
            dependents.join(", ")
        );
    }

    // Guard 2: the target id is a parent of one or more split children
    // (e.g. removing S1 when S1.1 and S1.2 exist would orphan them).
    let child_prefix = format!("{step_id}.");
    let children: Vec<String> = m
        .steps
        .iter()
        .filter(|s| s.id.starts_with(&child_prefix))
        .map(|s| s.id.clone())
        .collect();
    if !children.is_empty() {
        bail!(
            "cannot remove step {step_id}: split children exist ({})",
            children.join(", ")
        );
    }

    let mut m = m;
    let removed = {
        let pos = m
            .steps
            .iter()
            .position(|s| s.id == step_id)
            .with_context(|| format!("step {step_id} not found in milestone {milestone_id}"))?;
        m.steps.remove(pos);
        step_id.to_string()
    };
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(serde_json::json!({ "ok": true, "removed": removed }))
}

pub fn set_step_status(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    status: &str,
) -> Result<Step> {
    if !STEP_STATUSES.contains(&status) {
        bail!(
            "invalid step status: {status} (expected one of: {})",
            STEP_STATUSES.join(", ")
        );
    }

    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    if status == "in-progress" {
        let step = m
            .steps
            .iter()
            .find(|s| s.id == step_id)
            .with_context(|| format!("step {step_id} not found"))?;
        if !step.depends_on_steps.is_empty() {
            let deps_ok = step.depends_on_steps.iter().all(|dep| {
                m.steps
                    .iter()
                    .find(|s| s.id == *dep)
                    .map(|s| s.status == "done" || s.status == "skipped")
                    .unwrap_or(false)
            });
            if !deps_ok {
                bail!("step {step_id} has unfinished depends_on_steps");
            }
        }
    }

    let mut m = store::load_milestone(&path)?;
    let step = m
        .steps
        .iter_mut()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found"))?;
    step.status = status.to_string();
    if status == "done" || status == "skipped" {
        crate::step_claim::clear_claim(step);
    }
    let out = step.clone();

    // When all steps are done/skipped, auto-close referenced TW/BF track items.
    // execution_status stays in-progress until `mp milestone complete` sets
    // verified + done together (G7: done requires verified).
    if status == "done" {
        let all_done = m
            .steps
            .iter()
            .all(|s| s.status == "done" || s.status == "skipped");
        if all_done {
            for step in &m.steps {
                if step.status == "done" {
                    let _ = auto_close_track_refs(ctx, &step.action);
                }
            }
        }
    }

    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(out)
}

#[derive(Debug, Default)]
pub struct UpdateStepInput {
    pub action: Option<String>,
    pub files: Option<Vec<String>>,
    pub tests: Option<String>,
    pub done_when: Option<String>,
    pub covers_ac: Option<Vec<String>>,
    pub work_package: Option<String>,
    pub depends_on_steps: Option<Vec<String>>,
    /// M111 S1: per-step run evidence (last successful run note). Mirrors
    /// `mp milestone ac update --evidence` on the AC side.
    pub evidence: Option<String>,
}

pub fn update_step(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    input: UpdateStepInput,
) -> Result<Step> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    let step = m
        .steps
        .iter_mut()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found"))?;

    if let Some(action) = input.action {
        step.action = action;
    }
    if let Some(files) = input.files {
        step.files = files;
    }
    if let Some(tests) = input.tests {
        step.tests = tests;
    }
    if let Some(done_when) = input.done_when {
        step.done_when = done_when;
    }
    if let Some(covers_ac) = input.covers_ac {
        step.covers_ac = covers_ac;
    }
    if let Some(wp) = input.work_package {
        step.work_package = wp;
    }
    if let Some(deps) = input.depends_on_steps {
        step.depends_on_steps = deps;
    }
    if let Some(ev) = input.evidence {
        step.evidence = ev;
    }

    let out = step.clone();
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(out)
}

pub fn split_step(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Vec<Step>> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    let parent = m
        .steps
        .iter()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found"))?
        .clone();

    if parent.status == "done" {
        bail!("cannot split done step {step_id}");
    }

    let id1 = format!("{step_id}.{}", next_split_child_suffix(&m.steps, step_id));
    let mut with_first = m.steps.clone();
    with_first.push(Step {
        id: id1.clone(),
        ..Default::default()
    });
    let id2 = format!(
        "{step_id}.{}",
        next_split_child_suffix(&with_first, step_id)
    );

    let mut created = Vec::new();
    for (child_id, part) in [(id1, 1u8), (id2, 2u8)] {
        if m.steps.iter().any(|s| s.id == child_id) {
            continue;
        }
        let child = Step {
            id: child_id.clone(),
            work_package: parent.work_package.clone(),
            order: next_step_order(&m.steps),
            action: format!("{} (part {part})", parent.action),
            files: parent.files.clone(),
            tests: parent.tests.clone(),
            done_when: parent.done_when.clone(),
            status: "pending".to_string(),
            covers_ac: parent.covers_ac.clone(),
            depends_on_steps: parent.depends_on_steps.clone(),
            ..Default::default()
        };
        m.steps.push(child.clone());
        created.push(child);
    }

    m.steps
        .sort_by(|a, b| crate::path_engine::compare_step_ids(&a.id, &b.id));
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(created)
}

fn next_split_child_suffix(steps: &[Step], parent_id: &str) -> u32 {
    let prefix = format!("{parent_id}.");
    let mut max = 0u32;
    for step in steps {
        if step.id == parent_id {
            continue;
        }
        if let Some(rest) = step.id.strip_prefix(&prefix) {
            if let Some(n) = rest.split('.').next().and_then(|p| p.parse().ok()) {
                max = max.max(n);
            }
        }
    }
    max + 1
}

pub fn next_step_id(steps: &[Step]) -> String {
    let mut max = 0u32;
    for step in steps {
        if let Some(n) = outline_major(&step.id) {
            max = max.max(n);
        }
    }
    format!("S{}", max + 1)
}

fn next_step_order(steps: &[Step]) -> u32 {
    steps.iter().map(|s| s.order).max().unwrap_or(0) + 1
}

fn outline_major(id: &str) -> Option<u32> {
    let rest = id.strip_prefix('S')?;
    rest.split('.').next()?.parse().ok()
}

pub fn parse_csv_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

pub fn fail_step(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Step> {
    set_step_status(ctx, milestone_id, step_id, "failed")
}

pub fn infer_depends_on_steps(ctx: &PlanContext, milestone_id: &str) -> Result<serde_json::Value> {
    let path = crate::milestone::load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;

    let mut updated = 0u32;

    for wp in &m.work_packages {
        let wp_steps: Vec<usize> = m
            .steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.work_package == wp.id)
            .map(|(i, _)| i)
            .collect();

        for (j, &idx) in wp_steps.iter().enumerate() {
            if j == 0 {
                if !m.steps[idx].depends_on_steps.is_empty() {
                    m.steps[idx].depends_on_steps.clear();
                    updated += 1;
                }
            } else {
                let prev_id = m.steps[wp_steps[j - 1]].id.clone();
                if m.steps[idx].depends_on_steps != vec![prev_id.clone()] {
                    m.steps[idx].depends_on_steps = vec![prev_id];
                    updated += 1;
                }
            }
        }
    }

    if updated > 0 {
        m.milestone.updated = store::today();
        milestone::write_milestone_synced(ctx, &path, &m)?;
    }

    Ok(serde_json::json!({
        "ok": true,
        "milestone_id": milestone_id,
        "steps_updated": updated,
    }))
}

fn auto_close_track_refs(ctx: &PlanContext, action: &str) -> Result<()> {
    let refs = extract_track_refs(action);
    for track_ref in &refs {
        if let Some((kind, id)) = parse_track_ref(track_ref) {
            let track_path = ctx.track_path(&kind);
            if let Ok(mut track) = store::load_track(ctx, &kind) {
                if let Some(item) = track.items.iter_mut().find(|i| i.id == id) {
                    if item.status != "done" && item.status != "archived" {
                        item.status = "done".to_string();
                        item.completed = store::today();
                        let _ = store::write_track(ctx, &track_path, &track);
                    }
                }
            }
        }
    }
    Ok(())
}

fn extract_track_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let prefix = match c {
            'T' if chars.peek() == Some(&'W') => {
                let _ = chars.next();
                "TW"
            }
            'B' if chars.peek() == Some(&'F') => {
                let _ = chars.next();
                "BF"
            }
            _ => continue,
        };
        if chars.peek() != Some(&'-') {
            continue;
        }
        let _ = chars.next();
        let mut num = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                num.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        if !num.is_empty() {
            refs.push(format!("{}-{}", prefix, num));
        }
    }
    refs
}

fn parse_track_ref(track_ref: &str) -> Option<(String, String)> {
    let (prefix, num) = track_ref.split_once('-')?;
    if num.is_empty() {
        return None;
    }
    let kind = match prefix {
        "TW" => "tweak",
        "BF" => "bugfix",
        _ => return None,
    };
    Some((kind.to_string(), track_ref.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_step_id_increments_major_outline() {
        let steps = vec![
            Step {
                id: "S1".to_string(),
                ..Default::default()
            },
            Step {
                id: "S3.1".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(next_step_id(&steps), "S4");
    }
}
