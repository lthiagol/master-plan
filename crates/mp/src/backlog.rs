use anyhow::{bail, Context, Result};

use crate::model::BacklogItem;
use crate::paths::PlanContext;
use crate::store;
use crate::track_kind;

pub fn backlog_add(
    ctx: &PlanContext,
    description: &str,
    source: Option<&str>,
    suggested_when: Option<&str>,
    priority: Option<&str>,
) -> Result<BacklogItem> {
    if description.is_empty() {
        bail!("--desc is required");
    }
    let mut backlog = store::load_backlog(ctx)?;
    let id = store::next_backlog_id(&backlog);
    let item = BacklogItem {
        id: id.clone(),
        description: description.to_string(),
        source: source.unwrap_or("planning").to_string(),
        suggested_when: suggested_when.unwrap_or("").to_string(),
        priority: priority.unwrap_or("medium").to_string(),
        status: "active".to_string(),
        resolution: String::new(),
        resolved_at: String::new(),
        created: store::today(),
    };
    backlog.items.push(item.clone());
    store::write_backlog(ctx, &backlog)?;
    Ok(item)
}

pub fn backlog_show(ctx: &PlanContext, id: &str) -> Result<BacklogItem> {
    let backlog = store::load_backlog(ctx)?;
    backlog
        .items
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .with_context(|| format!("backlog item {id} not found"))
}

pub fn backlog_list(ctx: &PlanContext, status: Option<&str>) -> Result<Vec<BacklogItem>> {
    let backlog = store::load_backlog(ctx)?;
    Ok(backlog
        .items
        .into_iter()
        .filter(|i| status.is_none_or(|s| i.status == s))
        .collect())
}

/// M112 S1: read-only filtered list. Filters are AND-combined; an absent
/// filter does not match against that dimension. `--limit N` slices the
/// first N items after filtering (ascending file order). Source/status/
/// priority filters are case-sensitive exact-match — backlog items store
/// those fields as authored.
pub fn backlog_list_filtered(
    ctx: &PlanContext,
    source: Option<&str>,
    status: Option<&str>,
    priority: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<BacklogItem>> {
    let backlog = store::load_backlog(ctx)?;
    let mut items: Vec<BacklogItem> = backlog
        .items
        .into_iter()
        .filter(|i| source.is_none_or(|s| i.source == s))
        .filter(|i| status.is_none_or(|s| i.status == s))
        .filter(|i| priority.is_none_or(|p| i.priority == p))
        .collect();
    if let Some(n) = limit {
        items.truncate(n);
    }
    Ok(items)
}

pub fn backlog_resolve(
    ctx: &PlanContext,
    id: &str,
    into_milestone: Option<&str>,
    wont_fix: bool,
    reason: Option<&str>,
) -> Result<BacklogItem> {
    let mut backlog = store::load_backlog(ctx)?;
    let item = backlog
        .items
        .iter_mut()
        .find(|i| i.id == id)
        .with_context(|| format!("backlog item {id} not found"))?;
    if item.status == "resolved" {
        bail!("backlog item {id} already resolved");
    }
    item.status = "resolved".to_string();
    item.resolved_at = store::today();
    if wont_fix {
        item.resolution = format!("wont-fix: {}", reason.unwrap_or(""));
    } else if let Some(mid) = into_milestone {
        item.resolution = format!("milestone:{mid}");
    } else if let Some(r) = reason {
        item.resolution = r.to_string();
    } else {
        item.resolution = "resolved".to_string();
    }
    let out = item.clone();
    store::write_backlog(ctx, &backlog)?;
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
pub enum BacklogPromoteTarget<'a> {
    Milestone,
    Track(&'a str),
}

pub fn backlog_promote(
    ctx: &PlanContext,
    id: &str,
    target: BacklogPromoteTarget<'_>,
) -> Result<serde_json::Value> {
    use crate::milestone::{self, CreateMilestoneInput};
    use crate::model::{Intent, Problem, Scope, TrackItem};

    let item = backlog_show(ctx, id)?;
    if item.status == "resolved" {
        if item.resolution.starts_with("milestone:") || item.resolution.starts_with("track:") {
            return Ok(serde_json::json!({
                "ok": true,
                "backlog_id": id,
                "promoted_to": item.resolution,
                "idempotent": true,
            }));
        }
        anyhow::bail!("backlog item {id} already resolved ({})", item.resolution);
    }

    let promoted_to = match target {
        BacklogPromoteTarget::Milestone => {
            let problem = backlog_problem_text(&item);
            let m = milestone::create_milestone(
                ctx,
                CreateMilestoneInput {
                    title: Some(backlog_title(&item.description)),
                    intent: Intent {
                        outcome: item.description.clone(),
                    },
                    problem: Problem {
                        description: problem,
                    },
                    scope: Scope {
                        in_scope: vec![format!("From backlog {id}")],
                        out_of_scope: vec!["Out of backlog scope".to_string(), "TBD".to_string()],
                    },
                    effort: backlog_effort(&item.priority),
                    ..Default::default()
                },
            )?;
            format!("milestone:{}", m.milestone.id)
        }
        BacklogPromoteTarget::Track(kind) => {
            if kind.parse::<track_kind::TrackKind>().is_err() {
                anyhow::bail!("track kind must be bugfix or tweak");
            }
            let path = ctx.track_path(kind);
            let mut track = store::load_track(ctx, kind)?;
            let item_id = store::next_track_item_id(&track, kind)?;
            let track_item = TrackItem {
                id: item_id.clone(),
                title: backlog_title(&item.description),
                status: "pending".to_string(),
                effort: backlog_effort(&item.priority),
                problem: backlog_problem_text(&item),
                done_when: String::new(),
                verification: String::new(),
                steps: vec![],
                evidence: String::new(),
                created: store::today(),
                completed: String::new(),
                archived_at: String::new(),
            };
            track.items.push(track_item);
            store::write_track(ctx, &path, &track)?;
            format!("track:{kind}:{item_id}")
        }
    };

    let mut backlog = store::load_backlog(ctx)?;
    let entry = backlog
        .items
        .iter_mut()
        .find(|i| i.id == id)
        .with_context(|| format!("backlog item {id} not found"))?;
    entry.status = "resolved".to_string();
    entry.resolution = promoted_to.clone();
    entry.resolved_at = store::today();
    store::write_backlog(ctx, &backlog)?;

    Ok(serde_json::json!({
        "ok": true,
        "backlog_id": id,
        "promoted_to": promoted_to,
    }))
}

fn backlog_title(description: &str) -> String {
    let line = description.lines().next().unwrap_or(description).trim();
    if line.chars().count() > 80 {
        format!("{}...", line.chars().take(77).collect::<String>())
    } else {
        line.to_string()
    }
}

fn backlog_problem_text(item: &BacklogItem) -> String {
    let mut out = item.description.clone();
    if !item.source.is_empty() {
        out.push_str(&format!("\n\nSource: {}", item.source));
    }
    if !item.suggested_when.is_empty() {
        out.push_str(&format!("\nSuggested when: {}", item.suggested_when));
    }
    out
}

fn backlog_effort(priority: &str) -> String {
    match priority {
        "high" => "M".to_string(),
        "low" => "XS".to_string(),
        _ => "S".to_string(),
    }
}
