//! `mp overview` work-queue counts (AC-07).
//!
//! Each count reuses the loader / filter that the existing canonical
//! command relies on so the Overview is byte-stable against its source
//! of truth. We do NOT introduce a second classifier — the M180 scope
//! decision is "queues by reusing existing loaders and filters".
//!
//! | Count                  | Source                                                |
//! |------------------------|-------------------------------------------------------|
//! | `inbox`                | `inbox::build_inbox_from(..., "actionable", ...)`     |
//! | `pending_reviews`      | `reviews::pending_reviews_from(ctx, milestones)`        |
//! | `backlog`              | `store::load_backlog(ctx)` items with `status=active` |
//! | `parked_ideas`         | `store::load_ideas(ctx)` items with `status=open`     |
//! | `open_annotations`     | `store::load_annotations(ctx)` items with `status=open` |
//! | `blocked_milestones`   | milestones with `execution_status=blocked` or `blocked=true` |
//! | `remediation_milestones` | milestones with `effective_lifecycle() == "remediation"` |

use anyhow::Result;
use serde::Serialize;

use crate::inbox;
use crate::paths::PlanContext;
use crate::reviews;
use crate::store;

use super::health::OverviewHealthBundle;

#[derive(Debug, Clone, Serialize, Default)]
pub struct OverviewQueues {
    pub inbox: usize,
    pub pending_reviews: usize,
    pub backlog: usize,
    pub parked_ideas: usize,
    pub open_annotations: usize,
    pub blocked_milestones: usize,
    pub remediation_milestones: usize,
}

pub fn build_queues(ctx: &PlanContext, health: &OverviewHealthBundle) -> Result<OverviewQueues> {
    // Blockers: re-derive from the loaded milestone snapshot so the
    // count is exact (the health builder already counted them; we
    // re-iterate only when the count diverges for any reason — the
    // shape is small enough that this is free).
    let milestones = store::load_all_milestones(ctx)?;
    let mut blocked_milestones = 0usize;
    let mut remediation_milestones = 0usize;
    for (_, m) in &milestones {
        if m.milestone.execution_status == "blocked" || m.milestone.blocked {
            blocked_milestones += 1;
        }
        if m.effective_lifecycle() == "remediation" {
            remediation_milestones += 1;
        }
    }

    // Inbox: reuse the existing actionable filter so the count
    // matches `mp inbox --filter actionable` exactly.
    let pending = reviews::pending_reviews_from(ctx, &milestones).unwrap_or_default();
    let validate_error_count = health.health.validation_error_count;
    let inbox_report = inbox::build_inbox_from(
        ctx,
        "actionable",
        &milestones,
        health.health.validation_state == "ok",
        Some(validate_error_count),
        &pending,
    )
    .ok();
    let inbox_count = inbox_report.as_ref().map(|r| r.count).unwrap_or(0);
    let pending_reviews_count = pending.len();

    // Backlog / ideas / annotations: direct loader access, same
    // semantics as the inbox row builders.
    let backlog_count = store::load_backlog(ctx)
        .map(|b| {
            b.items
                .iter()
                .filter(|i| i.status == "active" && !i.id.is_empty())
                .count()
        })
        .unwrap_or(0);
    let parked_ideas_count = store::load_ideas(ctx)
        .map(|s| {
            s.ideas
                .iter()
                .filter(|i| i.status == "open" && !i.id.is_empty())
                .count()
        })
        .unwrap_or(0);
    let open_annotations_count = store::load_annotations(ctx)
        .map(|s| s.annotations.iter().filter(|a| a.status == "open").count())
        .unwrap_or(0);

    Ok(OverviewQueues {
        inbox: inbox_count,
        pending_reviews: pending_reviews_count,
        backlog: backlog_count,
        parked_ideas: parked_ideas_count,
        open_annotations: open_annotations_count,
        blocked_milestones,
        remediation_milestones,
    })
}
