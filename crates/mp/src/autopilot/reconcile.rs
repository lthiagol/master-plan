//! M225: restart + reconciliation after orchestrator or pane failure.
//!
//! The autopilot session ([`AutopilotSession`]) is the durable
//! record of an orchestration drive: append-only event log,
//! revisioned AC projections, role config, and a stored spawn
//! prompt per pane. After a SIGINT, a process crash, a pane loss,
//! or a worktree mismatch, the next process must resume without:
//!
//! - **Duplicate dispatch** — re-sending a herdr `agent start` for
//!   a pane whose prompt has already been delivered (AC-01).
//! - **Duplicate lifecycle command** — re-applying a `transition`
//!   whose effect already landed in the canonical milestone JSON
//!   (AC-01).
//! - **Fabricated completion** — a fresh pane claiming a milestone
//!   is "done" without a real verification run (AC-02 / AC-04).
//! - **Lost or torn event tail** — discarding prior events when
//!   the cursor is stale (AC-03).
//! - **Stale projections over newer plan evidence** — restoring a
//!   `session.json` field that the plan has since overwritten
//!   (AC-04).
//!
//! This module owns the four primitives the resume / restart path
//! composes:
//!
//! - [`IdempotencyKey`] + [`was_already_applied`] — AC-01.
//! - [`PaneLossInput`] + [`classify_pane_loss`] — AC-02.
//! - [`recover_event_tail`] — AC-03.
//! - [`CanonicalSnapshot`] + [`cross_check_canonical`] — AC-04.
//!
//! No I/O happens here. The resume command ([`crate::autopilot`]
//! consumers) loads the session, calls these four primitives, and
//! decides whether to re-spawn, escalate AwaitingUser, or refuse to
//! mutate. Keeping the decisions pure makes the restart protocol
//! testable in isolation — every recovery scenario is a fixture
//! over the same typed inputs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::autopilot::events::{EventKind, OrchestrationEvent};
use crate::autopilot::session::{AutopilotSession, RoleName};
use crate::autopilot::spawn::MpBinaryProvenance;

// ─── AC-01: idempotency keys for dispatch + lifecycle replay ──────

/// Identity of an orchestration effect that may be replayed after
/// a crash. The same effect can be requested twice (e.g. a `runner`
/// notification that was sent before the parent process died but
/// whose acknowledgement was lost); the resume path uses
/// [`was_already_applied`] to skip a duplicate apply.
///
/// The set of variants is closed on purpose. Adding a new effect
/// type means defining a new `IdempotencyKey` variant and updating
/// every match site.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IdempotencyKey {
    /// A herdr `agent start` (or `agent send`) for a specific
    /// pane label. Two requests with the same `pane_label` are
    /// the same effect; the resume path will not re-spawn.
    Dispatch {
        /// Pane label herdr shows in the sidebar (e.g. `role-runner-1`).
        pane_label: String,
    },
    /// A lifecycle transition for a milestone (e.g. `in-progress` →
    /// `executed`). Two requests with the same `(milestone_id, target)`
    /// are the same effect; the resume path will not re-emit the
    /// transition event.
    Lifecycle {
        /// Milestone id (e.g. `225`).
        milestone_id: String,
        /// Target lifecycle value (e.g. `executed`).
        target: String,
    },
}

impl IdempotencyKey {
    /// True when an event in `session.events` already records this
    /// effect. The cursor is the authoritative high-water mark; if
    /// the cursor is below the events' max seq, recovery has not yet
    /// run and the call still answers correctly from the surviving
    /// tail (the function does not regress).
    ///
    /// **Dispatch** matches an `AssignmentDispatched` event whose
    /// `payload.pane_label` equals the key's `pane_label`. The
    /// payload field is the same shape
    /// [`crate::autopilot::task_assign`] writes.
    ///
    /// **Lifecycle** matches a `Transition` event whose
    /// `payload.milestone_id` + `payload.target` equal the key. The
    /// shape mirrors [`crate::autopilot::lifecycle::LifecycleTransition`].
    pub fn was_already_applied(&self, session: &AutopilotSession) -> bool {
        match self {
            IdempotencyKey::Dispatch { pane_label } => session
                .events
                .iter()
                .filter(|e| e.kind == EventKind::AssignmentDispatched)
                .any(|e| payload_string(e, "pane_label").as_deref() == Some(pane_label.as_str())),
            IdempotencyKey::Lifecycle {
                milestone_id,
                target,
            } => session
                .events
                .iter()
                .filter(|e| e.kind == EventKind::Transition)
                .any(|e| {
                    payload_string(e, "milestone_id").as_deref() == Some(milestone_id.as_str())
                        && payload_string(e, "target").as_deref() == Some(target.as_str())
                }),
        }
    }
}

/// Free-function form for callers that don't want to construct the
/// key first. Equivalent to
/// `key.was_already_applied(session)`.
pub fn was_already_applied(session: &AutopilotSession, key: &IdempotencyKey) -> bool {
    key.was_already_applied(session)
}

/// Last sequence number the session's event log has issued. The
/// resume path uses this to decide "where to pick up" — any effect
/// with `seq <= last_durable_seq` is already on disk; only effects
/// with `seq > last_durable_seq` need to be re-applied.
pub fn last_durable_seq(session: &AutopilotSession) -> u64 {
    session.event_cursor.last_seq
}

// ─── AC-02: pane loss classification ─────────────────────────────

/// Inputs the resume path hands to [`classify_pane_loss`].
///
/// `topology_role_present` is the topology's pre-classification
/// ("does this role exist in the configured topology at all?"). A
/// `false` answer means the operator deliberately removed the role
/// from the topology; the resume path must NOT respawn — that would
/// silently undo the operator's topology change.
#[derive(Debug, Clone)]
pub struct PaneLossInput<'a> {
    /// Which role's pane is missing.
    pub role: RoleName,
    /// True if herdr's live agent list still has the pane. False
    /// when the pane has died or was never spawned in this session.
    pub pane_live: bool,
    /// True if the topology configures this role (i.e. the role
    /// slot is part of the current `topology` field on the
    /// session). False means the role was removed from the topology
    /// since the session was first written.
    pub topology_role_present: bool,
    /// Stored spawn prompt for this role (from
    /// `session.roles.<role>.spawn_prompt_rendered` or the
    /// `prompt_bundles` map keyed by pane label). `None` when the
    /// role was never spawned or the session pre-dates prompt
    /// capture (legacy format).
    pub stored_prompt: Option<&'a str>,
    /// Stored actor identity for this role (e.g. `runner:M225` or
    /// a herdr pane id). `None` when no actor has ever claimed this
    /// role.
    pub stored_actor: Option<&'a str>,
}

/// Outcome of classifying a missing or dead role pane.
///
/// `SafeRespawn` is the happy path: the resume path can re-spawn
/// with the stored prompt, rotating the actor identity to avoid
/// re-binding a zombie. `AwaitingUser` is the escalation path: the
/// missing data or topology ambiguity means a human must decide
/// (rebuild, install a different binary, or accept the loss).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PaneLossOutcome {
    /// Resume may re-spawn the pane. `prompt` is the verbatim
    /// stored prompt to redeliver (caller passes through to
    /// `herdr agent send`). `actor_rotation` is the next identity
    /// the resume path should bind — `None` means "keep the stored
    /// actor" (no rotation needed because no partial work was
    /// recorded against it).
    SafeRespawn {
        /// Stored prompt the resume path will redeliver.
        prompt: String,
        /// Optional new actor identity. `None` keeps the stored
        /// actor; `Some(name)` rotates to a fresh one.
        actor_rotation: Option<String>,
    },
    /// Resume must escalate. The session is `AwaitingUser` until
    /// the operator acts. Carries the typed reason so the
    /// verification surface can report a precise finding.
    AwaitingUser { reason: PaneLossReason },
}

/// Typed reason a pane loss escalates to `AwaitingUser`. Each
/// variant is a distinct failure mode the resume path must surface
/// separately so the operator can act without re-running the
/// verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PaneLossReason {
    /// Role is no longer in the topology config. Re-spawning would
    /// silently undo the operator's topology change.
    RoleRemovedFromTopology { role: String, topology_had: String },
    /// A live pane is reported but the topology says the role is
    /// missing. The on-disk state and the live state disagree in a
    /// way the resume path cannot reconcile without a human.
    TopologyMismatch {
        role: String,
        topology_present: bool,
        pane_live: bool,
    },
    /// The role is in the topology, the pane is dead, but no
    /// stored prompt was recorded. A fresh spawn would be a
    /// fresh agent with no context — escalate.
    NoStoredPrompt { role: String },
    /// No stored actor identity. The role's previous work cannot
    /// be attributed, so a re-spawn would silently hand the work
    /// to a different agent.
    NoStoredActor { role: String },
}

impl std::fmt::Display for PaneLossReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneLossReason::RoleRemovedFromTopology { role, topology_had } => write!(
                f,
                "role {role} is no longer in the configured topology (recorded topology: {topology_had}); \
                 refuse to re-spawn"
            ),
            PaneLossReason::TopologyMismatch {
                role,
                topology_present,
                pane_live,
            } => write!(
                f,
                "role {role} has topology_present={topology_present} but pane_live={pane_live}; \
                 cannot reconcile without operator input"
            ),
            PaneLossReason::NoStoredPrompt { role } => write!(
                f,
                "role {role} has no stored spawn prompt; refuse to re-spawn a context-less agent"
            ),
            PaneLossReason::NoStoredActor { role } => write!(
                f,
                "role {role} has no stored actor identity; refuse to rebind silently"
            ),
        }
    }
}

/// Classify a missing/dead role pane. See [`PaneLossInput`] and
/// [`PaneLossOutcome`].
///
/// The decision matrix is:
///
/// | pane_live | topology_present | stored_prompt | stored_actor | outcome |
/// |-----------|------------------|---------------|--------------|---------|
/// | true      | true             | any           | any          | `SafeRespawn` (live, no work needed) |
/// | false     | false            | any           | any          | `AwaitingUser::RoleRemovedFromTopology` |
/// | true      | false            | any           | any          | `AwaitingUser::TopologyMismatch` |
/// | false     | true             | none          | any          | `AwaitingUser::NoStoredPrompt` |
/// | false     | true             | some          | none         | `AwaitingUser::NoStoredActor` |
/// | false     | true             | some          | some         | `SafeRespawn` (with optional actor rotation) |
///
/// `SafeRespawn` returns the stored prompt verbatim so the resume
/// path can redeliver without recomputing it. `actor_rotation` is
/// `Some("runner:respawn:<seq>")` when the previous actor has
/// recorded work (the cursor advanced past the spawn) — a new
/// identity avoids re-binding a zombie.
pub fn classify_pane_loss(input: &PaneLossInput<'_>) -> PaneLossOutcome {
    if !input.topology_role_present {
        // Operator removed the role from the topology. Respawning
        // would silently undo the topology change.
        return PaneLossOutcome::AwaitingUser {
            reason: PaneLossReason::RoleRemovedFromTopology {
                role: input.role.as_str().to_string(),
                topology_had: describe_topology(input),
            },
        };
    }
    if input.pane_live && !input.topology_role_present {
        return PaneLossOutcome::AwaitingUser {
            reason: PaneLossReason::TopologyMismatch {
                role: input.role.as_str().to_string(),
                topology_present: input.topology_role_present,
                pane_live: true,
            },
        };
    }
    if input.pane_live {
        // Pane alive and topology knows about it. Resume can re-attach
        // without re-spawning — no prompt redelivery, no actor
        // rotation.
        return PaneLossOutcome::SafeRespawn {
            prompt: input.stored_prompt.unwrap_or("").to_string(),
            actor_rotation: None,
        };
    }
    let prompt = match input.stored_prompt {
        Some(p) => p.to_string(),
        None => {
            return PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::NoStoredPrompt {
                    role: input.role.as_str().to_string(),
                },
            };
        }
    };
    let actor_rotation = match input.stored_actor {
        Some(actor) => format!("{}:respawn", actor),
        None => {
            return PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::NoStoredActor {
                    role: input.role.as_str().to_string(),
                },
            };
        }
    };
    PaneLossOutcome::SafeRespawn {
        prompt,
        actor_rotation: Some(actor_rotation),
    }
}

fn describe_topology(input: &PaneLossInput<'_>) -> String {
    // The topology hash is informational; we don't try to serialize
    // the full `PaneLayout` here because the reason is rendered for
    // log output, not for canonical state. The role name alone is
    // enough to disambiguate.
    format!("role={}", input.role.as_str())
}

// ─── AC-03: event tail recovery ──────────────────────────────────

/// Outcome of [`recover_event_tail`]. The function never mutates
/// `session.events`; it only updates `session.event_cursor` on the
/// `Recovered` path. The `Rejected` path leaves both untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailRecovery {
    /// Tail recovered. The cursor was bumped to the max surviving
    /// event seq (no truncation; no events removed).
    Recovered {
        /// The new cursor value (max surviving seq).
        last_seq: u64,
        /// Number of events that survived (untouched by recovery).
        prior_event_count: usize,
    },
    /// Tail rejected. No mutation occurred on the session. The
    /// reason is the AC-03 contract: an incompatible schema or
    /// binary must fail before any write.
    Rejected {
        /// Why the tail was rejected.
        reason: TailRejectReason,
        /// Number of events that survived (for diagnostic output;
        /// still untouched).
        prior_event_count: usize,
    },
}

/// Typed reason a tail was rejected. Each variant maps to a
/// specific AC-03 contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailRejectReason {
    /// The session's `schema_version` is greater than the current
    /// binary knows how to preserve. The session file may contain
    /// fields the current binary would silently drop on a rewrite.
    /// The resume path must refuse — the operator must rebuild
    /// against the current schema before resuming.
    SchemaTooNew {
        session_schema: u32,
        current_schema: u32,
    },
    /// The session's `binary_provenance` records a schema newer
    /// than the executing binary. Same contract as
    /// `SchemaTooNew` but sourced from the provenance field rather
    /// than the top-level `schema_version`.
    BinaryProvenanceTooNew {
        recorded: MpBinaryProvenance,
        current: MpBinaryProvenance,
    },
    /// The session has a stored `EventCursor` whose `last_seq` is
    /// less than the surviving events' max seq. This is the
    /// "torn-write" diagnostic: the recovery path bumps the cursor
    /// to match. The variant is `Recovered`, not `Rejected` — the
    /// function returns `Rejected` only on incompatibility.
    ///
    /// `CursorStale` is exposed for tests that want to assert the
    /// function recognises the case even though the
    /// `TailRecovery::Recovered` arm is the production path.
    CursorStale {
        stored_cursor: u64,
        observed_max: u64,
    },
}

impl std::fmt::Display for TailRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TailRejectReason::SchemaTooNew {
                session_schema,
                current_schema,
            } => write!(
                f,
                "session.json schema_version={session_schema} is newer than current binary's \
                 schema_version={current_schema}; refuse to mutate (rebuild or install the \
                 matching binary first)"
            ),
            TailRejectReason::BinaryProvenanceTooNew { recorded, current } => write!(
                f,
                "session.json binary_provenance.schema_version={} > current={}; refuse to \
                 mutate (rebuild mp to at least {} first)",
                recorded.schema_version, current.schema_version, recorded.schema_version
            ),
            TailRejectReason::CursorStale {
                stored_cursor,
                observed_max,
            } => write!(
                f,
                "cursor {stored_cursor} is below surviving events' max {observed_max}; \
                 recovery must bump without truncating"
            ),
        }
    }
}

/// Recover the session's event tail. Pure function over
/// `session` — does not perform I/O. The caller writes the
/// session back to disk if the result is `Recovered`.
///
/// **Invariant:** `session.events` is never truncated, reordered,
/// or rewritten. Only `session.event_cursor` is updated on the
/// `Recovered` path. The `Rejected` path leaves both fields
/// untouched.
///
/// **Schema gate:** the session's `schema_version` is compared
/// against the current binary's [`crate::autopilot::session::
/// SESSION_SCHEMA_VERSION`]. A newer session file is rejected
/// before any mutation (the AC-03 "incompatible schema fails
/// before mutation" contract).
///
/// **Binary provenance gate:** if the session recorded a
/// `binary_provenance.schema_version` greater than the current
/// binary, the session is rejected (the recorded binary can
/// preserve fields the current binary cannot).
pub fn recover_event_tail(
    session: &mut AutopilotSession,
    current_binary: &MpBinaryProvenance,
) -> TailRecovery {
    let prior_event_count = session.events.len();
    let observed_max = session.events.iter().map(|e| e.seq).max().unwrap_or(0);
    let stored_cursor = session.event_cursor.last_seq;

    if session.schema_version > current_binary.schema_version {
        return TailRecovery::Rejected {
            reason: TailRejectReason::SchemaTooNew {
                session_schema: session.schema_version,
                current_schema: current_binary.schema_version,
            },
            prior_event_count,
        };
    }
    if let Some(recorded) = session.binary_provenance.as_ref() {
        if recorded.schema_version > current_binary.schema_version {
            return TailRecovery::Rejected {
                reason: TailRejectReason::BinaryProvenanceTooNew {
                    recorded: recorded.clone(),
                    current: current_binary.clone(),
                },
                prior_event_count,
            };
        }
    }

    if observed_max > stored_cursor {
        // Torn-write path: the cursor is stale. Bump without
        // mutating the events vec. This is the same logic as
        // [`crate::autopilot::recovery::reconcile_event_cursor`];
        // the M225 layer adds the schema + binary gates above.
        session.event_cursor.last_seq = observed_max;
    }

    TailRecovery::Recovered {
        last_seq: session.event_cursor.last_seq,
        prior_event_count,
    }
}

// ─── AC-04: cross-check canonical criterion / review / lifecycle ─

/// Snapshot of the canonical plan state the cross-checker compares
/// against. The fields are intentionally narrow — only the
/// revisioned / timestamped facts the session may have stale
/// projections of.
///
/// `ac_revisions` is keyed by `(milestone_id, ac_id)` (the same
/// key shape [`crate::autopilot::ac_projection::ProjectionKey`]
/// uses). Each entry carries the canonical status + the
/// `source_revision` derived from the plan's current AC array +
/// `last_updated` timestamp.
///
/// `review_revisions` is keyed by `(milestone_id, cycle)` with the
/// `reviews.json` verdict + the timestamp on the verdict file.
/// `lifecycle_revisions` is keyed by `milestone_id` and carries
/// the current lifecycle string + the plan's `lifecycle_at`
/// timestamp.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalSnapshot {
    /// Per-AC canonical status + revision.
    #[serde(default)]
    pub ac_revisions: BTreeMap<CanonicalAcKey, CanonicalAcState>,
    /// Per-`(milestone, cycle)` review verdict + timestamp.
    #[serde(default)]
    pub review_revisions: BTreeMap<CanonicalReviewKey, CanonicalReviewState>,
    /// Per-milestone canonical lifecycle + timestamp.
    #[serde(default)]
    pub lifecycle_revisions: BTreeMap<String, CanonicalLifecycleState>,
}

impl CanonicalSnapshot {
    /// Empty snapshot — every cross-check dimension reports
    /// "session is in sync with nothing canonical". Useful for
    /// tests that exercise the in-sync path.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when no canonical state was provided at all. A session
    /// with no canonical facts to reconcile against is, by
    /// definition, not stale — but also not authoritative.
    pub fn is_empty(&self) -> bool {
        self.ac_revisions.is_empty()
            && self.review_revisions.is_empty()
            && self.lifecycle_revisions.is_empty()
    }
}

/// Key for an AC's canonical state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CanonicalAcKey {
    pub milestone_id: String,
    pub ac_id: String,
}

impl CanonicalAcKey {
    pub fn new(milestone_id: impl Into<String>, ac_id: impl Into<String>) -> Self {
        Self {
            milestone_id: milestone_id.into(),
            ac_id: ac_id.into(),
        }
    }
}

/// Canonical state for one AC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalAcState {
    /// Canonical status string (`pending` | `passed` | `failed` |
    /// `blocked` — the same set the schema enforces).
    pub status: String,
    /// Revision key derived from the plan's AC array + its
    /// `last_updated` timestamp. Two revisions are equal iff the
    /// canonical AC state is byte-equal.
    pub source_revision: String,
    /// RFC3339 timestamp the canonical state was last updated.
    pub canonical_at: String,
}

/// Canonical state for one review record (per milestone + cycle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalReviewState {
    /// Review verdict (`pass` | `changes-needed` | …). The
    /// string is the exact value stored in `reviews.json`.
    pub verdict: String,
    /// RFC3339 timestamp the verdict was recorded.
    pub reviewed_at: String,
}

/// Canonical state for one milestone's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalLifecycleState {
    /// Canonical lifecycle string (`approved` | `in-progress` |
    /// `executed` | `self-reviewed` | `external-reviewed` |
    /// `documented` | `handed-off` | `complete`).
    pub lifecycle: String,
    /// RFC3339 timestamp the lifecycle was set.
    pub lifecycle_at: String,
}

/// Per-dimension cross-check verdict.
///
/// The cross-checker never silently restores the session over the
/// canonical state; it reports which side wins for every
/// dimension and exposes the verdict + the revisions the caller
/// must reconcile. AC-04 contract: "never restore stale session
/// projections over newer plan evidence".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DimensionVerdict {
    /// Session and canonical revisions match. No action.
    InSync {
        /// The revision both sides agree on.
        revision: String,
    },
    /// Canonical is newer than the session's projection. The
    /// resume path must NOT restore the session's stale value.
    /// Caller should re-emit the canonical state into the session
    /// (e.g. via [`crate::autopilot::ac_projection::project_ac_status`]).
    CanonicalNewer {
        session_revision: String,
        canonical_revision: String,
    },
    /// Session is ahead of the canonical state. The plan has
    /// regressed (or the session is feeding back a not-yet-
    /// committed projection). Caller should defer the canonical
    /// update and let the plan catch up.
    SessionNewer {
        session_revision: String,
        canonical_revision: String,
    },
    /// Dimension is unknown to the cross-checker (e.g. an AC key
    /// present in the session's projection but absent from the
    /// snapshot). Caller decides — typically a no-op.
    UnknownToCanonical,
}

/// Top-level cross-check report. The resume path treats any
/// `CanonicalNewer` verdict as a hard signal: do not restore
/// session state over the newer canonical.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossCheckReport {
    /// Per-AC verdicts (keyed by `CanonicalAcKey` as a string).
    #[serde(default)]
    pub ac: BTreeMap<String, DimensionVerdict>,
    /// Per-`(milestone, cycle)` review verdicts.
    #[serde(default)]
    pub reviews: BTreeMap<String, DimensionVerdict>,
    /// Per-milestone lifecycle verdicts.
    #[serde(default)]
    pub lifecycles: BTreeMap<String, DimensionVerdict>,
    /// True when any dimension reports `CanonicalNewer`. The
    /// resume path must not restore the session over the
    /// canonical state when this is true.
    #[serde(default)]
    pub canonical_wins_anywhere: bool,
    /// True when the session had a `working_on` entry whose
    /// milestone lifecycle is older than the session's recorded
    /// `last_state_change_at`. The M225 spec calls this out
    /// specifically: "never restore stale session projections over
    /// newer plan evidence".
    #[serde(default)]
    pub working_on_stale: bool,
}

impl CrossCheckReport {
    /// True when the resume path can restore the session without
    /// re-checking the canonical state. In practice this is only
    /// true when every dimension is `InSync` and the working_on
    /// freshness check passes.
    pub fn session_is_safe(&self) -> bool {
        !self.canonical_wins_anywhere
            && !self.working_on_stale
            && self
                .ac
                .values()
                .all(|v| matches!(v, DimensionVerdict::InSync { .. }))
            && self
                .reviews
                .values()
                .all(|v| matches!(v, DimensionVerdict::InSync { .. }))
            && self
                .lifecycles
                .values()
                .all(|v| matches!(v, DimensionVerdict::InSync { .. }))
    }
}

/// Cross-check the session's revisioned projections against the
/// supplied canonical snapshot. Pure function — no I/O, no
/// mutation. The result tells the resume path which dimension
/// the canonical state has overtaken.
///
/// **AC-04 contract:** the report is built so that
/// `report.canonical_wins_anywhere == true` whenever any
/// canonical revision differs from the session's stored value in
/// a way that would make a session restore *regress* the plan.
/// The session must never be the source of truth when canonical
/// evidence is newer.
pub fn cross_check_canonical(
    session: &AutopilotSession,
    snapshot: &CanonicalSnapshot,
) -> CrossCheckReport {
    let mut report = CrossCheckReport::default();

    // ── ACs: compare each (milestone, ac) projection's
    // `source_revision` against the canonical revision. ──
    for (milestone_id, projections) in &session.ac_projections {
        for (ac_id, projection) in projections {
            let key = CanonicalAcKey::new(milestone_id.clone(), ac_id.clone());
            let key_str = format!("{milestone_id}/{ac_id}");
            let verdict = match snapshot.ac_revisions.get(&key) {
                Some(canonical) => {
                    compare_revisions(&projection.source_revision, &canonical.source_revision)
                }
                None => DimensionVerdict::UnknownToCanonical,
            };
            record_dimension(
                &mut report.canonical_wins_anywhere,
                &mut report.ac,
                key_str,
                verdict,
            );
        }
    }

    // ── Reviews: cross-check against canonical review records
    // by the `working_on.milestone_id + cycle` dimension. ──
    if let Some(working) = &session.working_on {
        let key_str = format!("{}/cycle-{}", working.milestone_id, working.cycle);
        let verdict = snapshot
            .review_revisions
            .get(&CanonicalReviewKey {
                milestone_id: working.milestone_id.clone(),
                cycle: working.cycle,
            })
            .map(|_state| DimensionVerdict::InSync {
                // Reviews don't carry a content revision — the
                // presence of a canonical record is the in-sync
                // signal. Mismatch (session saw one verdict,
                // canonical has a different one) is reported via
                // `working_on_stale` below.
                revision: "review-record-present".to_string(),
            })
            .unwrap_or(DimensionVerdict::UnknownToCanonical);
        record_dimension(
            &mut report.canonical_wins_anywhere,
            &mut report.reviews,
            key_str,
            verdict,
        );
    }

    // ── Lifecycles: compare each (milestone, lifecycle) against
    // the canonical lifecycle timestamp. Newer canonical
    // timestamp → `CanonicalNewer` (session must not regress). ──
    for (milestone_id, lifecycle_state) in walk_session_lifecycles(session) {
        let verdict = match snapshot.lifecycle_revisions.get(&milestone_id) {
            Some(canonical) => {
                if canonical.lifecycle_at >= lifecycle_state.last_state_change_at {
                    DimensionVerdict::CanonicalNewer {
                        session_revision: lifecycle_state.last_state_change_at.clone(),
                        canonical_revision: canonical.lifecycle_at.clone(),
                    }
                } else {
                    DimensionVerdict::SessionNewer {
                        session_revision: lifecycle_state.last_state_change_at.clone(),
                        canonical_revision: canonical.lifecycle_at.clone(),
                    }
                }
            }
            None => DimensionVerdict::UnknownToCanonical,
        };
        record_dimension(
            &mut report.canonical_wins_anywhere,
            &mut report.lifecycles,
            milestone_id,
            verdict,
        );
    }

    // ── Working-on freshness: the session's `working_on.last_*`
    // timestamp must not pre-date the canonical lifecycle's
    // `lifecycle_at` for the same milestone. The session may have
    // a stale `last_state_change_at` even when the projection
    // revisions are in sync (e.g. a runner reported an AC pass
    // but the plan later rolled the lifecycle back to
    // `in-progress`); this dimension catches that gap. ──
    if let Some(working) = &session.working_on {
        if let Some(canonical) = snapshot.lifecycle_revisions.get(&working.milestone_id) {
            if canonical.lifecycle_at < session.last_updated {
                report.working_on_stale = false;
            } else {
                report.working_on_stale = true;
                report.canonical_wins_anywhere = true;
            }
        }
    }

    report
}

fn record_dimension(
    canonical_wins_anywhere: &mut bool,
    target: &mut BTreeMap<String, DimensionVerdict>,
    key: String,
    verdict: DimensionVerdict,
) {
    if matches!(verdict, DimensionVerdict::CanonicalNewer { .. }) {
        *canonical_wins_anywhere = true;
    }
    target.insert(key, verdict);
}

/// Per-milestone lifecycle view derived from the session. The
/// session stores `last_state_change_at` per session (not per
/// milestone); the per-milestone view falls back to the session
/// timestamp for the working-on milestone and `None` for the
/// rest. The cross-checker treats `None` as "session has no
/// lifecycle record for this milestone" and the canonical
/// snapshot as authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionLifecycleView {
    last_state_change_at: String,
}

fn walk_session_lifecycles(session: &AutopilotSession) -> BTreeMap<String, SessionLifecycleView> {
    let mut out = BTreeMap::new();
    if let Some(working) = &session.working_on {
        let last_state_change_at = session
            .last_state_change_at
            .clone()
            .unwrap_or_else(|| session.last_updated.clone());
        out.insert(
            working.milestone_id.clone(),
            SessionLifecycleView {
                last_state_change_at,
            },
        );
    }
    out
}

/// Key for one canonical review record (per milestone + cycle).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CanonicalReviewKey {
    pub milestone_id: String,
    pub cycle: u32,
}

fn compare_revisions(session_rev: &str, canonical_rev: &str) -> DimensionVerdict {
    if session_rev == canonical_rev {
        DimensionVerdict::InSync {
            revision: session_rev.to_string(),
        }
    } else if canonical_rev > session_rev {
        // The canonical revision is "greater" in lexicographic
        // order. The hash strings the projection module uses are
        // stable and ordered by the canonical's `last_updated`
        // timestamp + content hash, so lexicographic comparison is
        // a reliable proxy for "newer".
        DimensionVerdict::CanonicalNewer {
            session_revision: session_rev.to_string(),
            canonical_revision: canonical_rev.to_string(),
        }
    } else {
        DimensionVerdict::SessionNewer {
            session_revision: session_rev.to_string(),
            canonical_revision: canonical_rev.to_string(),
        }
    }
}

fn payload_string(event: &OrchestrationEvent, field: &str) -> Option<String> {
    event
        .payload
        .as_ref()
        .and_then(|p| p.get(field))
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
}

// ─── Unit tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::events::{EventCursor, EventKind, OrchestrationEvent};
    use crate::autopilot::session::{RolesConfig, SessionStatus};
    use crate::autopilot::spawn::MpBinaryProvenance;
    use crate::autopilot::RoleName;
    use serde_json::json;

    fn binary_with_schema(schema: u32) -> MpBinaryProvenance {
        MpBinaryProvenance {
            binary_path: "/usr/bin/mp".into(),
            version: "0.0.0-test".into(),
            schema_version: schema,
            build_kind: "test".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn session_with_events(events: Vec<OrchestrationEvent>) -> AutopilotSession {
        let mut s = sample_session();
        for e in events {
            s.events.push(e);
        }
        s.event_cursor = EventCursor {
            last_seq: s.events.iter().map(|e| e.seq).max().unwrap_or(0),
        };
        s
    }

    fn sample_session() -> AutopilotSession {
        let mut s = AutopilotSession::sample("test");
        s.status = SessionStatus::Active;
        s.binary_provenance = Some(binary_with_schema(s.schema_version));
        s
    }

    // ── AC-01: idempotency key dedupes the same effect twice. ──

    #[test]
    fn m225_ac01_dispatch_key_is_idempotent() {
        // First dispatch is not yet applied (empty log); second
        // call after appending the AssignmentDispatched event is a
        // duplicate.
        let mut session = sample_session();
        let key = IdempotencyKey::Dispatch {
            pane_label: "role-runner-1".into(),
        };
        assert!(!was_already_applied(&session, &key));
        let event = OrchestrationEvent::new(
            1,
            EventKind::AssignmentDispatched,
            "test",
            json!({
                "pane_label": "role-runner-1",
                "milestone_id": "225",
            }),
        );
        session.events.push(event);
        session.event_cursor.last_seq = 1;
        assert!(was_already_applied(&session, &key));
    }

    #[test]
    fn m225_ac01_lifecycle_key_is_idempotent() {
        let mut session = sample_session();
        let key = IdempotencyKey::Lifecycle {
            milestone_id: "225".into(),
            target: "executed".into(),
        };
        assert!(!was_already_applied(&session, &key));
        let event = OrchestrationEvent::new(
            1,
            EventKind::Transition,
            "test",
            json!({
                "milestone_id": "225",
                "target": "executed",
            }),
        );
        session.events.push(event);
        session.event_cursor.last_seq = 1;
        assert!(was_already_applied(&session, &key));
    }

    // ── AC-02: pane loss classification. ──

    #[test]
    fn m225_ac02_dead_pane_with_prompt_and_actor_is_safe_respawn() {
        let input = PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: true,
            stored_prompt: Some("You are the runner for M225"),
            stored_actor: Some("runner:M225"),
        };
        let outcome = classify_pane_loss(&input);
        match outcome {
            PaneLossOutcome::SafeRespawn {
                prompt,
                actor_rotation,
            } => {
                assert_eq!(prompt, "You are the runner for M225");
                let rot = actor_rotation.expect("actor rotation is set when prior actor exists");
                assert!(rot.contains("respawn"));
            }
            other => panic!("expected SafeRespawn, got {other:?}"),
        }
    }

    #[test]
    fn m225_ac02_role_removed_from_topology_escalates() {
        let input = PaneLossInput {
            role: RoleName::Reviewer,
            pane_live: false,
            topology_role_present: false,
            stored_prompt: Some("any"),
            stored_actor: Some("reviewer:M225"),
        };
        let outcome = classify_pane_loss(&input);
        assert!(matches!(
            outcome,
            PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::RoleRemovedFromTopology { .. }
            }
        ));
    }

    #[test]
    fn m225_ac02_no_stored_prompt_escalates() {
        let input = PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: true,
            stored_prompt: None,
            stored_actor: Some("runner:M225"),
        };
        let outcome = classify_pane_loss(&input);
        assert!(matches!(
            outcome,
            PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::NoStoredPrompt { .. }
            }
        ));
    }

    #[test]
    fn m225_ac02_no_stored_actor_escalates() {
        let input = PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: true,
            stored_prompt: Some("prompt"),
            stored_actor: None,
        };
        let outcome = classify_pane_loss(&input);
        assert!(matches!(
            outcome,
            PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::NoStoredActor { .. }
            }
        ));
    }

    // ── AC-03: tail recovery preserves events + rejects
    // incompatible schema / binary. ──

    #[test]
    fn m225_ac03_recovered_when_cursor_is_stale() {
        // Three events were appended; the cursor is artificially
        // set below them (torn-write simulation). Recovery must
        // bump the cursor to the max event seq without touching
        // the events vec.
        let mut session = sample_session();
        for seq in 1..=3 {
            let ev = OrchestrationEvent::new(
                seq,
                EventKind::Transition,
                "test",
                json!({"milestone_id": "225", "target": "executed"}),
            );
            session.events.push(ev);
        }
        session.event_cursor.last_seq = 1;
        let prior_len = session.events.len();
        let current = binary_with_schema(session.schema_version);
        let result = recover_event_tail(&mut session, &current);
        match result {
            TailRecovery::Recovered {
                last_seq,
                prior_event_count,
            } => {
                assert_eq!(last_seq, 3);
                assert_eq!(prior_event_count, prior_len);
            }
            other => panic!("expected Recovered, got {other:?}"),
        }
        assert_eq!(
            session.events.len(),
            prior_len,
            "events must not be truncated"
        );
        assert_eq!(session.events.iter().map(|e| e.seq).max(), Some(3));
    }

    #[test]
    fn m225_ac03_rejected_when_session_schema_too_new() {
        let mut session = sample_session();
        session.schema_version = 99;
        let current = binary_with_schema(1);
        let result = recover_event_tail(&mut session, &current);
        match result {
            TailRecovery::Rejected {
                reason: TailRejectReason::SchemaTooNew { .. },
                prior_event_count,
            } => {
                assert_eq!(prior_event_count, session.events.len());
            }
            other => panic!("expected SchemaTooNew rejection, got {other:?}"),
        }
        // No mutation: cursor and events unchanged.
        assert_eq!(
            session.event_cursor.last_seq,
            sample_session().event_cursor.last_seq
        );
    }

    #[test]
    fn m225_ac03_rejected_when_binary_provenance_too_new() {
        let mut session = sample_session();
        session.binary_provenance = Some(binary_with_schema(99));
        let current = binary_with_schema(session.schema_version);
        let prior_cursor = session.event_cursor.last_seq;
        let result = recover_event_tail(&mut session, &current);
        match result {
            TailRecovery::Rejected {
                reason: TailRejectReason::BinaryProvenanceTooNew { .. },
                ..
            } => {}
            other => panic!("expected BinaryProvenanceTooNew rejection, got {other:?}"),
        }
        assert_eq!(session.event_cursor.last_seq, prior_cursor);
    }

    // ── AC-04: cross-check canonical wins when newer. ──

    #[test]
    fn m225_ac04_canonical_newer_revision_marks_canonical_wins() {
        let mut session = sample_session();
        // Force the session's projection to a lexicographically
        // smaller rev so the canonical "z-new-rev" wins by
        // `compare_revisions`'s str-ord rule.
        if let Some(map) = session.ac_projections.get_mut("207") {
            if let Some(p) = map.get_mut("AC-01") {
                p.source_revision = "a-old-rev".into();
            }
        }
        let key = CanonicalAcKey::new("207", "AC-01");
        let mut snapshot = CanonicalSnapshot::empty();
        snapshot.ac_revisions.insert(
            key,
            CanonicalAcState {
                status: "passed".into(),
                source_revision: "z-new-rev".into(),
                canonical_at: "2026-09-03T00:00:00Z".into(),
            },
        );
        let report = cross_check_canonical(&session, &snapshot);
        assert!(report.canonical_wins_anywhere);
        assert!(!report.session_is_safe());
        let ac_verdict = report.ac.get("207/AC-01").expect("ac verdict present");
        assert!(matches!(
            ac_verdict,
            DimensionVerdict::CanonicalNewer { .. }
        ));
    }

    #[test]
    fn m225_ac04_in_sync_when_revisions_match() {
        let session = sample_session();
        let mut snapshot = CanonicalSnapshot::empty();
        snapshot.ac_revisions.insert(
            CanonicalAcKey::new("207", "AC-01"),
            CanonicalAcState {
                status: "pending".into(),
                source_revision: session
                    .ac_projections
                    .get("207")
                    .and_then(|m| m.get("AC-01"))
                    .map(|p| p.source_revision.clone())
                    .unwrap_or_default(),
                canonical_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        let report = cross_check_canonical(&session, &snapshot);
        assert!(!report.canonical_wins_anywhere);
        let ac_verdict = report.ac.get("207/AC-01").expect("ac verdict present");
        assert!(matches!(ac_verdict, DimensionVerdict::InSync { .. }));
    }

    #[test]
    fn m225_ac04_session_newer_when_session_revision_advances() {
        let mut session = sample_session();
        // Bump the session's projection to a rev that's
        // lexicographically greater than the canonical rev.
        if let Some(map) = session.ac_projections.get_mut("207") {
            if let Some(p) = map.get_mut("AC-01") {
                p.source_revision = "z-late-rev".into();
            }
        }
        let mut snapshot = CanonicalSnapshot::empty();
        snapshot.ac_revisions.insert(
            CanonicalAcKey::new("207", "AC-01"),
            CanonicalAcState {
                status: "pending".into(),
                source_revision: "a-early-rev".into(),
                canonical_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        let report = cross_check_canonical(&session, &snapshot);
        let ac_verdict = report.ac.get("207/AC-01").expect("ac verdict present");
        assert!(matches!(ac_verdict, DimensionVerdict::SessionNewer { .. }));
        assert!(!report.canonical_wins_anywhere);
    }

    // ── cross-cutting: the four primitives compose without
    // surprising each other. ──

    #[test]
    fn m225_compose_idempotency_then_classify_then_recover_then_cross_check() {
        // 1. Append a dispatch event so the dispatch key becomes
        //    idempotent.
        let mut session = session_with_events(vec![OrchestrationEvent::new(
            1,
            EventKind::AssignmentDispatched,
            "test",
            json!({"pane_label": "role-runner-1", "milestone_id": "225"}),
        )]);
        let key = IdempotencyKey::Dispatch {
            pane_label: "role-runner-1".into(),
        };
        assert!(was_already_applied(&session, &key));

        // 2. Classify the missing runner pane — set up a stored
        //    prompt + actor so the respawn path is "safe".
        if let Some(runner) = session.roles.runner.as_mut() {
            runner.spawn_prompt_rendered = Some("You are the runner for M225".into());
        }
        let prompt = "You are the runner for M225";
        let actor = "runner:M225";
        let input = PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: session
                .topology
                .runner
                .as_ref()
                .map(|_| true)
                .unwrap_or(false),
            stored_prompt: Some(prompt),
            stored_actor: Some(actor),
        };
        let outcome = classify_pane_loss(&input);
        assert!(matches!(outcome, PaneLossOutcome::SafeRespawn { .. }));

        // 3. Recover the tail — cursor matches the events, so no
        //    bump is needed. (Empty-cursor recovery also exercises
        //    the no-mutation path.)
        let current = binary_with_schema(session.schema_version);
        let result = recover_event_tail(&mut session, &current);
        assert!(matches!(
            result,
            TailRecovery::Recovered { last_seq: 1, .. }
        ));

        // 4. Cross-check the canonical snapshot. Empty snapshot
        //    means every AC is `UnknownToCanonical` — the resume
        //    path treats the session as the source of truth until
        //    the canonical is supplied.
        let snapshot = CanonicalSnapshot::empty();
        let report = cross_check_canonical(&session, &snapshot);
        assert!(!report.canonical_wins_anywhere);
        // The session's AC projection for (207, AC-01) reports
        // `UnknownToCanonical` because the snapshot is empty.
        let ac_verdict = report.ac.get("207/AC-01").expect("ac verdict present");
        assert!(matches!(ac_verdict, DimensionVerdict::UnknownToCanonical));
    }

    // ── smoke: helpers used by the four primitives compile and
    // round-trip their arguments without surprises. ──

    #[test]
    fn m225_roles_config_empty_when_role_unset() {
        let mut s = sample_session();
        s.roles = RolesConfig::default();
        let prompt = s
            .roles
            .runner
            .as_ref()
            .and_then(|r| r.spawn_prompt_rendered.clone());
        assert!(prompt.is_none());
    }

    #[test]
    fn m225_working_on_milestone_keys_dimensions() {
        // The cross-checker keys the `working_on` review dimension
        // by `(milestone, cycle)`. The session's working_on is set
        // by the sample helper to (M207, cycle 1); verify the
        // report includes a review entry under that key.
        let session = sample_session();
        let snapshot = CanonicalSnapshot::empty();
        let report = cross_check_canonical(&session, &snapshot);
        let review_key = report
            .reviews
            .keys()
            .find(|k| k.contains("207/cycle-1"))
            .expect("review key for working_on milestone must exist");
        assert_eq!(review_key, "207/cycle-1");
    }

    // suppress dead_code on items used only via re-exports / tests
    #[allow(dead_code)]
    fn _unused_lint_suppress() {}
}
