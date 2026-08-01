use anyhow::Result;
use serde::Serialize;

use crate::model::MilestoneIndexEntry;
use crate::paths::{self, PlanContext};
use crate::store;

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub ok: bool,
    pub milestones_indexed: usize,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

pub fn sync_plan(ctx: &PlanContext) -> Result<SyncReport> {
    ctx.ensure_plan_exists()?;
    let milestones = store::load_all_milestones(ctx)?;
    let mut plan = store::load_plan(ctx)?;

    let mut previous: std::collections::HashMap<String, MilestoneIndexEntry> = plan
        .milestones
        .iter()
        .map(|e| (paths::normalize_milestone_id(&e.id), e.clone()))
        .collect();

    let mut next_index = Vec::new();
    let mut added = Vec::new();
    let mut updated = Vec::new();

    for (_, m) in &milestones {
        let id = paths::normalize_milestone_id(&m.milestone.id);
        // M100: derive legacy field values from the unified lifecycle so the
        // plan.json index continues to expose `spec_status` and
        // `execution_status` for external consumers during the migration
        // window.
        let (spec_status, execution_status) = derive_index_status(m);
        let entry = MilestoneIndexEntry {
            id: id.clone(),
            title: m.milestone.title.clone(),
            spec_status,
            execution_status,
            blocked_by: m.milestone.blocked_by.clone(),
        };
        match previous.remove(&id) {
            None => added.push(id.clone()),
            Some(old) if index_changed(&old, &entry) => updated.push(id.clone()),
            _ => {}
        }
        next_index.push(entry);
    }

    next_index.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));

    let removed: Vec<String> = previous.keys().cloned().collect();
    plan.milestones = next_index.clone();
    store::write_plan(ctx, &plan)?;

    Ok(SyncReport {
        ok: true,
        milestones_indexed: next_index.len(),
        added,
        updated,
        removed,
    })
}

fn index_changed(old: &MilestoneIndexEntry, new: &MilestoneIndexEntry) -> bool {
    old.title != new.title
        || old.spec_status != new.spec_status
        || old.execution_status != new.execution_status
        || old.blocked_by != new.blocked_by
}

/// M100: derive legacy `spec_status` + `execution_status` from the unified
/// lifecycle so the plan.json index keeps emitting the legacy field names.
/// Used during the migration window only; once the bulk migration completes
/// and the legacy fields are removed from disk, this helper will be removed.
fn derive_index_status(m: &crate::model::MilestoneFile) -> (String, String) {
    let mut spec = m.milestone.spec_status.clone();
    let mut exec = m.milestone.execution_status.clone();
    if spec.is_empty() {
        spec = match m.effective_lifecycle().as_str() {
            "draft" => "draft".to_string(),
            "groomed" => "review".to_string(),
            "approved" => "ready".to_string(),
            "in-progress" => "ready".to_string(),
            "done" => "implemented".to_string(),
            "self-reviewed" => "implemented".to_string(),
            "reviewed" => "implemented".to_string(),
            "complete" => "verified".to_string(),
            "remediation" => "implemented".to_string(),
            other => other.to_string(),
        };
    }
    if exec.is_empty() {
        if m.milestone.blocked {
            exec = "blocked".to_string();
        } else if m.milestone.deferred {
            exec = "deferred".to_string();
        } else if m.milestone.cancelled {
            exec = "cancelled".to_string();
        } else {
            exec = match m.effective_lifecycle().as_str() {
                "draft" | "groomed" | "approved" => "planned".to_string(),
                "in-progress" => "in-progress".to_string(),
                "done" | "self-reviewed" | "reviewed" | "complete" | "remediation" => {
                    "done".to_string()
                }
                _ => "planned".to_string(),
            };
        }
    }
    (spec, exec)
}
