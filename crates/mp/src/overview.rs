//! M180: project-overview read model.
//!
//! [`build_overview`] is the single consolidated snapshot mp exposes
//! via `cmd_overview`. Raul consumes this
//! payload to render the Overview dashboard; ad-hoc scripts read it
//! through `--fields`.
//!
//! The shape mirrors the AC-05..AC-08 contract:
//!
//! ```text
//! {
//!   "health": { validation_state, validation_error_count,
//!               blocker_count, execution_mode, planning_state,
//!               watch_state },
//!   "totals": { milestones },
//!   "lifecycle": { draft, groomed, approved, in-progress, done,
//!                  self-reviewed, reviewed, complete, remediation },
//!   "steps":   { pending, in_progress, done, failed, skipped },
//!   "queues":  { inbox, pending_reviews, backlog, parked_ideas,
//!                open_annotations, blocked_milestones,
//!                remediation_milestones },
//!   "path":    [ ... 3..5 items ... ],
//!   "inbox":   [ ... up to 5 actionable items ... ],
//!   "activity": [ ... newest 5 events ... ]
//! }
//! ```
//!
//! The aggregator reuses the same loaders the legacy
//! `cmd_status` / `cmd_inbox` use — no second-class classification
//! path. A milestone's lifecycle bucket, for instance, comes from
//! `crate::model::MilestoneFile::effective_lifecycle` exactly the way
//! every other surface reads it.

use anyhow::Result;
use serde::Serialize;

use crate::activity;
use crate::paths::PlanContext;

pub mod health;
pub mod path;
pub mod queues;

/// The full overview snapshot. Every public field is documented on
/// the corresponding submodule. `health`, `totals`, `lifecycle`,
/// `steps`, `queues`, `path`, `inbox`, and `activity` are always
/// present (empty arrays for `path` / `inbox` / `activity` when the
/// underlying data is missing — never `null`).
#[derive(Debug, Clone, Serialize)]
pub struct OverviewSnapshot {
    pub health: health::OverviewHealth,
    pub totals: health::OverviewTotals,
    pub lifecycle: health::LifecycleRollup,
    pub steps: health::StepRollup,
    pub queues: queues::OverviewQueues,
    pub path: Vec<path::PathItem>,
    pub inbox: Vec<path::InboxItem>,
    pub activity: Vec<activity::ActivityEvent>,
}

/// Build the consolidated overview snapshot. Cheap relative to a full
/// `cmd_status` pass — the loader shares one milestone directory scan
/// across health + queues + lifecycle + steps (no duplicate
/// `load_all_milestones`).
pub fn build_overview(ctx: &PlanContext) -> Result<OverviewSnapshot> {
    let bundle = health::build_health(ctx)?;
    let queues = queues::build_queues(ctx, &bundle)?;
    let (path, inbox) = path::build_path_and_inbox(ctx)?;
    let activity_events = activity::read_recent_events(ctx, 5)?;
    Ok(OverviewSnapshot {
        health: bundle.health.clone(),
        totals: bundle.totals.clone(),
        lifecycle: bundle.lifecycle.clone(),
        steps: bundle.steps.clone(),
        queues,
        path,
        inbox,
        activity: activity_events,
    })
}

/// Condensed variant for `mp overview --summary` and the Overview tab
/// header strip. Drops the bounded `path` / `inbox` / `activity`
/// previews; keeps everything else (health / totals / lifecycle /
/// steps / queues) so a status line has the same numbers as the full
/// payload.
pub fn to_summary(snapshot: &OverviewSnapshot) -> serde_json::Value {
    serde_json::json!({
        "health": snapshot.health,
        "totals": snapshot.totals,
        "lifecycle": snapshot.lifecycle,
        "steps": snapshot.steps,
        "queues": snapshot.queues,
    })
}
