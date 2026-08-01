use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate};
use serde::Serialize;

use crate::paths::{self, PlanContext};
use crate::store;
use crate::track_kind;
use crate::validate::effective_execution_status;

#[derive(Debug, Clone)]
pub struct DigestOptions {
    pub since_handoff: bool,
    pub since: Option<String>,
    pub days: Option<u32>,
    pub markdown: bool,
    pub out: Option<PathBuf>,
}

pub fn validate_digest_opts(opts: &DigestOptions) -> Result<()> {
    let flags = [
        opts.since_handoff,
        opts.since.is_some(),
        opts.days.is_some(),
    ]
    .into_iter()
    .filter(|&b| b)
    .count();
    if flags > 1 {
        bail!("use only one of --since-handoff, --since, or --days");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct DigestReport {
    pub since: String,
    pub until: String,
    pub milestones_completed: Vec<DigestMilestone>,
    pub steps_done: Vec<DigestStep>,
    pub tracks_closed: Vec<DigestTrack>,
    pub decisions_added: Vec<DigestDecision>,
    pub blockers_resolved: Vec<DigestBlocker>,
    pub validate_ok: bool,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct DigestMilestone {
    pub id: String,
    pub title: String,
    pub completed: String,
}

#[derive(Debug, Serialize)]
pub struct DigestStep {
    pub milestone: String,
    pub step_id: String,
    pub action: String,
    pub completed: String,
}

#[derive(Debug, Serialize)]
pub struct DigestTrack {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub closed: String,
}

#[derive(Debug, Serialize)]
pub struct DigestDecision {
    pub id: String,
    pub summary: String,
    pub added: String,
}

#[derive(Debug, Serialize)]
pub struct DigestBlocker {
    pub milestone: String,
    pub reason: String,
    pub resolved: String,
}

pub fn resolve_since(ctx: &PlanContext, opts: &DigestOptions) -> Result<String> {
    validate_digest_opts(opts)?;

    if opts.since_handoff {
        let plan = store::load_plan(ctx)?;
        if plan.execution.handoff_at.is_empty() {
            bail!("no handoff recorded; use --since <iso> or --days N");
        }
        return handoff_to_date(&plan.execution.handoff_at);
    }
    if let Some(since) = &opts.since {
        return Ok(since.clone());
    }
    if let Some(days) = opts.days {
        return Ok(format!("{days}d"));
    }
    Ok("7d".to_string())
}

fn handoff_to_date(handoff_at: &str) -> Result<String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(handoff_at) {
        return Ok(dt.date_naive().to_string());
    }
    if let Ok(date) = NaiveDate::parse_from_str(handoff_at, "%Y-%m-%d") {
        return Ok(date.to_string());
    }
    bail!("invalid handoff_at timestamp: {handoff_at}")
}

pub fn format_markdown(report: &DigestReport) -> String {
    let mut md = String::new();
    md.push_str("# Progress Digest\n\n");
    md.push_str(&format!(
        "**Period:** {} → {}\n\n",
        report.since, report.until
    ));
    md.push_str("## Summary\n\n");
    md.push_str(&format!("{}\n\n", report.summary));

    if !report.milestones_completed.is_empty() {
        md.push_str("## Milestones completed\n\n");
        md.push_str("| ID | Title | Completed |\n");
        md.push_str("|----|-------|----------|\n");
        for m in &report.milestones_completed {
            md.push_str(&format!(
                "| M{} | {} | {} |\n",
                m.id,
                escape_md_cell(&m.title),
                m.completed
            ));
        }
        md.push('\n');
    }

    if !report.steps_done.is_empty() {
        md.push_str("## Steps done\n\n");
        md.push_str("| Milestone | Step | Action | Completed |\n");
        md.push_str("|-----------|------|--------|----------|\n");
        for s in &report.steps_done {
            md.push_str(&format!(
                "| M{} | {} | {} | {} |\n",
                s.milestone,
                s.step_id,
                escape_md_cell(&s.action),
                s.completed
            ));
        }
        md.push('\n');
    }

    if !report.tracks_closed.is_empty() {
        md.push_str("## Tracks closed\n\n");
        md.push_str("| Kind | ID | Title | Closed |\n");
        md.push_str("|------|----|-------|--------|\n");
        for t in &report.tracks_closed {
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                t.kind,
                t.id,
                escape_md_cell(&t.title),
                t.closed
            ));
        }
        md.push('\n');
    }

    if !report.decisions_added.is_empty() {
        md.push_str("## Decisions added\n\n");
        for d in &report.decisions_added {
            md.push_str(&format!("- **{}** ({}): {}\n", d.id, d.added, d.summary));
        }
        md.push('\n');
    }

    if !report.blockers_resolved.is_empty() {
        md.push_str("## Blockers resolved\n\n");
        for b in &report.blockers_resolved {
            md.push_str(&format!(
                "- **M{}** ({}): {}\n",
                b.milestone, b.resolved, b.reason
            ));
        }
        md.push('\n');
    }

    md.push_str(&format!(
        "**Validation:** {}\n",
        if report.validate_ok { "OK" } else { "FAIL" }
    ));
    md
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

pub fn build_digest(ctx: &PlanContext, since: &str) -> Result<DigestReport> {
    let until = store::today();
    let since_date = parse_since(since, &until)?;

    let mut milestones_completed = Vec::new();
    let mut steps_done = Vec::new();
    let mut blockers_resolved = Vec::new();

    for (_, m) in store::load_all_milestones(ctx)? {
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty register as done.
        if effective_execution_status(&m) == "done"
            && in_window(&m.verification.date, &since_date, &until)
        {
            milestones_completed.push(DigestMilestone {
                id: paths::normalize_milestone_id(&m.milestone.id),
                title: m.milestone.title.clone(),
                completed: m.verification.date.clone(),
            });
        }
        for step in &m.steps {
            if step.status == "done" && in_window(&m.milestone.updated, &since_date, &until) {
                steps_done.push(DigestStep {
                    milestone: paths::normalize_milestone_id(&m.milestone.id),
                    step_id: step.id.clone(),
                    action: step.action.clone(),
                    completed: m.milestone.updated.clone(),
                });
            }
        }
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty correctly
        // register transitions out of blocked.
        if effective_execution_status(&m) != "blocked"
            && !m.milestone.blocked_at.is_empty()
            && in_window(&m.milestone.updated, &since_date, &until)
        {
            blockers_resolved.push(DigestBlocker {
                milestone: paths::normalize_milestone_id(&m.milestone.id),
                reason: m.milestone.block_reason.clone(),
                resolved: m.milestone.updated.clone(),
            });
        }
    }

    let mut tracks_closed = Vec::new();
    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(track) = store::load_track(ctx, kind) {
            for item in track.items {
                if item.status == "done" && in_window(&item.completed, &since_date, &until) {
                    tracks_closed.push(DigestTrack {
                        kind: kind.to_string(),
                        id: item.id,
                        title: item.title,
                        closed: item.completed.clone(),
                    });
                }
            }
        }
    }

    let mut decisions_added = Vec::new();
    if let Ok(decisions) = store::load_decisions(ctx) {
        for d in decisions.decisions {
            if in_window(&d.date, &since_date, &until) {
                decisions_added.push(DigestDecision {
                    id: d.id,
                    summary: d.summary,
                    added: d.date,
                });
            }
        }
    }

    let validate_ok = crate::validate::validate_plan(ctx)?.ok;
    let summary = format!(
        "Since {since}: {} milestone(s) completed, {} step(s) done, {} track item(s) closed, {} decision(s), validate {}",
        milestones_completed.len(),
        steps_done.len(),
        tracks_closed.len(),
        decisions_added.len(),
        if validate_ok { "ok" } else { "has errors" }
    );

    Ok(DigestReport {
        since: since_date.to_string(),
        until,
        milestones_completed,
        steps_done,
        tracks_closed,
        decisions_added,
        blockers_resolved,
        validate_ok,
        summary,
    })
}

fn parse_since(since: &str, until: &str) -> Result<NaiveDate> {
    if let Some(days) = since.strip_suffix('d').and_then(|n| n.parse::<i64>().ok()) {
        let until_date =
            NaiveDate::parse_from_str(until, "%Y-%m-%d").context("invalid until date")?;
        return Ok(until_date - Duration::days(days));
    }
    NaiveDate::parse_from_str(since, "%Y-%m-%d")
        .with_context(|| format!("invalid --since {since}; use 7d or YYYY-MM-DD"))
}

fn in_window(date: &str, since: &NaiveDate, until: &str) -> bool {
    if date.is_empty() {
        return false;
    }
    let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return false;
    };
    let Ok(until_d) = NaiveDate::parse_from_str(until, "%Y-%m-%d") else {
        return false;
    };
    d >= *since && d <= until_d
}

pub fn digest_since_help() -> &'static str {
    "7d or YYYY-MM-DD"
}

pub fn validate_since(since: &str) -> Result<()> {
    let today = store::today();
    let _ = parse_since(since, &today)?;
    Ok(())
}
