//! M207: durable per-session state for `mp autopilot` drives.
//!
//! This module owns the schema and on-disk I/O for `session.json`, the
//! single canonical record of an autopilot drive. Each session lives
//! at `<plan_dir>/autopilot/<id>/session.json` — self-contained so it
//! can be archived, diffed, and recovered in isolation.
//!
//! ## Submodule layout
//!
//! - [`schema`] — embedded JSON-schema loader + bounded validation.
//! - [`session`] — top-level session.json struct, load/save, atomic
//!   writes, project-root containment, migration entry point.
//! - [`transitions`] — typed role-state machine (idle/working/blocked
//!   …) and the transition table that gates every mutation.
//! - [`notes`] — typed runner notes (kind/body/cycle/timestamp) and
//!   the cycle-derivation rule for `mp autopilot note`.
//! - [`ac_projection`] — revisioned AC projection synchronized from
//!   canonical milestone criterion state; rejects stale writes.
//! - [`events`] — append-only event types and the sequence-numbered
//!   cursor that survives a crash mid-write.
//!
//! The on-disk shape is validated against
//! `schemas/autopilot-session.schema.json`; that file is the source of
//! truth — every Rust struct here is a typed view over it.
//!
//! ## State authority
//!
//! Milestone criterion status and review records remain canonical
//! (`plan.json` / `reviews.json`). `session.json` only stores
//! *revisioned projections* plus an append-only event journal of
//! orchestration actions. Stale or conflicting writes are rejected
//! rather than silently overwriting the canonical truth — see
//! [`ac_projection`].

pub mod ac_projection;
pub mod events;
pub mod list;
pub mod notes;
pub mod schema;
pub mod session;
pub mod transitions;

pub use ac_projection::{
    AcProjection, AcStatus, PerMilestoneProjections, ProjectionKey, ProjectionRevision,
    ProjectionWriteOutcome, canonical_revision, project_ac_status,
};
pub use events::{EventCursor, EventKind, OrchestrationEvent, events_by_kind};
pub use list::{SessionListEntry, list_sessions};
pub use notes::{
    NoteError, NoteKind, RunnerNote, build_note, derive_cycle,
};
pub use schema::{SESSION_MAX_BYTES, validate_session_value};
#[allow(unused_imports)]
use schema::validate_value as _;
pub use session::{
    AutopilotSession, Controls, CycleHistoryEntry, EvidenceRefs, PaneRef, QueueItem, RoleConfig,
    RoleName, RoleStateEnvelope, RolesConfig, SchemaMigration, SessionConfigOverrides,
    SessionLoadError, SessionPath, SessionStatus, Stage, Topology, WorkingOn, append_event,
    autopilot_dir, load_session, load_session_from, sample_session_for_tests, save_session,
    save_session_at, SESSION_SCHEMA_VERSION,
};
pub use transitions::{
    RoleState, RoleStateRecord, TransitionError, TransitionOutcome, is_valid as is_valid_transition,
    transition as apply_transition,
};