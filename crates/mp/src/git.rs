use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::paths::{self, PlanContext};
use crate::store;

#[derive(Debug, Serialize)]
pub struct GitStatusReport {
    pub ok: bool,
    pub is_repo: bool,
    pub plan_dir: String,
    pub clean: bool,
    pub changed: Vec<GitChangedFile>,
}

#[derive(Debug, Serialize)]
pub struct GitChangedFile {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct GitSuggestReport {
    pub ok: bool,
    pub message: String,
    pub changed_count: usize,
}

#[derive(Debug, Serialize)]
pub struct GitCommitReport {
    pub ok: bool,
    pub message: String,
    pub committed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pushed: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Serialize)]
pub struct GitPushReport {
    pub ok: bool,
    pub pushed: bool,
    pub remote: String,
}

pub fn git_status(ctx: &PlanContext) -> Result<GitStatusReport> {
    let plan_rel = plan_dir_relative(ctx)?;
    if !git_is_repo(&ctx.project_root) {
        return Ok(GitStatusReport {
            ok: true,
            is_repo: false,
            plan_dir: plan_rel,
            clean: true,
            changed: vec![],
        });
    }
    let changed = git_porcelain(&ctx.project_root, &plan_rel)?;
    Ok(GitStatusReport {
        ok: true,
        is_repo: true,
        plan_dir: plan_rel,
        clean: changed.is_empty(),
        changed,
    })
}

pub fn git_suggest_message(ctx: &PlanContext) -> Result<GitSuggestReport> {
    let status = git_status(ctx)?;
    if !status.is_repo {
        bail!("not a git repository");
    }
    if status.changed.is_empty() {
        return Ok(GitSuggestReport {
            ok: true,
            message: "plan: no changes".to_string(),
            changed_count: 0,
        });
    }

    let message = suggest_from_changes(ctx, &status.changed)?;
    Ok(GitSuggestReport {
        ok: true,
        changed_count: status.changed.len(),
        message,
    })
}

pub fn git_commit(ctx: &PlanContext, message: Option<&str>) -> Result<GitCommitReport> {
    if !git_is_repo(&ctx.project_root) {
        bail!("not a git repository");
    }
    let status = git_status(ctx)?;
    if status.changed.is_empty() {
        return Ok(GitCommitReport {
            ok: true,
            message: String::new(),
            committed: false,
            pushed: false,
        });
    }

    let message = match message {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => git_suggest_message(ctx)?.message,
    };

    let plan_rel = plan_dir_relative(ctx)?;
    let add = Command::new("git")
        .current_dir(&ctx.project_root)
        .args(["add", "--", &plan_rel])
        .status()
        .context("git add")?;
    if !add.success() {
        bail!("git add failed");
    }

    let commit = Command::new("git")
        .current_dir(&ctx.project_root)
        .args(["commit", "-m", &message])
        .output()
        .context("git commit")?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        bail!("git commit failed: {stderr}");
    }

    let mut pushed = false;
    if store::load_config(ctx).git_auto_push() {
        pushed = git_push(ctx).map(|r| r.pushed).unwrap_or(false);
    }

    Ok(GitCommitReport {
        ok: true,
        message,
        committed: true,
        pushed,
    })
}

pub fn git_push(ctx: &PlanContext) -> Result<GitPushReport> {
    if !git_is_repo(&ctx.project_root) {
        bail!("not a git repository");
    }
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(&ctx.project_root)
        .output()
        .context("git upstream")?;
    let remote = if upstream.status.success() {
        String::from_utf8_lossy(&upstream.stdout).trim().to_string()
    } else {
        "origin".to_string()
    };

    let push = Command::new("git")
        .args(["push"])
        .current_dir(&ctx.project_root)
        .output()
        .context("git push")?;
    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr);
        bail!("git push failed: {stderr}");
    }

    Ok(GitPushReport {
        ok: true,
        pushed: true,
        remote,
    })
}

fn suggest_from_changes(ctx: &PlanContext, changed: &[GitChangedFile]) -> Result<String> {
    for file in changed {
        if let Some(id) = milestone_id_from_path(&file.path) {
            if let Ok(path) = crate::milestone::load_milestone_path(ctx, &id) {
                let m = store::load_milestone(&path)?;
                let norm = paths::normalize_milestone_id(&m.milestone.id);
                if m.milestone.execution_status == "done" {
                    return Ok(format!("plan({norm}): mark {} complete", m.milestone.title));
                }
                return Ok(format!("plan({norm}): update {}", m.milestone.title));
            }
        }
    }
    Ok("plan: update planning artifacts".to_string())
}

fn milestone_id_from_path(path: &str) -> Option<String> {
    let after = path
        .split("/milestones/")
        .nth(1)
        .or_else(|| path.strip_prefix("milestones/"))?;
    let filename = after.rsplit('/').next()?;
    filename
        .split('-')
        .next()
        .map(paths::normalize_milestone_id)
}

fn git_is_repo(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_porcelain(root: &Path, plan_rel: &str) -> Result<Vec<GitChangedFile>> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", plan_rel])
        .current_dir(root)
        .output()
        .context("git status")?;
    if !output.status.success() {
        bail!("git status failed");
    }
    let mut changed = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].trim().to_string();
        let path = line[3..].trim().to_string();
        if !path.is_empty() {
            changed.push(GitChangedFile { status, path });
        }
    }
    Ok(changed)
}

fn plan_dir_relative(ctx: &PlanContext) -> Result<String> {
    let rel = ctx
        .plan_dir
        .strip_prefix(&ctx.project_root)
        .unwrap_or(&ctx.plan_dir)
        .to_string_lossy()
        .to_string();
    Ok(rel.trim_start_matches('/').to_string())
}
