use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::model::{BriefFile, BriefTopic};
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub struct BriefTodoReport {
    pub ok: bool,
    pub pending_count: usize,
    pub topics: Vec<BriefTopicSummary>,
}

#[derive(Debug, Serialize)]
pub struct BriefTopicSummary {
    pub id: String,
    pub key: String,
    pub title: String,
    pub prompt: String,
    pub required: bool,
}

#[derive(Debug, Serialize)]
pub struct BriefDoneReport {
    pub ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<BriefGateError>,
}

#[derive(Debug, Serialize)]
pub struct BriefGateError {
    pub code: String,
    pub topic: String,
    pub message: String,
}

pub fn brief_todo(ctx: &PlanContext) -> Result<BriefTodoReport> {
    let brief = store::load_brief(ctx)?;
    let topics: Vec<_> = brief
        .topics
        .iter()
        .filter(|t| t.status == "pending")
        .map(topic_summary)
        .collect();
    Ok(BriefTodoReport {
        ok: true,
        pending_count: topics.len(),
        topics,
    })
}

pub fn brief_list(ctx: &PlanContext) -> Result<Vec<BriefTopicSummary>> {
    let brief = store::load_brief(ctx)?;
    Ok(brief
        .topics
        .iter()
        .filter(|t| t.status == "filled")
        .map(topic_summary)
        .collect())
}

pub fn brief_show(ctx: &PlanContext, id: Option<&str>) -> Result<serde_json::Value> {
    let brief = store::load_brief(ctx)?;
    if let Some(id) = id {
        let topic = find_topic(&brief, id)?;
        return Ok(serde_json::json!({ "topic": topic }));
    }
    Ok(serde_json::json!({ "brief": brief }))
}

pub fn brief_edit(
    ctx: &PlanContext,
    id: &str,
    body: Option<&str>,
    status: Option<&str>,
) -> Result<BriefTopic> {
    let mut brief = store::load_brief(ctx)?;
    let topic = find_topic_mut(&mut brief, id)?;
    if let Some(body) = body {
        topic.body = body.to_string();
        if !body.is_empty() && status.is_none() {
            topic.status = "filled".to_string();
        }
    }
    if let Some(status) = status {
        topic.status = status.to_string();
    }
    let out = topic.clone();
    store::write_brief(ctx, &brief)?;
    Ok(out)
}

pub fn brief_add(
    ctx: &PlanContext,
    title: &str,
    prompt: Option<&str>,
    required: bool,
) -> Result<BriefTopic> {
    let mut brief = store::load_brief(ctx)?;
    let id = store::next_brief_topic_id(&brief);
    let key = store::slugify(title);
    let order = brief.topics.iter().map(|t| t.order).max().unwrap_or(0) + 1;
    let topic = BriefTopic {
        id: id.clone(),
        key,
        title: title.to_string(),
        prompt: prompt.unwrap_or("").to_string(),
        body: String::new(),
        status: "pending".to_string(),
        builtin: false,
        required,
        order,
    };
    brief.topics.push(topic.clone());
    store::write_brief(ctx, &brief)?;
    Ok(topic)
}

pub fn brief_rm(ctx: &PlanContext, id: &str) -> Result<()> {
    let mut brief = store::load_brief(ctx)?;
    let idx = brief
        .topics
        .iter()
        .position(|t| t.id == id || t.key == id)
        .with_context(|| format!("topic {id} not found"))?;
    if brief.topics[idx].builtin {
        bail!("cannot remove built-in topic {id}; use skip or na");
    }
    brief.topics.remove(idx);
    store::write_brief(ctx, &brief)
}

pub fn brief_skip(ctx: &PlanContext, id: &str) -> Result<BriefTopic> {
    brief_edit(ctx, id, None, Some("skipped"))
}

pub fn brief_done(ctx: &PlanContext) -> Result<BriefDoneReport> {
    let brief = store::load_brief(ctx)?;
    let mut errors = Vec::new();
    for topic in &brief.topics {
        if topic.required
            && matches!(topic.status.as_str(), "pending" | "")
            && topic.body.is_empty()
        {
            errors.push(BriefGateError {
                code: "B1".to_string(),
                topic: topic.id.clone(),
                message: "required topic still pending".to_string(),
            });
        }
    }
    if !errors.is_empty() {
        return Ok(BriefDoneReport { ok: false, errors });
    }

    let mut brief = brief;
    brief.brief.status = "done".to_string();
    brief.brief.completed = store::today();
    store::write_brief(ctx, &brief)?;

    let mut plan = store::load_plan(ctx)?;
    plan.project.planning_phase = "charter".to_string();
    store::write_plan(ctx, &plan)?;

    Ok(BriefDoneReport {
        ok: true,
        errors: vec![],
    })
}

#[derive(Debug, Serialize)]
pub struct BriefReopenReport {
    pub ok: bool,
    pub status: String,
    pub planning_phase: String,
}

pub fn brief_reopen(ctx: &PlanContext) -> Result<BriefReopenReport> {
    let mut brief = store::load_brief(ctx)?;
    if brief.brief.status == "in_progress" {
        let plan = store::load_plan(ctx)?;
        if plan.project.planning_phase == "brief" {
            return Ok(BriefReopenReport {
                ok: true,
                status: "in_progress".to_string(),
                planning_phase: "brief".to_string(),
            });
        }
        bail!("brief already in progress but plan phase is inconsistent");
    }
    brief.brief.status = "in_progress".to_string();
    brief.brief.completed.clear();
    store::write_brief(ctx, &brief)?;

    let mut plan = store::load_plan(ctx)?;
    plan.project.planning_phase = "brief".to_string();
    store::write_plan(ctx, &plan)?;

    Ok(BriefReopenReport {
        ok: true,
        status: "in_progress".to_string(),
        planning_phase: "brief".to_string(),
    })
}

#[derive(Debug, Clone, Copy)]
pub enum BriefPromoteTarget {
    Idea,
    Backlog,
}

pub fn brief_promote(
    ctx: &PlanContext,
    id: &str,
    target: BriefPromoteTarget,
) -> Result<serde_json::Value> {
    use crate::backlog;
    use crate::idea;

    let topic = find_topic(&store::load_brief(ctx)?, id)?.clone();
    if topic.body.is_empty() {
        bail!("topic {id} has no body; fill with `mp brief edit` first");
    }
    if topic.status == "promoted" {
        let source = format!("brief:{}", topic.id);
        if let Some(entry) = store::load_ideas(ctx)?
            .ideas
            .into_iter()
            .find(|idea| idea.source == source)
        {
            return Ok(serde_json::json!({
                "ok": true,
                "topic_id": id,
                "promoted_to": format!("idea:{}", entry.id),
                "idempotent": true,
            }));
        }
        if let Some(entry) = store::load_backlog(ctx)?
            .items
            .into_iter()
            .find(|item| item.source == source)
        {
            return Ok(serde_json::json!({
                "ok": true,
                "topic_id": id,
                "promoted_to": format!("backlog:{}", entry.id),
                "idempotent": true,
            }));
        }
        bail!("topic {id} is promoted but its target is missing");
    }

    let promoted_to = match target {
        BriefPromoteTarget::Idea => {
            let entry = idea::idea_create(
                ctx,
                &topic.title,
                Some(&topic.body),
                vec![],
                Some(&format!("brief:{}", topic.id)),
            )?;
            format!("idea:{}", entry.id)
        }
        BriefPromoteTarget::Backlog => {
            let item = backlog::backlog_add(
                ctx,
                &topic.title,
                Some(&format!("brief:{}", topic.id)),
                None,
                Some("medium"),
            )?;
            format!("backlog:{}", item.id)
        }
    };

    let mut brief = store::load_brief(ctx)?;
    let topic = find_topic_mut(&mut brief, id)?;
    topic.status = "promoted".to_string();
    store::write_brief(ctx, &brief)?;

    Ok(serde_json::json!({
        "ok": true,
        "topic_id": id,
        "promoted_to": promoted_to,
    }))
}

#[derive(Debug, Serialize)]
pub struct BriefImportReport {
    pub ok: bool,
    pub topics_added: usize,
    pub topics_filled: usize,
    pub total_topics: usize,
}

pub fn brief_import(ctx: &PlanContext, path: &str) -> Result<BriefImportReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read handoff file: {path}"))?;

    let sections = parse_handoff_markdown(&content)?;

    if sections.is_empty() {
        bail!("no markdown headings found in {path}; handoff files need # or ## sections");
    }

    let mut brief = store::load_brief(ctx)?;
    let mut topics_added = 0;
    let mut topics_filled = 0;

    for (heading, body) in &sections {
        let heading_lower = heading.to_lowercase();

        match try_match_topic(&mut brief, &heading_lower, body) {
            MatchResult::Filled => topics_filled += 1,
            MatchResult::AddedFromSection => {
                let id = store::next_brief_topic_id(&brief);
                let key = store::slugify(heading);
                let order = brief.topics.iter().map(|t| t.order).max().unwrap_or(0) + 1;
                let topic = BriefTopic {
                    id: id.clone(),
                    key,
                    title: heading.clone(),
                    prompt: String::new(),
                    body: body.clone(),
                    status: "filled".to_string(),
                    builtin: false,
                    required: false,
                    order,
                };
                brief.topics.push(topic);
                topics_added += 1;
            }
        }
    }

    store::write_brief(ctx, &brief)?;

    Ok(BriefImportReport {
        ok: true,
        topics_added,
        topics_filled,
        total_topics: brief.topics.len(),
    })
}

enum MatchResult {
    Filled,
    AddedFromSection,
}

fn try_match_topic(brief: &mut BriefFile, heading_lower: &str, body: &str) -> MatchResult {
    for topic in &mut brief.topics {
        let title_lower = topic.title.to_lowercase();
        if topic.status == "pending" && topic.body.is_empty() {
            let heading_words: Vec<&str> = heading_lower.split_whitespace().collect();
            let title_words: Vec<&str> = title_lower.split_whitespace().collect();
            let matches: usize = heading_words
                .iter()
                .filter(|w| w.len() > 3 && title_words.contains(w))
                .count();
            if matches > 0
                || heading_lower.contains(&title_lower)
                || title_lower.contains(heading_lower)
            {
                topic.body = body.to_string();
                if !body.is_empty() {
                    topic.status = "filled".to_string();
                }
                return MatchResult::Filled;
            }
        }
    }
    MatchResult::AddedFromSection
}

pub(crate) fn parse_handoff_markdown(content: &str) -> Result<Vec<(String, String)>> {
    let mut sections = Vec::new();
    let mut current_heading = String::new();
    let mut current_body = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
            if !current_heading.is_empty() {
                sections.push((
                    current_heading.clone(),
                    current_body.join("\n").trim().to_string(),
                ));
            }
            current_heading = trimmed.trim_start_matches('#').trim().to_string();
            current_body.clear();
        } else {
            current_body.push(line);
        }
    }
    if !current_heading.is_empty() {
        sections.push((
            current_heading.clone(),
            current_body.join("\n").trim().to_string(),
        ));
    }

    Ok(sections)
}

fn topic_summary(t: &BriefTopic) -> BriefTopicSummary {
    BriefTopicSummary {
        id: t.id.clone(),
        key: t.key.clone(),
        title: t.title.clone(),
        prompt: t.prompt.clone(),
        required: t.required,
    }
}

fn find_topic<'a>(brief: &'a BriefFile, id: &str) -> Result<&'a BriefTopic> {
    brief
        .topics
        .iter()
        .find(|t| t.id == id || t.key == id)
        .with_context(|| format!("topic {id} not found"))
}

fn find_topic_mut<'a>(brief: &'a mut BriefFile, id: &str) -> Result<&'a mut BriefTopic> {
    brief
        .topics
        .iter_mut()
        .find(|t| t.id == id || t.key == id)
        .with_context(|| format!("topic {id} not found"))
}
