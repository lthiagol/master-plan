//! `mp overview` bounded path / inbox projections (AC-08).
//!
//! `path` is the suggested-path preview at 3..5 items (path_engine
//! already returns up to 5; we cap to 5 here for clarity). `inbox` is
//! the actionable inbox preview capped at 5.

use anyhow::Result;
use serde::Serialize;

use crate::inbox;
use crate::paths::PlanContext;
use crate::reviews;
use crate::store;

#[derive(Debug, Clone, Serialize)]
pub struct PathItem {
    pub id: String,
    pub display: String,
    pub kind: String,
    pub milestone: Option<String>,
    pub step: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxItem {
    pub id: String,
    pub display: String,
    pub kind: String,
    pub reason: String,
    pub action: String,
}

pub fn build_path_and_inbox(ctx: &PlanContext) -> Result<(Vec<PathItem>, Vec<InboxItem>)> {
    let path_items = build_path_items(ctx)?;
    let inbox_items = build_inbox_items(ctx)?;
    Ok((path_items, inbox_items))
}

const MAX_PREVIEW: usize = 5;

fn build_path_items(ctx: &PlanContext) -> Result<Vec<PathItem>> {
    let report = crate::path_engine::build_path(ctx, MAX_PREVIEW).ok();
    let mut items: Vec<PathItem> = Vec::new();
    if let Some(report) = report {
        for action in report.actions.iter().take(MAX_PREVIEW) {
            let mid_display = action
                .milestone
                .get("display")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mid_id = action
                .milestone
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let step_id = action.step.as_ref().map(|s| s.id.clone());
            let step_display = action.step.as_ref().map(|s| s.id.clone());
            let display = match (mid_display.is_empty(), step_display.as_ref()) {
                (false, Some(step)) => format!("{mid_display}/{step}"),
                (false, None) => mid_display,
                (true, Some(step)) => step.clone(),
                (true, None) => String::new(),
            };
            items.push(PathItem {
                id: if !mid_id.is_empty() {
                    mid_id.clone()
                } else {
                    step_id.clone().unwrap_or_default()
                },
                display,
                kind: action.r#type.clone(),
                milestone: if mid_id.is_empty() {
                    None
                } else {
                    Some(mid_id)
                },
                step: step_id,
            });
        }
    }
    Ok(items)
}

fn build_inbox_items(ctx: &PlanContext) -> Result<Vec<InboxItem>> {
    // Use the same single-snapshot loader as inbox::build_inbox so
    // the count matches; pull the items from the typed report
    // directly.
    let milestones = store::load_all_milestones(ctx)?;
    let validate_report = crate::validate::validate_plan_with_milestones(ctx, &milestones).ok();
    let validate_ok = validate_report.as_ref().map(|r| r.ok).unwrap_or(false);
    let validate_error_count = validate_report.as_ref().map(|r| r.errors.len());
    let pending = reviews::pending_reviews_from(ctx, &milestones).unwrap_or_default();
    let report = inbox::build_inbox_from(
        ctx,
        "actionable",
        &milestones,
        validate_ok,
        validate_error_count,
        &pending,
    )
    .ok();
    let items = report
        .map(|r| {
            r.items
                .into_iter()
                .take(MAX_PREVIEW)
                .map(|i| InboxItem {
                    id: i.id,
                    display: i.display.unwrap_or_default(),
                    kind: i.kind,
                    reason: i.reason,
                    action: i.action,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(items)
}
