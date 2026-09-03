//! M207 / M209: durable per-session state for `mp autopilot` drives
//! and the canonical role/topology model.
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
//! - [`role`] — M209's canonical three-role model and the topology
//!   enum + pane-slot mapping function. The session struct holds a
//!   [`session::PaneLayout`] (the per-role pane assignments) which
//!   is a derived view over [`Topology`] + a pane-id map.
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
pub mod recovery;
pub mod role;
pub mod schema;
pub mod session;
pub mod transitions;

pub use ac_projection::{
    canonical_revision, project_ac_status, AcProjection, AcStatus, PerMilestoneProjections,
    ProjectionKey, ProjectionRevision, ProjectionWriteOutcome,
};
pub use events::{events_by_kind, EventCursor, EventKind, OrchestrationEvent};
pub use list::{list_sessions, SessionListEntry};
pub use notes::{build_note, derive_cycle, NoteError, NoteKind, RunnerNote};
pub use recovery::{
    append_event_unchecked, reconcile_event_cursor, recover_session, recover_session_at,
    RecoveredSession,
};
pub use role::{
    builtin_role_default, pane_index_for, resolve_role_config, resolve_role_config_full,
    resolve_role_config_with_provenance, role_pane_slots, PaneSlots, ResolvedRoleConfig,
    ResolvedRoleConfigWithProvenance, Role, RoleConfigOverride, RoleConfigSource, RoleSlot,
    Topology,
};
#[allow(unused_imports)]
use schema::validate_value as _;
pub use schema::{validate_session_value, SESSION_MAX_BYTES};
pub use session::{
    append_event, autopilot_dir, load_session, load_session_from, sample_session_for_tests,
    save_session, save_session_at, AutopilotSession, Controls, CycleHistoryEntry, EvidenceRefs,
    PaneLayout, PaneRef, QueueItem, RoleConfig, RoleName, RoleStateEnvelope, RolesConfig,
    SchemaMigration, SessionConfigOverrides, SessionLoadError, SessionPath, SessionStatus, Stage,
    WorkingOn, SESSION_SCHEMA_VERSION,
};
pub use transitions::{
    is_valid as is_valid_transition, transition as apply_transition, RoleState, RoleStateRecord,
    TransitionError, TransitionOutcome,
};
