//! M207 / AC-05: revisioned AC projection synchronized from canonical
//! milestone criterion state.
//!
//! `session.json` stores a projection of each milestone's AC state
//! alongside the source revision that produced the projection. A
//! later write must carry a `source_revision` that matches the
//! canonical state on disk — otherwise the projection is *stale* and
//! the write is rejected rather than silently overwriting.
//!
//! This module owns the projection shape, the revision helper, and
//! the [`ProjectionWriteOutcome`] enum returned by
//! [`project_ac_status`]. The sync itself is driven by the
//! autopilot CLI; this module is the contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Status projected from a canonical AC. Mirrors the `ac_projection`
/// `status` enum in the embedded schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcStatus {
    Pending,
    InProgress,
    Passed,
    Failed,
    Blocked,
}

impl AcStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            AcStatus::Pending => "pending",
            AcStatus::InProgress => "in-progress",
            AcStatus::Passed => "passed",
            AcStatus::Failed => "failed",
            AcStatus::Blocked => "blocked",
        }
    }
}

/// Projection of one AC stored in `session.json`. The `source_revision`
/// is the only thing that prevents a stale writer from clobbering the
/// canonical truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcProjection {
    pub ac_id: String,
    pub status: AcStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// Revision key the projection was sourced from. Same value
    /// re-submitted on the next sync counts as a no-op; a different
    /// value is the conflict signal.
    pub source_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_at: Option<String>,
}

/// Identity used to talk to the AC projection. A tuple of
/// `(milestone_id, ac_id)` — the projection is keyed first by
/// milestone because the canonical record (`plan.json`) is also
/// keyed that way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectionKey {
    pub milestone_id: String,
    pub ac_id: String,
}

impl ProjectionKey {
    pub fn new(milestone_id: impl Into<String>, ac_id: impl Into<String>) -> Self {
        Self {
            milestone_id: milestone_id.into(),
            ac_id: ac_id.into(),
        }
    }
}

/// Identity of the canonical source the next projection was sourced
/// from. The autopilot CLI computes this from
/// `<plan_dir>/plan.json` (revision key derived from the milestone's
/// acceptance-criteria array + the last-updated timestamp). Two
/// revisions are equal iff their canonical AC state is byte-equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectionRevision(pub String);

impl ProjectionRevision {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Outcome of a projection write. The autopilot CLI reports one of
/// these to the caller; only `Written` and `NoChange` are happy
/// paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionWriteOutcome {
    /// New projection stored.
    Written,
    /// `source_revision` matched the stored value and `status`
    /// matched too — no-op.
    NoChange,
    /// `source_revision` did not match the stored value. The
    /// caller must reconcile the canonical state before re-trying.
    StaleRevision {
        stored: ProjectionRevision,
        attempted: ProjectionRevision,
    },
}

impl std::fmt::Display for ProjectionWriteOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionWriteOutcome::Written => f.write_str("written"),
            ProjectionWriteOutcome::NoChange => f.write_str("no-change"),
            ProjectionWriteOutcome::StaleRevision { stored, attempted } => write!(
                f,
                "stale-revision stored={} attempted={}",
                stored.as_str(),
                attempted.as_str()
            ),
        }
    }
}

/// Apply a projection to a session. Returns the outcome and mutates
/// `session` in place only on the happy paths (`Written` /
/// `NoChange`). `StaleRevision` leaves `session` unchanged.
pub fn project_ac_status(
    session: &mut crate::autopilot::session::AutopilotSession,
    key: ProjectionKey,
    next: AcProjection,
) -> ProjectionWriteOutcome {
    let map = session
        .ac_projections
        .entry(key.milestone_id.clone())
        .or_default();
    if let Some(existing) = map.get(&key.ac_id) {
        if existing.source_revision == next.source_revision
            && existing.status == next.status
            && existing.evidence == next.evidence
        {
            return ProjectionWriteOutcome::NoChange;
        }
        if existing.source_revision != next.source_revision {
            return ProjectionWriteOutcome::StaleRevision {
                stored: ProjectionRevision(existing.source_revision.clone()),
                attempted: ProjectionRevision(next.source_revision),
            };
        }
    }
    map.insert(key.ac_id, next);
    ProjectionWriteOutcome::Written
}

/// Compose a canonical revision key from the bytes of an AC list.
/// Used by the autopilot CLI to derive the revision a projection was
/// sourced from. The hash is intentionally simple (xxhash is not in
/// the dependency tree; `DefaultHasher` is std-only) — the point is
/// stable equality, not cryptographic strength.
pub fn canonical_revision(seed: &str, milestone_id: &str, ac_states: &[(&str, AcStatus)]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    seed.hash(&mut h);
    milestone_id.hash(&mut h);
    for (ac_id, status) in ac_states {
        ac_id.hash(&mut h);
        status.as_str().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Per-milestone AC projection map. Keyed by AC id (`AC-01`, …).
pub type PerMilestoneProjections = BTreeMap<String, AcProjection>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::session::AutopilotSession;

    #[test]
    fn projection_writes_when_no_existing_entry() {
        let mut s = AutopilotSession::blank("s1");
        let key = ProjectionKey::new("207", "AC-01");
        let next = AcProjection {
            ac_id: "AC-01".into(),
            status: AcStatus::Passed,
            evidence: Some("ok".into()),
            source_revision: "rev-1".into(),
            projected_at: None,
        };
        let outcome = project_ac_status(&mut s, key, next.clone());
        assert_eq!(outcome, ProjectionWriteOutcome::Written);
        assert_eq!(
            s.ac_projections
                .get("207")
                .and_then(|m| m.get("AC-01")),
            Some(&next)
        );
    }

    #[test]
    fn projection_no_change_when_equivalent() {
        let mut s = AutopilotSession::blank("s1");
        let key = ProjectionKey::new("207", "AC-01");
        let next = AcProjection {
            ac_id: "AC-01".into(),
            status: AcStatus::Passed,
            evidence: Some("ok".into()),
            source_revision: "rev-1".into(),
            projected_at: None,
        };
        project_ac_status(&mut s, key.clone(), next.clone());
        let outcome = project_ac_status(&mut s, key, next);
        assert_eq!(outcome, ProjectionWriteOutcome::NoChange);
    }

    #[test]
    fn stale_revision_rejected() {
        let mut s = AutopilotSession::blank("s1");
        let key = ProjectionKey::new("207", "AC-01");
        project_ac_status(
            &mut s,
            key.clone(),
            AcProjection {
                ac_id: "AC-01".into(),
                status: AcStatus::Passed,
                evidence: Some("ok".into()),
                source_revision: "rev-1".into(),
                projected_at: None,
            },
        );
        // Same status, different revision -> stale.
        let outcome = project_ac_status(
            &mut s,
            key,
            AcProjection {
                ac_id: "AC-01".into(),
                status: AcStatus::Passed,
                evidence: Some("ok".into()),
                source_revision: "rev-2".into(),
                projected_at: None,
            },
        );
        match outcome {
            ProjectionWriteOutcome::StaleRevision { stored, attempted } => {
                assert_eq!(stored.as_str(), "rev-1");
                assert_eq!(attempted.as_str(), "rev-2");
            }
            other => panic!("expected StaleRevision, got {other:?}"),
        }
        // Stored projection is unchanged.
        assert_eq!(
            s.ac_projections
                .get("207")
                .and_then(|m| m.get("AC-01"))
                .map(|p| &p.source_revision),
            Some(&"rev-1".to_string())
        );
    }

    #[test]
    fn canonical_revision_stable_for_same_inputs() {
        let rev1 = canonical_revision("seed", "207", &[("AC-01", AcStatus::Passed)]);
        let rev2 = canonical_revision("seed", "207", &[("AC-01", AcStatus::Passed)]);
        assert_eq!(rev1, rev2);
    }

    #[test]
    fn canonical_revision_changes_on_status_change() {
        let rev1 = canonical_revision("seed", "207", &[("AC-01", AcStatus::Pending)]);
        let rev2 = canonical_revision("seed", "207", &[("AC-01", AcStatus::Passed)]);
        assert_ne!(rev1, rev2);
    }
}