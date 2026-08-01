use anyhow::Result;
use serde::Serialize;

use crate::ac_verify;
use crate::milestone::load_milestone_path;
use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::plan_gaps;
use crate::store;

#[derive(Debug, Clone, Serialize)]
pub struct TraceReport {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
    pub acceptance_criteria: Vec<AcTrace>,
    pub steps: Vec<StepTrace>,
    pub gaps: Vec<GapRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcTrace {
    pub id: String,
    pub description: String,
    pub verification: String,
    pub verification_kind: String,
    pub ac_status: String,
    pub evidence: String,
    pub covered_by_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepTrace {
    pub id: String,
    pub status: String,
    pub tests: String,
    pub tests_kind: String,
    pub covers_ac: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GapRow {
    pub kind: String,
    pub message: String,
}

pub fn milestone_trace(ctx: &PlanContext, milestone_id: &str) -> Result<TraceReport> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    Ok(trace_milestone(&m))
}

pub fn trace_milestone(m: &MilestoneFile) -> TraceReport {
    let gaps_report = plan_gaps::plan_gaps_for_milestone(m);
    let mut gaps = Vec::new();

    for ac in &gaps_report.coverage.acceptance_criteria {
        if ac.status == "uncovered" {
            gaps.push(GapRow {
                kind: "uncovered_ac".into(),
                message: format!("AC {} has no covering step", ac.id),
            });
        }
    }
    for step_id in &gaps_report.coverage.orphan_steps {
        gaps.push(GapRow {
            kind: "orphan_step".into(),
            message: format!("step {step_id} covers no AC"),
        });
    }

    let acceptance_criteria: Vec<AcTrace> = m
        .acceptance_criteria
        .iter()
        .map(|ac| {
            let kind = ac_verify::classify(&ac.verification);
            let covered_by_steps = m
                .steps
                .iter()
                .filter(|s| s.covers_ac.iter().any(|id| id == &ac.id))
                .map(|s| s.id.clone())
                .collect();
            AcTrace {
                id: ac.id.clone(),
                description: ac.description.clone(),
                verification: ac.verification.clone(),
                verification_kind: kind_label(kind),
                ac_status: ac.status.clone(),
                evidence: ac.evidence.clone(),
                covered_by_steps,
            }
        })
        .collect();

    let steps: Vec<StepTrace> = m
        .steps
        .iter()
        .map(|s| {
            let kind = ac_verify::classify(&s.tests);
            StepTrace {
                id: s.id.clone(),
                status: s.status.clone(),
                tests: s.tests.clone(),
                tests_kind: kind_label(kind),
                covers_ac: s.covers_ac.clone(),
            }
        })
        .collect();

    for ac in &acceptance_criteria {
        if ac.verification_kind == "manual" {
            let runnable_cover = m.steps.iter().any(|s| {
                s.covers_ac.iter().any(|id| id == &ac.id)
                    && ac_verify::classify(&s.tests) == ac_verify::Kind::Runnable
            });
            if runnable_cover {
                gaps.push(GapRow {
                    kind: "manual_ac_runnable_step".into(),
                    message: format!(
                        "AC {} is manual but covered by step with runnable tests",
                        ac.id
                    ),
                });
            }
        }
    }
    for step in &steps {
        if step.tests_kind == "empty" {
            gaps.push(GapRow {
                kind: "missing_step_tests".into(),
                message: format!("step {} has empty tests", step.id),
            });
        }
    }

    TraceReport {
        milestone_id: m.milestone.id.clone(),
        display: paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        acceptance_criteria,
        steps,
        gaps,
    }
}

fn kind_label(kind: ac_verify::Kind) -> String {
    match kind {
        ac_verify::Kind::Runnable => "runnable".into(),
        ac_verify::Kind::Manual => "manual".into(),
        ac_verify::Kind::Empty => "empty".into(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneLogReport {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
    pub slug: String,
    pub created: String,
    pub updated: String,
    pub file_path: String,
    pub commits: Vec<GitLogLine>,
    /// Captured `stderr` from `git log` when it failed (e.g. fixture copied
    /// outside a git working tree). When set, `commits` may be `[]` and
    /// callers must check `git_error` to avoid claiming "no history".
    pub git_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitLogLine {
    /// Raw line from `git log --oneline`. Kept as a single string so the
    /// caller can split commit/short-sha on whitespace if they need to.
    pub line: String,
}

/// M112 S4: read-only milestone history log. Emits `created` / `updated`
/// from the on-disk milestone plus the milestone file's git history
/// (`git log --oneline -- <relative-path>`). Resolves the path via the
/// existing `load_milestone_path` so the git-relative path matches what
/// the user sees in `find . -name '03-...'`. Pure read; no on-disk writes,
/// no auto-sync, no MP_HOME side effects.
pub fn milestone_log(ctx: &PlanContext, milestone_id: &str) -> Result<MilestoneLogReport> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;

    // Compute a path relative to the project root so the git invocation
    // is stable across CWDs and project_root overrides. `ctx.project_root`
    // is the canonical project root per `PlanContext`.
    let project_root = &ctx.project_root;
    let relative_path = match path.strip_prefix(project_root) {
        Ok(p) => p.to_path_buf(),
        Err(_) => path.clone(),
    };

    let (commits, git_error) = git_log_oneline(&relative_path, project_root);

    Ok(MilestoneLogReport {
        milestone_id: m.milestone.id.clone(),
        display: paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        slug: m.milestone.slug.clone(),
        created: m.milestone.created.clone(),
        updated: m.milestone.updated.clone(),
        file_path: relative_path.to_string_lossy().to_string(),
        commits,
        git_error,
    })
}

fn git_log_oneline(
    relative_path: &std::path::Path,
    cwd: &std::path::Path,
) -> (Vec<GitLogLine>, Option<String>) {
    use std::process::Command;
    let path_arg = relative_path.to_string_lossy().to_string();

    // Try the plain invocation first. If git exits non-zero (e.g. file is
    // outside the working tree, fixture has no .git), capture the stderr
    // and surface it so the caller can distinguish "no history" from "git
    // unavailable".
    let output = Command::new("git")
        .arg("log")
        .arg("--oneline")
        .arg("--")
        .arg(&path_arg)
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return (Vec::new(), Some(stderr));
            }
            let text = String::from_utf8_lossy(&out.stdout);
            let commits = text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| GitLogLine {
                    line: l.to_string(),
                })
                .collect();
            (commits, None)
        }
        Err(err) => (Vec::new(), Some(format!("git could not be spawned: {err}"))),
    }
}
