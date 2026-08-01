//! M181 S1: typed parser for M180's consolidated Overview JSON.
//!
//! M180 ships `mp overview` with a fixed, JSON-default payload (see
//! `crates/mp/src/overview.rs`). Raul is a thin rendering / interaction
//! adapter — it consumes the consolidated snapshot instead of fanning
//! out separate `mp status` + `mp inbox` subprocesses (the previous
//! pre-M181 design).
//!
//! The shape this parser consumes:
//!
//! ```text
//! {
//!   "health":    { validation_state, validation_error_count,
//!                  blocker_count, execution_mode, planning_state,
//!                  watch_state },
//!   "totals":    { milestones },
//!   "lifecycle": { draft, groomed, approved, in-progress, executed,
//!                  self-reviewed, reviewed, complete, remediation },
//!   "steps":     { pending, in_progress, done, failed, skipped },
//!   "queues":    { inbox, pending_reviews, backlog, parked_ideas,
//!                  open_annotations, blocked_milestones,
//!                  remediation_milestones },
//!   "path":      [ { id, display, kind, milestone?, step? }, ... ],
//!   "inbox":     [ { id, display, kind, reason, action }, ... ],
//!   "activity":  [ { timestamp, type, subject, summary }, ... ]
//! }
//! ```
//!
//! Each section is independently optional / list-of-zero-elements;
//! the parser falls back to `Default::default()` rather than failing
//! so a missing section on a fresh project renders as an explicit
//! empty state rather than aborting the load.

use serde::Deserialize;

/// Top-level typed view of M180's `mp overview` payload.
///
/// `path`, `inbox`, and `activity` are intentionally typed as owned
/// `Vec`s (not borrowed from the raw `Value`) so the snapshot is
/// `Clone + Send + 'static` and can sit on `App::dashboard_snapshot`
/// without lifetime gymnastics.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OverviewSnapshot {
    #[serde(default)]
    pub health: OverviewHealth,
    #[serde(default)]
    pub totals: OverviewTotals,
    #[serde(default)]
    pub lifecycle: LifecycleRollup,
    #[serde(default)]
    pub steps: StepRollup,
    #[serde(default)]
    pub queues: OverviewQueues,
    #[serde(default)]
    pub path: Vec<PathItem>,
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
    #[serde(default)]
    pub activity: Vec<ActivityEvent>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OverviewHealth {
    #[serde(default)]
    pub validation_state: String,
    #[serde(default)]
    pub validation_error_count: u64,
    #[serde(default)]
    pub blocker_count: u64,
    #[serde(default)]
    pub execution_mode: String,
    #[serde(default)]
    pub planning_state: String,
    /// One of `idle`, `running`, `stopped`, `failed`, `complete`.
    #[serde(default)]
    pub watch_state: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OverviewTotals {
    #[serde(default)]
    pub milestones: u64,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LifecycleRollup {
    #[serde(default)]
    pub draft: u64,
    #[serde(default)]
    pub groomed: u64,
    #[serde(default)]
    pub approved: u64,
    #[serde(default)]
    pub in_progress: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub self_reviewed: u64,
    #[serde(default)]
    pub reviewed: u64,
    #[serde(default)]
    pub complete: u64,
    #[serde(default)]
    pub remediation: u64,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StepRollup {
    #[serde(default)]
    pub pending: u64,
    #[serde(default)]
    pub in_progress: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub skipped: u64,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OverviewQueues {
    #[serde(default)]
    pub inbox: u64,
    #[serde(default)]
    pub pending_reviews: u64,
    #[serde(default)]
    pub backlog: u64,
    #[serde(default)]
    pub parked_ideas: u64,
    #[serde(default)]
    pub open_annotations: u64,
    #[serde(default)]
    pub blocked_milestones: u64,
    #[serde(default)]
    pub remediation_milestones: u64,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PathItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub milestone: Option<String>,
    #[serde(default)]
    pub step: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct InboxItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ActivityEvent {
    #[serde(default)]
    pub timestamp: String,
    /// Event discriminator in kebab-case
    /// (`milestone-created`, `lifecycle-transition`, …).
    #[serde(rename = "type", default)]
    pub event_type: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub summary: String,
}

/// Parse the `mp overview` JSON payload into a typed
/// [`OverviewSnapshot`]. Missing / malformed sections fall back to
/// `Default::default()` so a partial payload (or a future mp version
/// without a new section) renders as an empty state rather than
/// aborting the load.
pub fn parse(raw: &serde_json::Value) -> OverviewSnapshot {
    serde_json::from_value(raw.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_full_payload() {
        let raw = json!({
            "health": {
                "validation_state": "ok",
                "validation_error_count": 0,
                "blocker_count": 1,
                "execution_mode": "autonomous",
                "planning_state": "in-execution",
                "watch_state": "running"
            },
            "totals": { "milestones": 42 },
            "lifecycle": {
                "draft": 1, "groomed": 2, "approved": 3, "in_progress": 4,
                "executed": 5, "self_reviewed": 6, "reviewed": 7, "complete": 8,
                "remediation": 0
            },
            "steps": {
                "pending": 5, "in_progress": 2, "done": 30, "failed": 1,
                "skipped": 0
            },
            "queues": {
                "inbox": 3, "pending_reviews": 2, "backlog": 4,
                "parked_ideas": 1, "open_annotations": 0,
                "blocked_milestones": 1, "remediation_milestones": 0
            },
            "path": [
                { "id": "180", "display": "M180/S1", "kind": "step",
                  "milestone": "180", "step": "S1" }
            ],
            "inbox": [
                { "id": "180", "display": "M180", "kind": "milestone",
                  "reason": "needs grooming", "action": "mp milestone groom 180" }
            ],
            "activity": [
                { "timestamp": "2026-07-17T18:00:00Z", "type": "milestone-created",
                  "subject": "180", "summary": "milestone created (180)" }
            ]
        });
        let s = parse(&raw);
        assert_eq!(s.health.validation_state, "ok");
        assert_eq!(s.totals.milestones, 42);
        assert_eq!(s.lifecycle.complete, 8);
        assert_eq!(s.steps.done, 30);
        assert_eq!(s.queues.inbox, 3);
        assert_eq!(s.path.len(), 1);
        assert_eq!(s.path[0].display, "M180/S1");
        assert_eq!(s.inbox[0].id, "180");
        assert_eq!(s.activity[0].event_type, "milestone-created");
    }

    #[test]
    fn empty_object_yields_default_snapshot() {
        let s = parse(&json!({}));
        assert_eq!(s.totals.milestones, 0);
        assert!(s.path.is_empty());
        assert!(s.inbox.is_empty());
        assert!(s.activity.is_empty());
        assert_eq!(s.lifecycle.complete, 0);
    }

    #[test]
    fn malformed_payload_falls_back_to_default() {
        // Wrong types in nested fields — the deserializer fails on the
        // first type mismatch; the wrapper falls back to default rather
        // than propagating the error so a partial payload never blocks
        // the Overview load.
        let s = parse(&json!({ "totals": { "milestones": "not a number" } }));
        assert_eq!(s.totals.milestones, 0);
        assert!(s.path.is_empty());
    }

    #[test]
    fn lifecycle_zero_buckets_stay_zero() {
        let s = parse(&json!({
            "lifecycle": {
                "draft": 0, "groomed": 0, "approved": 0, "in_progress": 0,
                "done": 0, "self_reviewed": 0, "reviewed": 0, "complete": 0,
                "remediation": 0
            }
        }));
        assert_eq!(s.lifecycle.draft, 0);
        assert_eq!(s.lifecycle.complete, 0);
        // Sum of buckets should match `totals.milestones` (mp enforces
        // this invariant; the test pins the read shape).
        let sum = s.lifecycle.draft
            + s.lifecycle.groomed
            + s.lifecycle.approved
            + s.lifecycle.in_progress
            + s.lifecycle.done
            + s.lifecycle.self_reviewed
            + s.lifecycle.reviewed
            + s.lifecycle.complete
            + s.lifecycle.remediation;
        assert_eq!(sum, 0);
    }

    #[test]
    fn activity_event_type_field_renames_to_event_type() {
        // M180 emits the discriminator under the JSON key `type`
        // (Rust's `type` is a reserved keyword). The serde rename in
        // `ActivityEvent` pins the on-disk shape; this test guards a
        // future rename that would silently drop activity rows.
        let raw = json!({
            "activity": [
                { "timestamp": "t", "type": "validation-state",
                  "subject": "", "summary": "ok" }
            ]
        });
        let s = parse(&raw);
        assert_eq!(s.activity[0].event_type, "validation-state");
    }
}
