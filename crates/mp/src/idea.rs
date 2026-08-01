use anyhow::{Context, Result};
use serde::Serialize;

use crate::model::IdeaEntry;
use crate::paths::PlanContext;
use crate::store;
use crate::track_kind;

fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn similar_idea_titles(ideas: &[IdeaEntry], title: &str) -> Vec<String> {
    let needle = normalize_title(title);
    if needle.is_empty() {
        return vec![];
    }
    ideas
        .iter()
        .filter(|i| i.status == "open" || i.status.is_empty())
        .filter(|i| {
            let hay = normalize_title(&i.title);
            !hay.is_empty() && (hay == needle || hay.contains(&needle) || needle.contains(&hay))
        })
        .map(|i| i.title.clone())
        .collect()
}

pub fn idea_create_meeting(
    ctx: &PlanContext,
    title: &str,
    body: Option<&str>,
) -> Result<IdeaEntry> {
    idea_create(
        ctx,
        title,
        body,
        vec!["meeting".to_string()],
        Some("meeting"),
    )
}

pub fn idea_create(
    ctx: &PlanContext,
    title: &str,
    body: Option<&str>,
    tags: Vec<String>,
    source: Option<&str>,
) -> Result<IdeaEntry> {
    let mut ideas = store::load_ideas(ctx)?;
    let similar = similar_idea_titles(&ideas.ideas, title);
    let id = store::next_idea_id(&ideas);
    let idea = IdeaEntry {
        id: id.clone(),
        title: title.to_string(),
        body: body.unwrap_or("").to_string(),
        status: "open".to_string(),
        tags,
        source: source.unwrap_or("conversation").to_string(),
        created: store::today(),
        promoted_to: String::new(),
    };
    ideas.ideas.push(idea.clone());
    store::write_ideas(ctx, &ideas)?;
    if !similar.is_empty() {
        eprintln!(
            "mp: idea dup-check warning: similar open ideas: {}",
            similar.join(", ")
        );
    }
    Ok(idea)
}

pub fn idea_list(ctx: &PlanContext, status: Option<&str>) -> Result<Vec<IdeaEntry>> {
    let ideas = store::load_ideas(ctx)?;
    Ok(ideas
        .ideas
        .into_iter()
        .filter(|i| status.is_none_or(|s| i.status == s))
        .collect())
}

pub fn idea_show(ctx: &PlanContext, id: &str) -> Result<IdeaEntry> {
    let ideas = store::load_ideas(ctx)?;
    ideas
        .ideas
        .into_iter()
        .find(|i| i.id == id)
        .with_context(|| format!("idea {id} not found"))
}

pub fn idea_update(
    ctx: &PlanContext,
    id: &str,
    title: Option<&str>,
    body: Option<&str>,
    status: Option<&str>,
    tags: Option<Vec<String>>,
) -> Result<IdeaEntry> {
    let mut ideas = store::load_ideas(ctx)?;
    let idea = ideas
        .ideas
        .iter_mut()
        .find(|i| i.id == id)
        .with_context(|| format!("idea {id} not found"))?;
    if let Some(title) = title {
        idea.title = title.to_string();
    }
    if let Some(body) = body {
        idea.body = body.to_string();
    }
    if let Some(status) = status {
        idea.status = status.to_string();
    }
    if let Some(tags) = tags {
        idea.tags = tags;
    }
    let out = idea.clone();
    store::write_ideas(ctx, &ideas)?;
    Ok(out)
}

pub fn idea_dismiss(ctx: &PlanContext, id: &str) -> Result<IdeaEntry> {
    idea_update(ctx, id, None, None, Some("dismissed"), None)
}

pub fn idea_archive(ctx: &PlanContext, id: &str) -> Result<IdeaEntry> {
    idea_update(ctx, id, None, None, Some("archived"), None)
}

pub fn idea_remove(ctx: &PlanContext, id: &str) -> Result<()> {
    let mut ideas = store::load_ideas(ctx)?;
    let len_before = ideas.ideas.len();
    ideas.ideas.retain(|i| i.id != id);
    if ideas.ideas.len() == len_before {
        anyhow::bail!("idea {id} not found");
    }
    store::write_ideas(ctx, &ideas)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum IdeaPromoteTarget<'a> {
    Milestone,
    Backlog,
    Track(&'a str),
}

pub fn idea_promote(
    ctx: &PlanContext,
    id: &str,
    target: IdeaPromoteTarget<'_>,
) -> Result<serde_json::Value> {
    use crate::backlog;
    use crate::milestone::{self, CreateMilestoneInput};
    use crate::model::{Intent, Problem, Scope, TrackItem};

    let idea = idea_show(ctx, id)?;
    if idea.status == "promoted" {
        return Ok(serde_json::json!({
            "ok": true,
            "idea_id": id,
            "promoted_to": idea.promoted_to,
            "idempotent": true,
        }));
    }

    let promoted_to = match target {
        IdeaPromoteTarget::Milestone => {
            let m = milestone::create_milestone(
                ctx,
                CreateMilestoneInput {
                    title: Some(idea.title.clone()),
                    intent: Intent {
                        outcome: if idea.body.is_empty() {
                            idea.title.clone()
                        } else {
                            idea.body.clone()
                        },
                    },
                    problem: Problem {
                        description: if idea.body.is_empty() {
                            idea.title.clone()
                        } else {
                            idea.body.clone()
                        },
                    },
                    scope: Scope {
                        in_scope: vec!["From promoted idea".to_string()],
                        out_of_scope: vec!["Out of idea scope".to_string(), "TBD".to_string()],
                    },
                    ..Default::default()
                },
            )?;
            format!("milestone:{}", m.milestone.id)
        }
        IdeaPromoteTarget::Backlog => {
            let item = backlog::backlog_add(
                ctx,
                &idea.title,
                Some(&format!("idea:{id}")),
                None,
                Some("medium"),
            )?;
            format!("backlog:{}", item.id)
        }
        IdeaPromoteTarget::Track(kind) => {
            if kind.parse::<track_kind::TrackKind>().is_err() {
                anyhow::bail!("track kind must be bugfix or tweak");
            }
            let path = ctx.track_path(kind);
            let mut track = store::load_track(ctx, kind)?;
            let item_id = store::next_track_item_id(&track, kind)?;
            let item = TrackItem {
                id: item_id.clone(),
                title: idea.title.clone(),
                status: "pending".to_string(),
                effort: "S".to_string(),
                problem: idea.body.clone(),
                done_when: String::new(),
                verification: String::new(),
                steps: vec![],
                evidence: String::new(),
                created: store::today(),
                completed: String::new(),
                archived_at: String::new(),
            };
            track.items.push(item);
            store::write_track(ctx, &path, &track)?;
            format!("track:{kind}:{item_id}")
        }
    };

    let mut ideas = store::load_ideas(ctx)?;
    let entry = ideas
        .ideas
        .iter_mut()
        .find(|i| i.id == id)
        .context("idea not found")?;
    entry.status = "promoted".to_string();
    entry.promoted_to = promoted_to.clone();
    store::write_ideas(ctx, &ideas)?;

    Ok(serde_json::json!({
        "ok": true,
        "idea_id": id,
        "promoted_to": promoted_to,
    }))
}

#[derive(Debug, Serialize)]
pub struct IdeaListReport {
    pub ideas: Vec<IdeaEntry>,
}
