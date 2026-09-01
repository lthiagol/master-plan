use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::model::{MilestoneFile, MilestoneMeta, SessionFile, SessionMeta};
use crate::paths::{self, PlanContext};
use crate::store;

fn session_is_active(status: &str) -> bool {
    matches!(status, "draft" | "ready" | "in-progress" | "")
}

pub fn find_session_by_branch(ctx: &PlanContext, branch: &str) -> Result<Option<SessionSummary>> {
    for summary in session_list(ctx)? {
        if summary.branch == branch && session_is_active(&summary.status) {
            return Ok(Some(summary));
        }
    }
    Ok(None)
}

#[derive(Debug, Serialize)]
pub struct SessionShowReport {
    pub ok: bool,
    pub session: SessionSummary,
    pub milestone: MilestoneSummary,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub branch: String,
    pub title: String,
    pub status: String,
    pub milestone_id: String,
    #[serde(default)]
    pub focused: bool,
}

#[derive(Debug, Serialize)]
pub struct MilestoneSummary {
    pub id: String,
    pub title: String,
    pub spec_status: String,
    pub execution_status: String,
}

pub fn session_start(
    ctx: &PlanContext,
    branch: Option<&str>,
    title: Option<&str>,
) -> Result<serde_json::Value> {
    session_start_impl(ctx, branch, title, None)
}

pub(crate) fn session_start_in_txn(
    ctx: &PlanContext,
    branch: Option<&str>,
    title: Option<&str>,
    txn: &crate::plan_io::PlanWriteTxn,
) -> Result<serde_json::Value> {
    session_start_impl(ctx, branch, title, Some(txn))
}

fn session_start_impl(
    ctx: &PlanContext,
    branch: Option<&str>,
    title: Option<&str>,
    txn: Option<&crate::plan_io::PlanWriteTxn>,
) -> Result<serde_json::Value> {
    let cfg = store::load_config(ctx);
    let branch = branch
        .map(str::to_string)
        .or_else(current_git_branch)
        .unwrap_or_else(|| "feature/session".to_string());

    if cfg.auto_bind_branch() {
        if let Some(existing) = find_session_by_branch(ctx, &branch)? {
            let report = session_show(ctx, Some(&existing.id))?;
            return Ok(json!({
                "ok": true,
                "resumed": true,
                "session_id": report.session.id,
                "branch": report.session.branch,
                "milestone_id": report.session.milestone_id,
                "plan_dir": ctx.plan_dir,
            }));
        }
    }

    let session_id = slug_from_branch(&branch);
    let title = title.unwrap_or(&session_id.replace('-', " ")).to_string();
    let milestone_id = store::next_milestone_id(ctx)?;

    let session_dir = store::session_dir(ctx, &session_id)?;
    if session_dir.exists() {
        bail!("session {session_id} already exists");
    }
    std::fs::create_dir_all(&session_dir)?;

    let session = SessionFile {
        session: SessionMeta {
            id: session_id.clone(),
            branch: branch.clone(),
            title: title.clone(),
            status: "draft".to_string(),
            milestone_id: milestone_id.clone(),
            milestone_file: "milestone.json".to_string(),
            started: store::today(),
            updated: store::today(),
            merged: String::new(),
            archived_at: String::new(),
        },
    };
    store::write_session(ctx, &session_id, &session)?;

    let slug = store::slugify(&title);
    let milestone = MilestoneFile {
        milestone: MilestoneMeta {
            id: milestone_id.clone(),
            title,
            slug,
            lifecycle: "draft".to_string(),
            // M144: track lifecycle transition timestamp on session-created
            // milestones too.
            lifecycle_at: Some(store::now_rfc3339()),
            spec_status: String::new(),
            execution_status: String::new(),
            blocked: false,
            needs_regrooming: false,
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            deferred: false,
            deferred_reason: String::new(),
            depends_on: vec![],
            effort: "S".to_string(),
            risk: "low".to_string(),
            change_kind: "greenfield".to_string(),
            priority: "normal".to_string(),
            created: store::today(),
            updated: store::today(),
            blocked_at: String::new(),
            block_reason: String::new(),
            blocked_by: String::new(),
            target_version: String::new(),
            executed_by: String::new(),
            remediation_pre_state: None,
            flow_stages: BTreeMap::new(),
        },
        ..Default::default()
    };
    let m_path = session_dir.join("milestone.json");
    store::write_milestone(&m_path, &milestone)?;

    // M180 S3: session-start creates a milestone in `draft`; record
    // one milestone-created event so the activity feed sees the new
    // milestone even when it lives under a session directory.
    let event = crate::activity::milestone_created_event(&milestone_id);
    if let Some(txn) = txn {
        txn.append_activity_best_effort(ctx, event)?;
    } else {
        crate::activity::append_event_best_effort(ctx, event)?;
    }

    Ok(json!({
        "ok": true,
        "resumed": false,
        "session_id": session_id,
        "branch": branch,
        "milestone_id": milestone_id,
        "plan_dir": ctx.plan_dir,
    }))
}

pub fn session_show(ctx: &PlanContext, session_id: Option<&str>) -> Result<SessionShowReport> {
    let cfg = store::load_config(ctx);
    let id = if let Some(id) = session_id {
        id.to_string()
    } else if let Some(focus) = cfg.focus_session() {
        focus.to_string()
    } else if cfg.auto_bind_branch() {
        if let Some(branch) = current_git_branch() {
            if let Some(existing) = find_session_by_branch(ctx, &branch)? {
                existing.id
            } else {
                find_active_session_id(ctx).context("provide session id")?
            }
        } else {
            find_active_session_id(ctx).context("provide session id")?
        }
    } else {
        find_active_session_id(ctx).context("provide session id")?
    };
    let session = store::load_session(ctx, &id)?;
    let sid = session.session.id.clone();
    let bid = session.session.branch.clone();
    let m_path = store::session_dir(ctx, &id)?.join("milestone.json");
    let m = store::load_milestone(&m_path)?;
    Ok(SessionShowReport {
        ok: true,
        session: SessionSummary {
            id: sid.clone(),
            branch: bid,
            title: session.session.title,
            status: session.session.status,
            milestone_id: session.session.milestone_id,
            focused: cfg.focus_session() == Some(&sid),
        },
        milestone: MilestoneSummary {
            id: m.milestone.id,
            title: m.milestone.title,
            // M100 ER-8 follow-up: intentionally raw read of the
            // legacy fields. Session replay preserves the
            // byte-state of the milestone as it was at capture
            // time; routing through `effective_spec_status` /
            // `effective_execution_status` would lose the empty
            // (post-migration) shape and break the session's
            // round-trip invariant. ER-8 routed decision-making
            // sites only; this site preserves replay fidelity.
            spec_status: m.milestone.spec_status,
            execution_status: m.milestone.execution_status,
        },
    })
}

pub fn session_list(ctx: &PlanContext) -> Result<Vec<SessionSummary>> {
    let cfg = store::load_config(ctx);
    let focused = cfg.focus_session();
    let mut out = Vec::new();
    for dir in store::list_session_dirs(ctx)? {
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        if let Ok(session) = store::load_session(ctx, &id) {
            let is_focused = focused == Some(&session.session.id);
            out.push(SessionSummary {
                id: session.session.id,
                branch: session.session.branch,
                title: session.session.title,
                status: session.session.status,
                milestone_id: session.session.milestone_id,
                focused: is_focused,
            });
        }
    }
    Ok(out)
}

pub fn session_archive(ctx: &PlanContext, session_id: &str, force: bool) -> Result<()> {
    let archived = ctx.archive_dir().join("sessions").join(session_id);
    if archived.is_dir() {
        return Ok(());
    }
    let session = store::load_session(ctx, session_id)?;
    let m_path = store::session_dir(ctx, session_id)?.join("milestone.json");
    let m = store::load_milestone(&m_path)?;
    if !force && m.steps.iter().any(|s| s.status == "in-progress") {
        bail!("session has in-progress steps; use --force");
    }

    let mut session = session;
    session.session.status = "archived".to_string();
    session.session.archived_at = store::now_rfc3339();
    store::write_session(ctx, session_id, &session)?;

    let src = store::session_dir(ctx, session_id)?;
    let dest = ctx.archive_dir().join("sessions").join(session_id);
    if dest.exists() {
        bail!("session already archived");
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    store::rename_plan_path(&src, &dest)?;
    Ok(())
}

#[derive(Debug, serde::Serialize)]
pub struct SessionExport {
    pub session_id: String,
    pub format: String,
    pub body: String,
}

pub fn session_export(ctx: &PlanContext, session_id: &str) -> Result<SessionExport> {
    let report = session_show(ctx, Some(session_id))?;
    let m_path = store::session_dir(ctx, session_id)?.join("milestone.json");
    let m = store::load_milestone(&m_path)?;

    let mut lines = Vec::new();
    let title = &report.session.title;
    lines.push(format!("# Session: {title}"));
    lines.push(String::new());
    lines.push(format!("- Session: `{}`", report.session.id));
    lines.push(format!("- Branch: `{}`", report.session.branch));
    lines.push(format!(
        "- Milestone: {} ({})",
        paths::display_milestone_id(&report.milestone.id),
        report.milestone.title
    ));
    lines.push(format!(
        "- Status: spec={}, execution={}",
        report.milestone.spec_status, report.milestone.execution_status
    ));
    if !m.intent.outcome.is_empty() {
        lines.push(String::new());
        lines.push("## Outcome".to_string());
        lines.push(m.intent.outcome.clone());
    }
    if !m.steps.is_empty() {
        lines.push(String::new());
        lines.push("## Steps".to_string());
        for step in &m.steps {
            lines.push(format!(
                "- **{}** [{}] {}",
                step.id, step.status, step.action
            ));
        }
    }

    Ok(SessionExport {
        session_id: session_id.to_string(),
        format: "json".to_string(),
        body: lines.join("\n"),
    })
}

pub fn session_promote(
    ctx: &PlanContext,
    session_id: &str,
    milestone_id: Option<&str>,
) -> Result<serde_json::Value> {
    let (session_path, m_path) = resolve_session_milestone_path(ctx, session_id)?;
    let mut m = store::load_milestone(&m_path)?;
    let source_session = store::load_session_from_path(&session_path)?;
    let provenance = format!("Promoted from session {session_id}");
    if source_session.session.status == "promoted" {
        if let Some((path, existing)) = store::load_all_milestones(ctx)?
            .into_iter()
            .find(|(_, milestone)| milestone.scope.in_scope.iter().any(|s| s == &provenance))
        {
            return Ok(serde_json::json!({
                "ok": true,
                "session_id": session_id,
                "milestone_id": existing.milestone.id,
                "milestone_file": path.strip_prefix(&ctx.plan_dir).unwrap_or(&path).to_string_lossy(),
                "idempotent": true,
            }));
        }
        bail!("session {session_id} is promoted but its target is missing");
    }
    let target_id = if let Some(id) = milestone_id {
        paths::normalize_milestone_id(id)
    } else {
        store::next_milestone_id(ctx)?
    };

    if ctx
        .milestones_dir()
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with(&format!("{target_id}-")))
                .unwrap_or(false)
        })
    {
        anyhow::bail!("milestone {target_id} already exists in milestones/");
    }

    m.milestone.id = target_id.clone();
    m.milestone.updated = store::today();
    if !m.scope.in_scope.iter().any(|item| item == &provenance) {
        m.scope.in_scope.push(provenance);
    }
    let dest = ctx
        .milestones_dir()
        .join(format!("{}-{}.json", m.milestone.id, m.milestone.slug));
    store::write_milestone(&dest, &m)?;

    let mut session = store::load_session_from_path(&session_path)?;
    session.session.status = "promoted".to_string();
    session.session.updated = store::today();
    store::write_session_file(&session_path, &session)?;

    Ok(serde_json::json!({
        "ok": true,
        "session_id": session_id,
        "milestone_id": target_id,
        "milestone_file": dest.strip_prefix(&ctx.plan_dir).unwrap_or(&dest).to_string_lossy(),
    }))
}

pub fn session_focus(ctx: &PlanContext, session_id: &str) -> Result<serde_json::Value> {
    store::load_session(ctx, session_id)?;
    let mut cfg = store::load_config(ctx);
    cfg.workflow.session.focus = Some(session_id.to_string());
    store::write_config(ctx, &cfg)?;
    Ok(json!({
        "ok": true,
        "focused": session_id,
    }))
}

pub fn session_unfocus(ctx: &PlanContext) -> Result<serde_json::Value> {
    let mut cfg = store::load_config(ctx);
    cfg.workflow.session.focus = None;
    store::write_config(ctx, &cfg)?;
    Ok(json!({
        "ok": true,
        "focused": serde_json::Value::Null,
    }))
}

fn resolve_session_milestone_path(
    ctx: &PlanContext,
    session_id: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let active = store::session_dir(ctx, session_id)?;
    let active_session = active.join("session.json");
    if active_session.is_file() {
        return Ok((active_session, active.join("milestone.json")));
    }
    paths::assert_safe_path_segment(session_id, "session")?;
    let archived = ctx
        .archive_dir()
        .join("sessions")
        .join(session_id)
        .join("session.json");
    if archived.is_file() {
        let dir = archived.parent().context("session dir")?.to_path_buf();
        return Ok((archived, dir.join("milestone.json")));
    }
    anyhow::bail!("session {session_id} not found")
}

fn find_active_session_id(ctx: &PlanContext) -> Result<String> {
    let dirs = store::list_session_dirs(ctx)?;
    if dirs.len() == 1 {
        return Ok(dirs[0]
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string());
    }
    bail!("multiple sessions; provide session id")
}

fn slug_from_branch(branch: &str) -> String {
    let base = branch.rsplit('/').next().unwrap_or(branch);
    store::slugify(base)
}

fn current_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}
