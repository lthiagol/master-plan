use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use serde::Serialize;

use crate::milestone::{self, load_milestone_path};
use crate::model::{MilestoneFile, WorkPackage};
use crate::paths::{self, PlanContext};
use crate::store;
use crate::validate::effective_spec_status;

#[derive(Debug, Clone, Serialize)]
pub struct PlanGapsReport {
    pub milestone_id: String,
    pub missing: Vec<GapItem>,
    pub blockers: Vec<String>,
    pub coverage: CoverageReport,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GapItem {
    pub field: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub ok: bool,
    pub acceptance_criteria: Vec<AcCoverage>,
    pub orphan_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcCoverage {
    pub id: String,
    pub covered_by: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecomposeReport {
    pub milestone_id: String,
    pub scaffolded: bool,
    pub gaps: PlanGapsReport,
    pub next_commands: Vec<String>,
}

pub fn plan_gaps(ctx: &PlanContext, milestone_id: &str) -> Result<PlanGapsReport> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    Ok(plan_gaps_for_milestone(&m))
}

pub fn plan_gaps_for_milestone(m: &MilestoneFile) -> PlanGapsReport {
    let id = m.milestone.id.clone();
    let mut missing = Vec::new();
    let mut blockers = Vec::new();

    if !milestone::spec_status_allows_steps(&effective_spec_status(m)) {
        blockers.push(format!(
            "spec_status {} — approve spec first",
            effective_spec_status(m)
        ));
    }
    if m.work_packages.is_empty() {
        let msg = "no work packages";
        missing.push(gap("work_packages", "blocker", msg));
        blockers.push(msg.to_string());
    }
    for wp in &m.work_packages {
        let wp_steps: Vec<_> = m.steps.iter().filter(|s| s.work_package == wp.id).collect();
        if wp_steps.is_empty() {
            let msg = format!("work package {} has no steps", wp.id);
            missing.push(gap(&format!("work_packages.{}", wp.id), "blocker", &msg));
            blockers.push(msg);
        }
        for step in wp_steps {
            if step.action.is_empty() {
                missing.push(gap(
                    &format!("steps.{}.action", step.id),
                    "major",
                    "step action required",
                ));
            }
            if step.done_when.is_empty() {
                missing.push(gap(
                    &format!("steps.{}.done_when", step.id),
                    "major",
                    "step done_when required",
                ));
            }
            if step.tests.is_empty() {
                missing.push(gap(
                    &format!("steps.{}.tests", step.id),
                    "major",
                    "step tests required",
                ));
            }
        }
    }

    let coverage = coverage_report(m);
    if !coverage.ok {
        blockers.push("acceptance criteria not fully covered".to_string());
        missing.push(gap("coverage", "major", "every AC needs a covering step"));
    }

    PlanGapsReport {
        milestone_id: id,
        ready: blockers.is_empty() && missing.iter().all(|g| g.severity != "blocker"),
        missing,
        blockers,
        coverage,
    }
}

pub fn coverage_report(m: &MilestoneFile) -> CoverageReport {
    let mut covered_by: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_ac_ids: HashSet<String> = HashSet::new();
    for ac in &m.acceptance_criteria {
        all_ac_ids.insert(ac.id.clone());
    }
    let mut orphan_steps = Vec::new();
    for step in &m.steps {
        if step.covers_ac.is_empty() {
            orphan_steps.push(step.id.clone());
        }
        for ac_id in &step.covers_ac {
            covered_by
                .entry(ac_id.clone())
                .or_default()
                .push(step.id.clone());
        }
    }
    let acceptance_criteria: Vec<AcCoverage> = m
        .acceptance_criteria
        .iter()
        .map(|ac| {
            let refs = covered_by.get(&ac.id).cloned().unwrap_or_default();
            AcCoverage {
                id: ac.id.clone(),
                covered_by: refs.clone(),
                status: if refs.is_empty() {
                    "uncovered".to_string()
                } else {
                    "covered".to_string()
                },
            }
        })
        .collect();
    let ok = m.acceptance_criteria.is_empty()
        || acceptance_criteria.iter().all(|ac| ac.status == "covered");
    CoverageReport {
        ok,
        acceptance_criteria,
        orphan_steps,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanScaffoldReport {
    pub milestone_id: String,
    pub scaffolded: bool,
    pub work_packages: Vec<String>,
    pub next_commands: Vec<String>,
}

pub fn plan_milestone(
    ctx: &PlanContext,
    milestone_id: &str,
    work_packages: Option<u32>,
) -> Result<PlanScaffoldReport> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    if !milestone::spec_status_allows_steps(&effective_spec_status(&m)) {
        bail!(
            "milestone plan requires spec_status ready or later (current: {})",
            effective_spec_status(&m)
        );
    }

    let mut scaffolded = false;
    if m.work_packages.is_empty() {
        let count = work_packages.unwrap_or(2).max(1) as usize;
        for i in 0..count {
            m.work_packages.push(WorkPackage {
                id: format!("WP{}", i + 1),
                name: if count == 1 {
                    "Main".to_string()
                } else {
                    format!("Work package {}", i + 1)
                },
                goal: String::new(),
                rollback: String::new(),
                steps: vec![],
            });
        }
        scaffolded = true;
    }

    if !m
        .work_packages
        .iter()
        .any(|wp| wp.id == "WP-close" || wp.name == "Closure")
    {
        m.work_packages.push(WorkPackage {
            id: "WP-close".to_string(),
            name: "Closure".to_string(),
            goal: "Verify tests, lint, and update plan".to_string(),
            rollback: String::new(),
            steps: vec![],
        });
        scaffolded = true;
    }

    if scaffolded {
        m.milestone.updated = store::today();
        milestone::write_milestone_synced(ctx, &path, &m)?;
    }

    let norm = paths::normalize_milestone_id(milestone_id);
    let wp_ids: Vec<String> = m.work_packages.iter().map(|wp| wp.id.clone()).collect();
    Ok(PlanScaffoldReport {
        milestone_id: norm.clone(),
        scaffolded,
        work_packages: wp_ids,
        next_commands: vec![
            format!("mp plan gaps {norm} --format json"),
            format!("mp milestone wp add {norm} --name \"...\" --goal \"...\""),
            format!("mp milestone step add {norm} --wp WP1 --action \"...\" --covers-ac AC-01"),
            "mp validate".to_string(),
        ],
    })
}

pub fn decompose_milestone(
    ctx: &PlanContext,
    milestone_id: &str,
    work_packages: Option<u32>,
) -> Result<DecomposeReport> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    if !milestone::spec_status_allows_steps(&effective_spec_status(&m)) {
        bail!(
            "decompose requires spec_status ready or later (current: {})",
            effective_spec_status(&m)
        );
    }

    let mut scaffolded = false;
    if m.work_packages.is_empty() {
        let count = work_packages.unwrap_or(1).max(1) as usize;
        for i in 0..count {
            m.work_packages.push(WorkPackage {
                id: format!("WP{}", i + 1),
                name: if count == 1 {
                    "Main".to_string()
                } else {
                    format!("Work package {}", i + 1)
                },
                goal: String::new(),
                rollback: String::new(),
                steps: vec![],
            });
        }
        scaffolded = true;
        m.milestone.updated = store::today();
        milestone::write_milestone_synced(ctx, &path, &m)?;
    }

    let gaps = plan_gaps_for_milestone(&m);
    let norm = paths::normalize_milestone_id(milestone_id);
    let mut next_commands = vec![
        format!("mp plan gaps {norm} --format json"),
        format!("mp wp add {norm} --name \"...\" --goal \"...\""),
        format!("mp step add {norm} --wp WP1 --action \"...\" --covers-ac AC-01"),
        "mp validate".to_string(),
    ];
    if gaps.ready {
        next_commands = vec![
            format!("mp milestone set-status {norm} in-progress"),
            "mp next --format json".to_string(),
        ];
    }

    Ok(DecomposeReport {
        milestone_id: norm,
        scaffolded,
        gaps,
        next_commands,
    })
}

pub fn execution_ready(m: &MilestoneFile, done_ids: &HashSet<String>) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();
    // M100: lifecycle-based read; rejects anything not at approved or
    // in-progress (terminal states, drafts, groomed).
    let lc = m.effective_lifecycle();
    if !matches!(lc.as_str(), "approved" | "in-progress") {
        reasons.push(format!("lifecycle {lc}"));
    }
    if m.steps.is_empty() {
        reasons.push("no steps".to_string());
    }
    // M100: overlays replace the legacy blocked/deferred/cancelled execution
    // status values.
    if m.milestone.blocked {
        reasons.push("blocked overlay".to_string());
    }
    if m.milestone.deferred {
        reasons.push("deferred overlay".to_string());
    }
    if m.milestone.cancelled {
        reasons.push("cancelled overlay".to_string());
    }
    if !milestone_deps_met(m, done_ids) {
        reasons.push("dependencies not done".to_string());
    }
    (reasons.is_empty(), reasons)
}

fn milestone_deps_met(m: &MilestoneFile, done_ids: &HashSet<String>) -> bool {
    m.milestone.depends_on.iter().all(|dep| {
        dep.is_empty() || dep == "none" || done_ids.contains(&paths::normalize_milestone_id(dep))
    })
}

fn gap(field: &str, severity: &str, message: &str) -> GapItem {
    GapItem {
        field: field.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
    }
}
