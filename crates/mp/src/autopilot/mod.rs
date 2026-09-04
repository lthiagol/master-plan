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
//! - [`task_assign`] — M211's typed task-assignment renderer for
//!   orchestrator-to-runner and orchestrator-to-reviewer dispatch.
//!   Produces a deterministic herdr argv, validates the payload
//!   against the session's pane layout before any spawn, and
//!   appends an `AssignmentDispatched` event after the spawn
//!   outcome is known.
//! - [`verifier`] — M212's independent state-reads + role-boundary
//!   violation detection. Cross-checks milestone JSON,
//!   `reviews.json`, and `activity.json` for every lane notification
//!   and surfaces typed violations (7 named detectors +
//!   lifecycle-claim unbacked, unknown actor, evidence contract,
//!   command-list). Topology-aware remediation lives in
//!   [`recommend_remediation`].
//! - [`drive`] — the herdr drive engine: preconditions, pane
//!   spawn/wait, the stage-done bridge, lifecycle-stage prompts, the
//!   driver loop, the cross-milestone sequencer, the JSONL run log,
//!   and the crash-safe run/resume state. This tree was named
//!   `mp::watch` before the autopilot cutover; it is now the
//!   canonical `mp::autopilot::drive` engine and the `mp watch` CLI
//!   verb is a thin compatibility adapter over it
//!   (`commands::watch`), retained until the alias is removed.
//! - [`review_env`] — M224's reviewer execution isolation and
//!   clean-room policy. Records reviewer provenance (binary /
//!   worktree / target dir / pid / actor identity), selects the
//!   mode ([`ReviewEnvMode::Normal`] default vs [`ReviewEnvMode::
//!   CleanRoom`] explicit-or-provenance-forced), and gates the
//!   pre-review pass on a typed refusal for dirty worktree / shared
//!   actor / stale binary / unverifiable environment.
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
pub mod commit_policy;
pub mod cycle;
pub mod drive;
pub mod events;
pub mod gate;
pub mod lifecycle;
pub mod list;
pub mod notes;
pub mod prompts;
pub mod reconcile;
pub mod recovery;
pub mod review_env;
pub mod role;
pub mod schema;
pub mod session;
pub mod spawn;
pub mod task_assign;
pub mod transitions;
pub mod verifier;

pub use ac_projection::{
    canonical_revision, project_ac_status, AcProjection, AcStatus, PerMilestoneProjections,
    ProjectionKey, ProjectionRevision, ProjectionWriteOutcome,
};
pub use commit_policy::{
    classify_subject, lifecycle_metadata_overwrites_evidence, validate_fixed_in, CommitIndex,
    CommitInspection, CommitKind, PolicyError,
};
pub use cycle::{
    apply_decision_matrix, build_reviewer_activation, classify_liveness,
    cycle_state_machine_to_json, predict_next_action, reviewer_mode_for_cycle, start_cycle,
    working_on_for, CycleDecision, CycleEscalateReason, CycleEvent, CycleStartError, CycleState,
    CycleStateMachine, CycleStats, CycleVerdictRecord, DecisionInput, FindingSummary,
    HeartbeatAckState, HeartbeatTracker, LivenessStatus, NextAction, ReviewRequestPayload,
    ReviewerMode, ReviewerVerdict, REVIEWER_SOFT_CAP_CYCLE,
};
pub use events::{events_by_kind, EventCursor, EventKind, OrchestrationEvent};
pub use gate::{
    check_autopilot_herdr_gate, check_autopilot_herdr_gate_default, AutopilotGateError, GateReason,
    EX_AUTOPILOT_GATE, HERDR_INSTALL_HINT,
};
pub use lifecycle::{
    validate_evidence_shape as validate_lifecycle_evidence, Clock, ClockT, ClosureJournal,
    ClosureOutcome, CommitAttestation, JournalEntry, LifecycleClosure, LifecycleTransition,
    MilestoneSnapshot, NullAttestation, TransitionKind,
    TransitionOutcome as LifecycleTransitionOutcome, TransitionRejectReason,
    LIFECYCLE_TRANSITION_ORDER,
};
pub use list::{list_sessions, SessionListEntry};
pub use notes::{build_note, derive_cycle, NoteError, NoteKind, RunnerNote};
pub use reconcile::{
    classify_pane_loss, cross_check_canonical, last_durable_seq, recover_event_tail,
    was_already_applied, CanonicalAcKey, CanonicalAcState, CanonicalLifecycleState,
    CanonicalReviewKey, CanonicalReviewState, CanonicalSnapshot, CrossCheckReport,
    DimensionVerdict, IdempotencyKey, PaneLossInput, PaneLossOutcome, PaneLossReason, TailRecovery,
    TailRejectReason,
};
pub use recovery::{
    append_event_unchecked, list_session_ids, reconcile_event_cursor, recover_session,
    recover_session_at, run_startup_recovery, run_startup_recovery_all, RecoveredSession,
    StartupRecoveryOutcome, StartupRecoveryReport,
};
pub use review_env::{
    build_provenance, clean_room_commands, gate, provenance_issues, select_mode, ActorIdentity,
    CleanRoomTrigger, GateInputs, ModeSelection, ReviewEnvConfig, ReviewEnvDecision,
    ReviewEnvError, ReviewEnvMode, ReviewerProvenance,
};
pub use role::{
    builtin_role_default, pane_index_for, resolve_role_config, resolve_role_config_full,
    resolve_role_config_with_provenance, resolve_with_legacy_fallback, role_pane_slots, tighten,
    topology_policy, topology_preflight, MilestoneKind, PaneSlots, ResolvedRoleConfig,
    ResolvedRoleConfigWithProvenance, ReviewBypassPolicy, Role, RoleConfigOverride,
    RoleConfigSource, RoleResolutionError, RoleSlot, Topology, TopologyMode, TopologyPolicy,
    TopologyPreflightError,
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
pub use task_assign::{
    build_assignment_argv, dispatch_assignment, execute_assignment, parse_assignment,
    render_task_text, validate_assignment, validate_assignment_structure, validate_pane_membership,
    AssignmentOutcome, RoleDirection, TaskAssignment, TaskAssignmentValidationError,
};
pub use transitions::{
    is_valid as is_valid_transition, transition as apply_transition, RoleState, RoleStateRecord,
    TransitionError, TransitionOutcome,
};
pub use verifier::{
    check_command_list, check_evidence_contract, check_evidence_not_overwritten,
    check_notification, cross_check_state, detect_orchestrator_code_edit_violation,
    detect_pre_start_notification_violation, detect_reviewer_code_edit_violation,
    detect_reviewer_premature_pass_violation, detect_runner_claim_violation,
    detect_runner_plan_edit_violation, detect_runner_review_violation, git_log_for_path,
    recommend_remediation, validate_evidence_shape, violations_to_json, ActorAttribution,
    AttributionError, CrossCheckMismatch, EvidenceContractViolation, EvidenceShapeError, Lane,
    LaneNotification, LifecycleClaimUnbacked, List as ViolationList, OrchestratorCodeEditViolation,
    PreStartNotificationViolation, Remediation, ReviewerCodeEditViolation,
    ReviewerPrematurePassViolation, RunnerClaimViolation, RunnerPlanEditViolation,
    RunnerReviewViolation, UnknownActorViolation, UnsupportedCommandOperator, Verdict,
    VerificationCommand, VerifierInputs, VerifierState, Violation,
};
