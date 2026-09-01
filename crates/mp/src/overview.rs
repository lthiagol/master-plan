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
//!   "activity": [ ... newest 5 events ... ],
//!   "mp_flow_stage_counts": { draft, groom, specify, approve, execute,
//!                             self-review, complete, external-review,
//!                             remediate, re-review, document, hand-off }
//! }
//! ```
//!
//! The aggregator reuses the same loaders the legacy
//! `cmd_status` / `cmd_inbox` use — no second-class classification
//! path. A milestone's lifecycle bucket, for instance, comes from
//! `crate::model::MilestoneFile::effective_lifecycle` exactly the way
//! every other surface reads it.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;

use crate::activity;
use crate::paths::PlanContext;

pub mod health;
pub mod path;
pub mod queues;

/// The full overview snapshot. Every public field is documented on
/// the corresponding submodule. `health`, `totals`, `lifecycle`,
/// `steps`, `queues`, `path`, `inbox`, `activity`, and
/// `mp_flow_stage_counts` are always present (empty arrays for
/// `path` / `inbox` / `activity` when the underlying data is missing
/// — never `null`).
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
    /// M202 / AC-16: 12-stage mp-flow rollup — how many milestones
    /// are currently at each stage (keyed by the canonical
    /// `MP_FLOW_STAGE_KEYS` slug). Always contains all 12 keys (zero
    /// for empty buckets) so the raul dashboard grid can render the
    /// full 12-bucket layout without defaulting logic. F-01 fix: the
    /// writer was missing in cycle 1, so the grid always rendered
    /// zeros.
    pub mp_flow_stage_counts: BTreeMap<String, usize>,
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
    let milestones = crate::store::load_all_milestones(ctx)?;
    let mp_flow_stage_counts = rollup_mp_flow_stages(&milestones);
    Ok(OverviewSnapshot {
        health: bundle.health.clone(),
        totals: bundle.totals.clone(),
        lifecycle: bundle.lifecycle.clone(),
        steps: bundle.steps.clone(),
        queues,
        path,
        inbox,
        activity: activity_events,
        mp_flow_stage_counts,
    })
}

/// M202 / AC-16: count milestones by their current mp-flow stage.
/// The current stage is derived via
/// `mp_model::current_mp_flow_stage` — the SAME single-source
/// derivation the raul Stage cell uses (F-01 / F-11), so the
/// dashboard grid and the list column can never disagree.
///
/// Every one of the 12 canonical slugs appears in the output with a
/// count (zero when empty) so the grid renders a full 12-bucket
/// layout.
fn rollup_mp_flow_stages(
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = mp_model::MP_FLOW_STAGE_KEYS
        .iter()
        .map(|slug| (slug.to_string(), 0usize))
        .collect();
    for (_, m) in milestones {
        let slug = mp_model::current_mp_flow_stage(&m.milestone.flow_stages);
        *counts.entry(slug.to_string()).or_insert(0) += 1;
    }
    counts
}

/// Condensed variant for `mp overview --summary` and the Overview tab
/// header strip. Drops the bounded `path` / `inbox` / `activity`
/// previews; keeps everything else (health / totals / lifecycle /
/// steps / queues / mp_flow_stage_counts) so a status line has the
/// same numbers as the full payload.
pub fn to_summary(snapshot: &OverviewSnapshot) -> serde_json::Value {
    serde_json::json!({
        "health": snapshot.health,
        "totals": snapshot.totals,
        "lifecycle": snapshot.lifecycle,
        "steps": snapshot.steps,
        "queues": snapshot.queues,
        "mp_flow_stage_counts": snapshot.mp_flow_stage_counts,
    })
}
