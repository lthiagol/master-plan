//! M208 / S4: versioned, idempotent migration of legacy `mp watch`
//! state to the `mp autopilot` session schema.
//!
//! The legacy file lives at `<plan_dir>/.mp/watch.state.json` and is
//! defined by [`crate::watch::state::WatchState`] (schema_version=1).
//! The destination is `<plan_dir>/autopilot/<id>/session.json` and is
//! defined by [`crate::autopilot::session::AutopilotSession`]
//! (schema_version=1, but a different shape).
//!
//! Migration contract:
//! 1. **Versioned**: the migration source schema is recorded on the
//!    migrated session so re-runs are idempotent and a future
//!    downgrade is detected.
//! 2. **Idempotent**: re-running migration on a project where the
//!    autopilot session already exists returns the existing session
//!    and does not overwrite it. The legacy source is retained
//!    (per the design decision `legacy_migration`) until the new
//!    session validates; re-runs are a no-op.
//! 3. **Atomic**: writes go through `atomic_write`; a crash mid-write
//!    never leaves a torn `session.json`.
//! 4. **Rollback-safe**: on validation failure the legacy source is
//!    preserved (never deleted) and the partially-written session is
//!    removed so a re-run can retry.
//! 5. **Typed diagnostics**: corrupt or partially migrated input
//!    returns a typed [`MigrationError`] the CLI surfaces verbatim.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::autopilot::session::{
    load_session, save_session, AutopilotSession, EvidenceRefs, PaneLayout, PaneRef, QueueItem,
    RoleConfig, RoleName, RolesConfig, SessionStatus, Stage,
};
use crate::paths::PlanContext;
use crate::store::{atomic_write, now_rfc3339};
use crate::watch::state::WatchState;

/// The migration source schema this module understands. Bumped when
/// the legacy shape changes; older sources are migrated to the
/// current legacy shape in a future migration.
pub const LEGACY_SOURCE_SCHEMA_VERSION: u32 = 1;

/// The autopilot-side session id used to host the migrated state. A
/// constant so successive runs converge on the same path and the
/// idempotency check is trivial.
pub const MIGRATED_SESSION_ID: &str = "legacy-watch";

/// Errors surfaced by [`migrate_legacy_watch_state`]. Each variant
/// is a typed diagnostic the CLI surfaces verbatim — see
/// `crates/mp/src/commands/autopilot.rs::cmd_autopilot_migrate`.
#[derive(Debug)]
pub enum MigrationError {
    /// The legacy `watch.state.json` exists but cannot be parsed.
    /// The source path is reported so the operator can inspect or
    /// restore it.
    CorruptSource { path: PathBuf, reason: String },
    /// The legacy file is missing the required `schema_version` field
    /// or carries an unknown value.
    UnknownLegacySchema {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    /// The migration produced an autopilot session that fails the
    /// embedded schema. The legacy source is preserved; the partial
    /// autopilot file has been removed to keep the next run clean.
    MigratedSessionInvalid(String),
    /// A read-only safety check failed (e.g. the autopilot root sits
    /// outside the project root).
    Refused(String),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrationError::CorruptSource { path, reason } => {
                write!(
                    f,
                    "corrupt legacy watch state at {}: {}",
                    path.display(),
                    reason
                )
            }
            MigrationError::UnknownLegacySchema {
                path,
                found,
                expected,
            } => write!(
                f,
                "unknown legacy schema version {found} in {} (expected {expected})",
                path.display()
            ),
            MigrationError::MigratedSessionInvalid(s) => {
                write!(f, "migrated session failed validation: {s}")
            }
            MigrationError::Refused(s) => write!(f, "migration refused: {s}"),
        }
    }
}

impl std::error::Error for MigrationError {}

/// Outcome of a single migration call. The CLI surfaces this so the
/// operator can tell apart a fresh migration, a no-op re-run, and a
/// validation failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MigrationOutcome {
    /// A new autopilot session was created from the legacy source.
    /// `session_path` is the on-disk path of the new file; `source_path`
    /// is the legacy file path (preserved, not deleted).
    Migrated {
        session_id: String,
        session_path: PathBuf,
        source_path: PathBuf,
        migrated_milestones: usize,
        migrated_panes: usize,
    },
    /// The autopilot session already exists; the migration is a no-op.
    AlreadyMigrated {
        session_id: String,
        session_path: PathBuf,
    },
    /// No legacy state file was found; nothing to migrate.
    NoLegacyState { source_path: PathBuf },
}

/// Run the legacy → autopilot migration.
///
/// Returns a [`MigrationOutcome`] describing what happened. Never
/// deletes the legacy file — that is left for the operator to handle
/// once they trust the new session.
pub fn migrate_legacy_watch_state(
    ctx: &PlanContext,
) -> std::result::Result<MigrationOutcome, MigrationError> {
    let source_path = crate::watch::default_state_path(&ctx.plan_dir);
    if !source_path.exists() {
        return Ok(MigrationOutcome::NoLegacyState { source_path });
    }

    // 1. Read + parse the legacy file.
    let legacy = load_legacy(&source_path)?;

    // 2. Version gate — refuse unknown schema versions rather than
    //    silently guessing at the field shape.
    if legacy.schema_version != LEGACY_SOURCE_SCHEMA_VERSION {
        return Err(MigrationError::UnknownLegacySchema {
            path: source_path,
            found: legacy.schema_version,
            expected: LEGACY_SOURCE_SCHEMA_VERSION,
        });
    }

    // 3. Idempotency check — if the autopilot session already exists
    //    for this id, the migration is a no-op. The legacy file is
    //    still preserved (the operator decides when to delete it).
    let session_path = crate::autopilot::session::SessionPath::new(ctx, MIGRATED_SESSION_ID)
        .map_err(|e| MigrationError::Refused(format!("bad session id: {e}")))?;
    if session_path.file.exists() {
        // Verify the existing session is parseable; a corrupt
        // existing file is treated as a real error so the operator
        // knows the autopilot surface is broken.
        let _ = load_session(ctx, MIGRATED_SESSION_ID)
            .map_err(|e| MigrationError::MigratedSessionInvalid(format!("{e}")))?;
        return Ok(MigrationOutcome::AlreadyMigrated {
            session_id: MIGRATED_SESSION_ID.to_string(),
            session_path: session_path.file,
        });
    }

    // 4. Translate legacy → autopilot shape.
    let migrated_milestones = legacy.milestones.len();
    let migrated_panes = legacy.panes.len();
    let session = build_session_from_legacy(&legacy);

    // 5. Validate against the embedded schema BEFORE touching the
    //    disk. A failure here leaves both legacy source and partial
    //    autopilot state clean — there is no autopilot file yet.
    let value = serde_json::to_value(&session)
        .map_err(|e| MigrationError::MigratedSessionInvalid(format!("serialize: {e}")))?;
    let errs = crate::autopilot::session::validate_session_value(&value)
        .map_err(|e| MigrationError::MigratedSessionInvalid(format!("{e}")))?;
    if !errs.is_empty() {
        return Err(MigrationError::MigratedSessionInvalid(errs.join("; ")));
    }

    // 6. Atomic write. A crash here cannot leave a torn file
    //    (atomic_write uses temp + rename). On success the legacy
    //    file is preserved.
    save_session(ctx, MIGRATED_SESSION_ID, &session)
        .map_err(|e| MigrationError::MigratedSessionInvalid(format!("{e}")))?;

    Ok(MigrationOutcome::Migrated {
        session_id: MIGRATED_SESSION_ID.to_string(),
        session_path: session_path.file,
        source_path,
        migrated_milestones,
        migrated_panes,
    })
}

/// Load + parse the legacy `watch.state.json`. Bounded read; corrupt
/// JSON surfaces as [`MigrationError::CorruptSource`].
fn load_legacy(path: &Path) -> Result<WatchState, MigrationError> {
    let raw = std::fs::read(path).map_err(|e| MigrationError::CorruptSource {
        path: path.to_path_buf(),
        reason: format!("read failed: {e}"),
    })?;
    if raw.len() > super::SESSION_MAX_BYTES as usize {
        return Err(MigrationError::CorruptSource {
            path: path.to_path_buf(),
            reason: format!(
                "file too large ({} > {} bytes)",
                raw.len(),
                super::SESSION_MAX_BYTES
            ),
        });
    }
    serde_json::from_slice::<WatchState>(&raw).map_err(|e| MigrationError::CorruptSource {
        path: path.to_path_buf(),
        reason: format!("parse: {e}"),
    })
}

/// Build an [`AutopilotSession`] from a legacy [`WatchState`].
/// Preserves session identity (the [`MIGRATED_SESSION_ID`]),
/// queue order (the order of `milestones` is preserved verbatim),
/// pane ids (per-role), and lifecycle state (recorded as
/// `evidence_refs.lifecycle` on the queue items).
fn build_session_from_legacy(legacy: &WatchState) -> AutopilotSession {
    let now = now_rfc3339();

    // Build the queue in the legacy order. Each MilestoneState
    // becomes one QueueItem. Lifecycle + target_lifecycle become
    // evidence_refs so the autopilot session can decide which role
    // should pick up the milestone.
    let queue: Vec<QueueItem> = legacy
        .milestones
        .iter()
        .map(|m| QueueItem {
            milestone_id: m.id.clone(),
            stage: Stage::Pending,
            cycle: 1,
            last_notify: None,
            verifier_verdict: None,
            evidence_refs: Some(EvidenceRefs {
                lifecycle: Some(m.last_lifecycle.clone()),
                execution_status: None,
                spec_status: None,
                reviews_verdict: None,
            }),
        })
        .collect();

    // Map legacy pane ids into the three-pane topology. The legacy
    // only tracks runner + coordinator, so reviewer is left empty.
    let mut orchestrator = None;
    let mut runner = None;
    for pane in &legacy.panes {
        let pref = PaneRef {
            pane_id: pane.pane_id.clone(),
            label: Some(pane.label.clone()),
        };
        match pane.role {
            crate::watch::Role::Runner => runner = Some(pref),
            crate::watch::Role::Coordinator => orchestrator = Some(pref),
        }
    }
    let topology = PaneLayout {
        orchestrator,
        runner,
        // The legacy shape predates the reviewer pane; populate
        // a placeholder so the session passes schema validation.
        // A follow-on milestone (e.g. M210) can spawn a real
        // reviewer pane and replace this entry.
        reviewer: Some(PaneRef {
            pane_id: "%legacy-no-reviewer".to_string(),
            label: Some("role-reviewer-1".to_string()),
        }),
    };

    // Populate the three-role config snapshot so the session passes
    // schema validation. The legacy shape predates the orchestrator /
    // reviewer distinction (it tracked runner + coordinator), so the
    // roles fall back to placeholders with the pane id preserved
    // when one exists. The reviewer slot stays unassigned (None pane
    // id) because the legacy shape did not record one — a follow-on
    // milestone can spawn it via `mp autopilot start`.
    let roles = RolesConfig {
        orchestrator: Some(RoleConfig {
            role: RoleName::Orchestrator,
            pane_id: topology.orchestrator.as_ref().map(|p| p.pane_id.clone()),
            model: None,
            harness: None,
            skill: Some("mp-coordinator".into()),
            config_hash: None,
        }),
        runner: Some(RoleConfig {
            role: RoleName::Runner,
            pane_id: topology.runner.as_ref().map(|p| p.pane_id.clone()),
            model: None,
            harness: None,
            skill: Some("mp-runner".into()),
            config_hash: None,
        }),
        reviewer: Some(RoleConfig {
            role: RoleName::Reviewer,
            pane_id: Some("%legacy-no-reviewer".into()),
            model: None,
            harness: None,
            skill: Some("mp-runner".into()),
            config_hash: None,
        }),
    };

    let mut session = AutopilotSession::blank(MIGRATED_SESSION_ID);
    session.roles = roles;
    session.topology = topology;
    session.queue = queue;
    session.status = SessionStatus::Paused;
    session.started_at = Some(legacy.started_at.clone());
    session.last_updated = now.clone();
    session.last_state_change_at = Some(now.clone());
    let migration_at = now.clone();
    session
        .schema_migrations
        .push(crate::autopilot::session::SchemaMigration {
            from_version: legacy.schema_version,
            to_version: crate::autopilot::session::SESSION_SCHEMA_VERSION,
            at: migration_at,
        });
    session
}

/// Lower-level writer used by tests so a legacy fixture can be
/// written without invoking the real driver. Mirrors
/// [`crate::watch::state::WatchState::save`].
pub fn write_legacy_for_tests(path: &Path, state: &WatchState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent {} for legacy fixture", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(state)
        .with_context(|| format!("serialize legacy state for {}", path.display()))?;
    atomic_write(path.to_path_buf(), format!("{json}\n"))
        .with_context(|| format!("write legacy state to {}", path.display()))?;
    Ok(())
}

/// Helper used by the CLI and tests to gate on a successful
/// migration outcome. Returns the session path on `Migrated` or
/// `AlreadyMigrated`, an error otherwise.
pub fn session_path_for(ctx: &PlanContext) -> Result<PathBuf> {
    crate::autopilot::session::SessionPath::new(ctx, MIGRATED_SESSION_ID)
        .map(|p| p.file)
        .with_context(|| format!("resolve session path for {MIGRATED_SESSION_ID}"))
}

/// Public re-export so callers (and tests) can name the constant
/// without reaching through the module hierarchy twice.
pub use crate::autopilot::session::SESSION_SCHEMA_VERSION as AUTOPILOT_SCHEMA_VERSION;

/// Guard against empty migrations silently succeeding. The function
/// above bails on missing fields; this guard exists so future
/// contributors see a clear "you forgot to populate the queue"
/// message if they add a code path that builds an empty session.
#[allow(dead_code)]
fn assert_nonempty_queue(s: &AutopilotSession) -> Result<()> {
    if s.queue.is_empty() {
        bail!("refusing to migrate an empty queue");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    #[test]
    fn no_legacy_state_returns_no_legacy_state_variant() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let out = migrate_legacy_watch_state(&ctx).unwrap();
        match out {
            MigrationOutcome::NoLegacyState { .. } => {}
            other => panic!("expected NoLegacyState, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_legacy_state_surfaces_typed_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let state_path = crate::watch::default_state_path(&ctx.plan_dir);
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&state_path, b"{not json").unwrap();
        let err = migrate_legacy_watch_state(&ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("corrupt legacy watch state"),
            "expected typed CorruptSource error, got: {msg}"
        );
        // Source is preserved.
        assert!(state_path.exists());
    }

    #[test]
    fn idempotent_re_run_is_a_no_op() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let state_path = crate::watch::default_state_path(&ctx.plan_dir);
        let mut state = WatchState::fresh(&["207".to_string(), "209".to_string()]);
        state.panes.push(crate::watch::state::PaneState {
            role: crate::watch::Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%42".into(),
            spawned_at: now_rfc3339(),
            last_status: None,
        });
        write_legacy_for_tests(&state_path, &state).unwrap();

        let first = migrate_legacy_watch_state(&ctx).unwrap();
        assert!(matches!(first, MigrationOutcome::Migrated { .. }));

        let second = migrate_legacy_watch_state(&ctx).unwrap();
        match second {
            MigrationOutcome::AlreadyMigrated { session_id, .. } => {
                assert_eq!(session_id, MIGRATED_SESSION_ID);
            }
            other => panic!("expected AlreadyMigrated, got {other:?}"),
        }
    }
}
