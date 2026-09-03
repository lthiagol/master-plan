//! M207 / AC-01 / AC-02 / AC-07: session.json shape, load/save, atomic writes.
//!
//! This module is the heart of M207:
//!
//! - Defines [`AutopilotSession`] — the typed view over `session.json`.
//! - [`load_session`] reads with the shared bounded-read primitive
//!   (32 MiB cap, nofollow open, project-root containment).
//! - [`save_session`] validates the value against the embedded
//!   schema, then writes atomically (temp file + fsync + rename +
//!   parent dir fsync) via [`crate::store::atomic_write`].
//! - [`SessionPath`] is the typed answer to "where does a session
//!   live on disk?" — every consumer routes through it so the layout
//!   stays in one place.
//!
//! ## Schema authority
//!
//! The JSON schema (`schemas/autopilot-session.schema.json`) is the
//! source of truth; every typed struct here mirrors it. The structs
//! use `serde` defaults that match the schema's required fields so
//! `serde_json::from_value` round-trips a validated document without
//! data loss.
//!
//! ## State authority
//!
//! Milestone criterion status and review records remain canonical
//! (`plan.json` / `reviews.json`). `session.json` only stores
//! *revisioned projections* plus an append-only event journal of
//! orchestration actions. Stale or conflicting writes are rejected
//! rather than silently overwriting the canonical truth — see
//! [`crate::autopilot::ac_projection`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::autopilot::ac_projection::{AcProjection, PerMilestoneProjections};
use crate::autopilot::events::{EventCursor, OrchestrationEvent};
use crate::autopilot::notes::RunnerNote;
use crate::autopilot::schema::{validate_value, SESSION_MAX_BYTES};
use crate::autopilot::transitions::RoleStateRecord;
use crate::json_input;
use crate::paths::PlanContext;
use crate::store;

/// Current session schema version. Bumped on any breaking change.
/// Loaders reject unknown versions with [`SessionLoadError::UnknownSchemaVersion`].
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// Errors raised by [`load_session`].
#[derive(Debug)]
pub enum SessionLoadError {
    /// Schema version in the file is greater than what we know.
    /// Caller may decide to migrate-and-retry or treat as fatal.
    UnknownSchemaVersion { file_version: u32, known_max: u32 },
    /// Bounded-read, JSON parse, or schema validation failed.
    Parse(String),
    /// Refusing to read outside the project root.
    OutsideProjectRoot { path: PathBuf, root: PathBuf },
}

impl std::fmt::Display for SessionLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionLoadError::UnknownSchemaVersion {
                file_version,
                known_max,
            } => write!(
                f,
                "session.json schema_version={file_version} is newer than known max {known_max}; refusing to load"
            ),
            SessionLoadError::Parse(e) => write!(f, "session.json parse error: {e}"),
            SessionLoadError::OutsideProjectRoot { path, root } => write!(
                f,
                "session path {} escapes project root {}",
                path.display(),
                root.display()
            ),
        }
    }
}

impl std::error::Error for SessionLoadError {}

/// Three-pane topology enum. Mirrors `roles.*.role` and
/// `topology.*.pane_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleName {
    Orchestrator,
    Runner,
    Reviewer,
}

impl RoleName {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoleName::Orchestrator => "orchestrator",
            RoleName::Runner => "runner",
            RoleName::Reviewer => "reviewer",
        }
    }
}

impl std::str::FromStr for RoleName {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "orchestrator" => Ok(RoleName::Orchestrator),
            "runner" => Ok(RoleName::Runner),
            "reviewer" => Ok(RoleName::Reviewer),
            other => Err(format!("unknown role {other:?}")),
        }
    }
}

/// Live pane reference. Pane id is mandatory; the label is the
/// human-readable name in the herdr tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaneRef {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Per-role config snapshot. Re-spawn reuses these to keep the same
/// model/harness/skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleConfig {
    pub role: RoleName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
}

/// Per-role topology pane_ids. Mirrors `topology.*` in the schema.
///
/// Renamed from `Topology` to `PaneLayout` in M209 so the
/// [`crate::autopilot::role::Topology`] enum can own the canonical
/// name; the on-disk shape (`topology.orchestrator.pane_id`, etc.)
/// and serde representation are unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaneLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator: Option<PaneRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<PaneRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<PaneRef>,
}

/// Per-role config envelope on the session. Mirrors `roles.*`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RolesConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator: Option<RoleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<RoleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<RoleConfig>,
}

/// Per-role state envelope. Mirrors `role_state.*`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RoleStateEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator: Option<RoleStateRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<RoleStateRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<RoleStateRecord>,
}

/// Free-form driver config. Mirrors `config_overrides.*`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionConfigOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stall_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

/// Per-milestone sub-state in the queue. Mirrors `queue.items`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueItem {
    pub milestone_id: String,
    pub stage: Stage,
    pub cycle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_notify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_refs: Option<EvidenceRefs>,
}

/// Stage values used inside [`QueueItem`] and (indirectly) the
/// session lifecycle. Serialized as kebab-case to match the
/// lifecycle status convention (`in-progress`, `self-reviewed`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Pending,
    InProgress,
    Executed,
    SelfReviewed,
    Reviewed,
    Complete,
}

impl Stage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stage::Pending => "pending",
            Stage::InProgress => "in-progress",
            Stage::Executed => "executed",
            Stage::SelfReviewed => "self-reviewed",
            Stage::Reviewed => "reviewed",
            Stage::Complete => "complete",
        }
    }
}

/// Cross-pointers to canonical sources of truth. Mirrors
/// `queue.items.evidence_refs`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceRefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviews_verdict: Option<String>,
}

/// Session lifecycle. Mirrors `status` and `terminal_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Draft,
    Active,
    Paused,
    Stopped,
    Completed,
    Failed,
}

impl SessionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Draft => "draft",
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused",
            SessionStatus::Stopped => "stopped",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for SessionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(SessionStatus::Draft),
            "active" => Ok(SessionStatus::Active),
            "paused" => Ok(SessionStatus::Paused),
            "stopped" => Ok(SessionStatus::Stopped),
            "completed" => Ok(SessionStatus::Completed),
            "failed" => Ok(SessionStatus::Failed),
            other => Err(format!("unknown session status {other:?}")),
        }
    }
}

/// Pause / resume / force-cancel controls. Mirrors `controls.*`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Controls {
    #[serde(default)]
    pub paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_after: Option<String>,
}

/// In-flight milestone + cycle. Mirrors `working_on` and the
/// `role_state_record.working_on` sub-shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkingOn {
    pub milestone_id: String,
    pub cycle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<RoleName>,
}

/// The session.json typed view. Every field except the required ones
/// is optional so `from_value` round-trips a freshly-minted sample
/// without panicking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutopilotSession {
    pub id: String,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_workspace: Option<String>,
    pub topology: PaneLayout,
    pub roles: RolesConfig,
    #[serde(default)]
    pub config_overrides: SessionConfigOverrides,
    pub queue: Vec<QueueItem>,
    pub status: SessionStatus,
    pub last_updated: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_status: Option<SessionStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state_change_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_state: Option<RoleStateEnvelope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_on: Option<WorkingOn>,
    #[serde(default)]
    pub prompt_bundles: BTreeMap<String, Value>,
    #[serde(default)]
    pub controls: Controls,
    #[serde(default)]
    pub runner_notes: Vec<RunnerNote>,
    #[serde(default)]
    pub events: Vec<OrchestrationEvent>,
    #[serde(default)]
    pub event_cursor: EventCursor,
    #[serde(default)]
    pub ac_projections: BTreeMap<String, PerMilestoneProjections>,
    #[serde(default)]
    pub queue_cycle_history: Vec<CycleHistoryEntry>,
    #[serde(default)]
    pub schema_migrations: Vec<SchemaMigration>,
}

/// One entry in `queue_cycle_history`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CycleHistoryEntry {
    pub milestone_id: String,
    pub cycle: u32,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// One entry in `schema_migrations`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaMigration {
    pub from_version: u32,
    pub to_version: u32,
    pub at: String,
}

/// Resolve the autopilot root for a plan context.
///
/// Returns `<plan_dir>/autopilot`. The directory may not exist yet —
/// callers that need it to exist should call [`std::fs::create_dir_all`].
pub fn autopilot_dir(ctx: &PlanContext) -> PathBuf {
    ctx.plan_dir.join("autopilot")
}

/// Typed answer to "where does a session live?". Every consumer
/// routes through this so the layout stays in one place.
#[derive(Debug, Clone)]
pub struct SessionPath {
    /// `<plan_dir>/autopilot/<id>/session.json`
    pub file: PathBuf,
    /// `<plan_dir>/autopilot/<id>/` (parent of `file`)
    pub dir: PathBuf,
}

impl SessionPath {
    pub fn new(ctx: &PlanContext, session_id: &str) -> Result<Self> {
        crate::paths::assert_safe_path_segment(session_id, "autopilot session")?;
        let dir = autopilot_dir(ctx).join(session_id);
        let file = dir.join("session.json");
        Ok(Self { file, dir })
    }

    pub fn for_file(file: PathBuf) -> Self {
        let dir = file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Self { file, dir }
    }
}

/// Read a session.json from disk.
///
/// The read goes through [`json_input::read_file_bounded`] (32 MiB
/// cap, O_NOFOLLOW open) plus the project-root containment guard
/// from the same module. The parsed value is then validated against
/// the embedded schema — schema errors surface as
/// [`SessionLoadError::Parse`].
///
/// On `schema_version > SESSION_SCHEMA_VERSION`, the file is rejected
/// with [`SessionLoadError::UnknownSchemaVersion`] rather than
/// silently loaded — newer fields may not match the runtime
/// expectations.
pub fn load_session(
    ctx: &PlanContext,
    session_id: &str,
) -> Result<AutopilotSession, SessionLoadError> {
    let path = SessionPath::new(ctx, session_id)
        .map_err(|e| SessionLoadError::Parse(format!("bad session id: {e}")))?;
    load_session_from(&path.file, &ctx.project_root)
}

/// Lower-level loader that bypasses the `SessionPath` id check.
/// Useful for tests and for paths resolved by list helpers.
pub fn load_session_from(
    file: &Path,
    project_root: &Path,
) -> Result<AutopilotSession, SessionLoadError> {
    // Containment: refuse to read outside the project root.
    let root_c = project_root.canonicalize().map_err(|e| {
        SessionLoadError::Parse(format!(
            "canonicalize project root {}: {e}",
            project_root.display()
        ))
    })?;
    let abs = if file.is_absolute() {
        file.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| SessionLoadError::Parse(format!("current_dir: {e}")))?
            .join(file)
    };
    let candidate = match abs.canonicalize() {
        Ok(c) => c,
        Err(e) => {
            // ENOENT etc. propagate as parse-style errors; the
            // caller can distinguish via the error message.
            return Err(SessionLoadError::Parse(format!(
                "canonicalize {}: {e}",
                abs.display()
            )));
        }
    };
    if !candidate.starts_with(&root_c) {
        return Err(SessionLoadError::OutsideProjectRoot {
            path: candidate,
            root: root_c,
        });
    }

    // Bounded read (32 MiB cap, O_NOFOLLOW open via json_input).
    let raw = json_input::read_file_bounded(&candidate, SESSION_MAX_BYTES).map_err(|e| {
        SessionLoadError::Parse(format!("read session.json {}: {e}", candidate.display()))
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| {
        SessionLoadError::Parse(format!("parse session.json {}: {e}", candidate.display()))
    })?;
    let session: AutopilotSession = serde_json::from_value(value.clone()).map_err(|e| {
        SessionLoadError::Parse(format!("decode session.json {}: {e}", candidate.display()))
    })?;
    if session.schema_version > SESSION_SCHEMA_VERSION {
        return Err(SessionLoadError::UnknownSchemaVersion {
            file_version: session.schema_version,
            known_max: SESSION_SCHEMA_VERSION,
        });
    }
    let errs = validate_value(&value).map_err(|e| SessionLoadError::Parse(format!("{e}")))?;
    if !errs.is_empty() {
        return Err(SessionLoadError::Parse(format!(
            "session.json failed schema validation: {}",
            errs.join("; ")
        )));
    }
    Ok(session)
}

/// Validate `value` against the embedded schema. Convenience wrapper
/// used by [`save_session`] and exposed to the autopilot CLI for
/// pre-flight checks.
pub fn validate_session_value(value: &Value) -> Result<Vec<String>> {
    validate_value(value)
}

/// Atomically write the session to disk. The flow is:
///
/// 1. Serialize the typed value via `serde_json::to_value_pretty`.
/// 2. Validate against the embedded schema — fail loudly before
///    touching the disk.
/// 3. `atomic_write` via [`crate::store::atomic_write`] (temp file +
///    fsync + rename + parent dir fsync).
pub fn save_session(
    ctx: &PlanContext,
    session_id: &str,
    session: &AutopilotSession,
) -> Result<PathBuf> {
    let path = SessionPath::new(ctx, session_id)?;
    save_session_at(&path.file, session)
}

/// Lower-level writer used by tests and by callers that already hold
/// a path. The path's parent directory is created.
pub fn save_session_at(file: &Path, session: &AutopilotSession) -> Result<PathBuf> {
    // Stamp last_updated on every save — `serde_json::to_value` would
    // not pick up mutations made after the struct was constructed.
    let mut snapshot = session.clone();
    snapshot.last_updated = store::now_rfc3339();

    let value: Value = serde_json::to_value(&snapshot)
        .with_context(|| format!("serialize session.json for {}", file.display()))?;
    let errs = validate_value(&value)
        .with_context(|| format!("validate session.json for {}", file.display()))?;
    if !errs.is_empty() {
        bail!(
            "session.json failed schema validation; refusing to write {}: {}",
            file.display(),
            errs.join("; ")
        );
    }
    let pretty = serde_json::to_vec_pretty(&value)
        .with_context(|| format!("re-serialize pretty session.json for {}", file.display()))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    store::atomic_write(file, pretty)
        .with_context(|| format!("atomic write session.json {}", file.display()))?;
    Ok(file.to_path_buf())
}

/// Build a fully-populated sample session suitable for round-trip
/// tests. Uses the shape the spec calls out: 3-pane topology, 2
/// milestones in queue, role config snapshots, and evidence_refs
/// pointing at lifecycle / execution_status / spec_status /
/// reviews.json verdict. Returns a session that *validates* against
/// the embedded schema, so callers can immediately save+load without
/// further setup.
pub fn sample_session_for_tests(id: &str) -> AutopilotSession {
    let queue = vec![
        QueueItem {
            milestone_id: "207".to_string(),
            stage: Stage::InProgress,
            cycle: 1,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: Some(EvidenceRefs {
                lifecycle: Some("in-progress".into()),
                execution_status: Some("in-progress".into()),
                spec_status: Some("ready".into()),
                reviews_verdict: None,
            }),
        },
        QueueItem {
            milestone_id: "209".to_string(),
            stage: Stage::Pending,
            cycle: 1,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: Some(EvidenceRefs {
                lifecycle: Some("approved".into()),
                execution_status: Some("planned".into()),
                spec_status: Some("ready".into()),
                reviews_verdict: None,
            }),
        },
    ];

    let topology = PaneLayout {
        orchestrator: Some(PaneRef {
            pane_id: "%1".into(),
            label: Some("role-orchestrator-1".into()),
        }),
        runner: Some(PaneRef {
            pane_id: "%2".into(),
            label: Some("role-runner-1".into()),
        }),
        reviewer: Some(PaneRef {
            pane_id: "%3".into(),
            label: Some("role-reviewer-1".into()),
        }),
    };

    let roles = RolesConfig {
        orchestrator: Some(RoleConfig {
            role: RoleName::Orchestrator,
            pane_id: Some("%1".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-coordinator".into()),
            config_hash: Some("orch-coord-v1".into()),
        }),
        runner: Some(RoleConfig {
            role: RoleName::Runner,
            pane_id: Some("%2".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-runner".into()),
            config_hash: Some("runner-v1".into()),
        }),
        reviewer: Some(RoleConfig {
            role: RoleName::Reviewer,
            pane_id: Some("%3".into()),
            model: Some("anthropic/claude-opus-4-1".into()),
            harness: Some("opencode".into()),
            skill: Some("mp-runner".into()),
            config_hash: Some("reviewer-v1".into()),
        }),
    };

    let mut ac_projections = BTreeMap::new();
    let m207 = PerMilestoneProjections::from_iter([(
        "AC-01".to_string(),
        AcProjection {
            ac_id: "AC-01".into(),
            status: crate::autopilot::AcStatus::Pending,
            evidence: None,
            source_revision: "m207-rev-1".into(),
            projected_at: Some("2026-01-01T00:00:00Z".into()),
        },
    )]);
    ac_projections.insert("207".to_string(), m207);

    let now = "2026-01-01T00:00:00Z".to_string();
    AutopilotSession {
        id: id.to_string(),
        schema_version: SESSION_SCHEMA_VERSION,
        herdr_workspace: Some(format!("{id}-autopilot")),
        topology,
        roles,
        config_overrides: SessionConfigOverrides::default(),
        queue,
        status: SessionStatus::Active,
        last_updated: now.clone(),
        started_at: Some(now.clone()),
        terminal_status: None,
        last_state_change_at: Some(now.clone()),
        role_state: Some(RoleStateEnvelope {
            orchestrator: Some(RoleStateRecord {
                role: RoleName::Orchestrator,
                state: crate::autopilot::RoleState::Idle,
                since: Some(now.clone()),
                actor: Some("seed".into()),
                working_on: None,
            }),
            runner: Some(RoleStateRecord {
                role: RoleName::Runner,
                state: crate::autopilot::RoleState::Working,
                since: Some(now.clone()),
                actor: Some("seed".into()),
                working_on: Some(WorkingOn {
                    milestone_id: "207".into(),
                    cycle: 1,
                    role: Some(RoleName::Runner),
                }),
            }),
            reviewer: Some(RoleStateRecord {
                role: RoleName::Reviewer,
                state: crate::autopilot::RoleState::Idle,
                since: Some(now),
                actor: Some("seed".into()),
                working_on: None,
            }),
        }),
        working_on: Some(WorkingOn {
            milestone_id: "207".into(),
            cycle: 1,
            role: Some(RoleName::Runner),
        }),
        prompt_bundles: BTreeMap::new(),
        controls: Controls::default(),
        runner_notes: Vec::new(),
        events: Vec::new(),
        event_cursor: EventCursor::new(),
        ac_projections,
        queue_cycle_history: Vec::new(),
        schema_migrations: Vec::new(),
    }
}

// Convenience: sample() method on AutopilotSession so tests don't
// have to import the helper by name.
impl AutopilotSession {
    /// Fully-populated sample (3-pane topology, 2 milestones in
    /// queue, role config snapshots, evidence_refs, working_on set,
    /// role_state populated). The canonical "happy-path" sample.
    pub fn sample(id: &str) -> Self {
        sample_session_for_tests(id)
    }

    /// Minimal blank session with the required schema fields and
    /// nothing else populated. Tests that need to drive role
    /// transitions, AC projections, or note derivation from a clean
    /// state should start here.
    pub fn blank(id: &str) -> Self {
        let now = "2026-01-01T00:00:00Z".to_string();
        Self {
            id: id.to_string(),
            schema_version: SESSION_SCHEMA_VERSION,
            herdr_workspace: None,
            topology: PaneLayout::default(),
            roles: RolesConfig::default(),
            config_overrides: SessionConfigOverrides::default(),
            queue: Vec::new(),
            status: SessionStatus::Draft,
            last_updated: now.clone(),
            started_at: Some(now),
            terminal_status: None,
            last_state_change_at: None,
            role_state: None,
            working_on: None,
            prompt_bundles: BTreeMap::new(),
            controls: Controls::default(),
            runner_notes: Vec::new(),
            events: Vec::new(),
            event_cursor: EventCursor::new(),
            ac_projections: BTreeMap::new(),
            queue_cycle_history: Vec::new(),
            schema_migrations: Vec::new(),
        }
    }
}

/// Append a single event to the session and write atomically. Helper
/// used by the autopilot CLI; also the test entry point for AC-07.
pub fn append_event(
    ctx: &PlanContext,
    session_id: &str,
    event: OrchestrationEvent,
) -> Result<PathBuf> {
    let path = SessionPath::new(ctx, session_id)?;
    let mut session =
        load_session_from(&path.file, &ctx.project_root).map_err(|e| anyhow::anyhow!("{e}"))?;
    session.event_cursor.advance_to(event.seq)?;
    session.events.push(event);
    save_session_at(&path.file, &session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    #[test]
    fn session_path_resolves_under_plan_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let p = SessionPath::new(&ctx, "alpha").unwrap();
        assert!(p.file.ends_with("master-plan/autopilot/alpha/session.json"));
    }

    #[test]
    fn session_path_rejects_path_traversal_id() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        assert!(SessionPath::new(&ctx, "../etc").is_err());
        assert!(SessionPath::new(&ctx, "a/b").is_err());
        assert!(SessionPath::new(&ctx, "").is_err());
    }

    #[test]
    fn autopilot_dir_sits_under_plan_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let dir = autopilot_dir(&ctx);
        assert!(dir.ends_with("master-plan/autopilot"));
    }

    #[test]
    fn save_then_load_round_trips_losslessly() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let session = sample_session_for_tests("alpha");
        save_session(&ctx, "alpha", &session).unwrap();
        let loaded = load_session(&ctx, "alpha").unwrap();
        // last_updated is auto-stamped on save; everything else
        // must round-trip exactly.
        let mut expected = session;
        expected.last_updated = loaded.last_updated.clone();
        assert_eq!(loaded, expected);
    }

    #[test]
    fn load_session_rejects_unknown_schema_version() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let path = SessionPath::new(&ctx, "alpha").unwrap();
        std::fs::create_dir_all(&path.dir).unwrap();
        let raw = r#"{
            "id": "alpha",
            "schema_version": 999,
            "topology": {},
            "roles": {},
            "queue": [],
            "status": "draft",
            "last_updated": "2026-01-01T00:00:00Z"
        }"#;
        std::fs::write(&path.file, raw).unwrap();
        let err = load_session(&ctx, "alpha").unwrap_err();
        match err {
            SessionLoadError::UnknownSchemaVersion {
                file_version,
                known_max,
            } => {
                assert_eq!(file_version, 999);
                assert_eq!(known_max, SESSION_SCHEMA_VERSION);
            }
            other => panic!("expected UnknownSchemaVersion, got {other:?}"),
        }
    }

    #[test]
    fn load_session_rejects_outside_project_root() {
        // Two disjoint temp dirs; reading from one with the other as
        // project_root must surface OutsideProjectRoot.
        let project = TempDir::new().unwrap();
        let foreign = TempDir::new().unwrap();
        let foreign_dir = foreign.path().join("autopilot/foreign/session.json");
        std::fs::create_dir_all(foreign_dir.parent().unwrap()).unwrap();
        std::fs::write(&foreign_dir, r#"{"id":"x","schema_version":1,"topology":{},"roles":{},"queue":[],"status":"draft","last_updated":"t"}"#).unwrap();
        let err = load_session_from(&foreign_dir, project.path()).unwrap_err();
        assert!(matches!(err, SessionLoadError::OutsideProjectRoot { .. }));
    }

    #[test]
    fn save_session_rejects_schema_invalid_value() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        // Typed struct permits any `String`; the schema requires
        // `^[a-z0-9][a-z0-9-]*$`. Setting an upper-case id
        // therefore fails validation *after* the typed struct is
        // built — the right place to test the schema gate.
        let mut bad = sample_session_for_tests("alpha");
        bad.id = "Alpha-Capital".to_string();
        let err = save_session(&ctx, "alpha", &bad).unwrap_err();
        assert!(err.to_string().contains("schema validation"), "got {err}");
        // Nothing was written.
        let p = SessionPath::new(&ctx, "alpha").unwrap();
        assert!(!p.file.exists());
    }

    #[test]
    fn save_session_creates_parent_directory() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let s = sample_session_for_tests("alpha");
        save_session(&ctx, "alpha", &s).unwrap();
        let p = SessionPath::new(&ctx, "alpha").unwrap();
        assert!(p.dir.is_dir());
        assert!(p.file.is_file());
    }

    #[test]
    fn sample_session_validates_against_schema() {
        let s = sample_session_for_tests("alpha");
        let value = serde_json::to_value(&s).unwrap();
        let errs = validate_value(&value).unwrap();
        assert!(errs.is_empty(), "sample failed validation: {errs:?}");
    }

    #[test]
    fn role_name_round_trips_via_serde() {
        for r in [RoleName::Orchestrator, RoleName::Runner, RoleName::Reviewer] {
            let s = serde_json::to_string(&r).unwrap();
            let back: RoleName = serde_json::from_str(&s).unwrap();
            assert_eq!(back, r);
            assert_eq!(r.as_str(), s.trim_matches('"'));
            let parsed: RoleName = r.as_str().parse().unwrap();
            assert_eq!(parsed, r);
        }
    }

    #[test]
    fn session_status_round_trips_via_serde() {
        for s in [
            SessionStatus::Draft,
            SessionStatus::Active,
            SessionStatus::Paused,
            SessionStatus::Stopped,
            SessionStatus::Completed,
            SessionStatus::Failed,
        ] {
            let raw = serde_json::to_string(&s).unwrap();
            let back: SessionStatus = serde_json::from_str(&raw).unwrap();
            assert_eq!(back, s);
            let parsed: SessionStatus = s.as_str().parse().unwrap();
            assert_eq!(parsed, s);
        }
    }

    #[test]
    fn append_event_bumps_cursor_and_persists() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let s = sample_session_for_tests("alpha");
        save_session(&ctx, "alpha", &s).unwrap();
        let event = OrchestrationEvent::new(
            1,
            crate::autopilot::EventKind::Dispatch,
            "test",
            serde_json::json!({"stage": "execute"}),
        );
        append_event(&ctx, "alpha", event.clone()).unwrap();
        let loaded = load_session(&ctx, "alpha").unwrap();
        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].seq, 1);
        assert_eq!(loaded.event_cursor.last_seq, 1);
    }
}
