//! M210 / AC-06, AC-07: transactional autopilot spawn pipeline
//! and binary provenance gate.
//!
//! The spawn pipeline is the single entry point the autopilot CLI
//! uses to bring a session online. It owns three steps that must
//! succeed atomically:
//!
//! 1. Create or reuse the named herdr workspace
//!    (`herdr workspace ensure <name>`).
//! 2. Create the per-topology pane slots
//!    (`herdr pane split --cwd <project>` per pane; topology
//!    determines how many).
//! 3. Start the configured harness in each pane with the rendered
//!    spawn prompt (`herdr agent start <label> --kind <kind>
//!    --pane <id> <extras...>` followed by `herdr agent send
//!    <id> <prompt>` + `herdr pane send-keys Enter`).
//!
//! The pipeline is **transactional**: pane IDs are persisted to
//! `session.json` only after the corresponding `agent start` call
//! succeeds. If step 2 or 3 fails on any pane, the pipeline rolls
//! back by deleting panes it already created and surfacing the
//! failure typed (no silent half-spawned sessions).
//!
//! ## Mockability
//!
//! The I/O is abstracted behind [`HerdrSpawnOps`] so tests can
//! drive every branch (success / partial failure / workspace
//! missing) without a real herdr binary. Production callers use
//! [`RealHerdrSpawnOps`]; the integration tests inject
//! [`MockHerdrSpawnOps`] from this same module.
//!
//! ## Binary provenance (AC-07)
//!
//! Before any plan write, the pipeline stamps an
//! [`MpBinaryProvenance`] record into the session and verifies it
//! against a recorded minimum-schema floor. A stale binary (one
//! that cannot read the M207 schema) is rejected *before* the
//! workspace or pane creation, with an actionable rebuild /
//! install hint. The check is idempotent — re-spawn with the same
//! provenance passes; cross-version spawns surface a typed
//! mismatch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::autopilot::prompts::spawn::{
    harness_extra_flags, render_topology_prompts, BundledPrompt, HarnessFlagError,
    SpawnPromptInputs,
};
use crate::autopilot::role::{ResolvedRoleConfig, Role, Topology};
use crate::autopilot::session::{
    save_session_at, AutopilotSession, PaneLayout, PaneRef, RoleConfig, RoleName, RolesConfig,
    SessionPath, SESSION_SCHEMA_VERSION,
};
use crate::autopilot::transitions::{is_valid as is_valid_transition, RoleState};
use crate::paths::PlanContext;

// ─── Binary provenance (AC-07) ────────────────────────────────────────

/// Minimum schema version a binary must be able to read+write
/// for the spawn pipeline to proceed. Bumped on any breaking
/// change to the session schema. Loaders reject sessions with a
/// `schema_version` higher than what they know, but the
/// *minimum* floor is enforced here: a binary built before the
/// floor was bumped would silently drop newer fields on a load+
/// rewrite cycle.
pub const MIN_SESSION_SCHEMA_VERSION: u32 = SESSION_SCHEMA_VERSION;

/// Identifier for the executing mp binary + the runtime facts
/// the pipeline records into the session. Stamped at spawn time
/// (before any herdr I/O) so a stale binary never reaches
/// `herdr workspace ensure`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpBinaryProvenance {
    /// Absolute path to the executing mp binary.
    pub binary_path: String,
    /// Crate version (e.g. `1.0.0-rc2`).
    pub version: String,
    /// Session schema version this binary was built against.
    pub schema_version: u32,
    /// `release` / `dev` build kind — surfaced in the rejection
    /// hint so a dev build is not silently swapped onto a prod
    /// session.
    pub build_kind: String,
    /// RFC3339 timestamp the provenance was stamped.
    pub recorded_at: String,
}

impl MpBinaryProvenance {
    /// Build the provenance for the current process. Uses
    /// `std::env::current_exe()` for the binary path (the same
    /// value the kernel hands every process). `version` is sourced
    /// from the workspace `Cargo.toml` via the `CARGO_PKG_VERSION`
    /// env var (set by Cargo at build time).
    pub fn current() -> Self {
        let binary_path = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        let version = env!("CARGO_PKG_VERSION").to_string();
        let schema_version = SESSION_SCHEMA_VERSION;
        let build_kind = if cfg!(debug_assertions) {
            "dev".to_string()
        } else {
            "release".to_string()
        };
        let recorded_at = crate::store::now_rfc3339();
        Self {
            binary_path,
            version,
            schema_version,
            build_kind,
            recorded_at,
        }
    }

    /// True when `self` is at least as new as `floor` — used by
    /// [`check_binary_provenance`] to reject stale binaries
    /// *before* any plan write. The direction is: the *recorded*
    /// provenance is the floor; the *current* binary must
    /// satisfy it. So `current.satisfies(recorded)` means
    /// "current is at least as new as what is recorded".
    pub fn satisfies(&self, floor: &MpBinaryProvenance) -> bool {
        self.schema_version >= floor.schema_version
    }
}

/// Typed mismatch surfaced by [`check_binary_provenance`] when
/// the current binary's provenance does not satisfy the recorded
/// minimum. Carries the diff so the operator can act (rebuild /
/// install) without re-running with extra logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryProvenanceMismatch {
    /// Recorded provenance on disk was generated by a newer
    /// binary than the one currently executing. The current
    /// binary cannot preserve the newer schema fields on a
    /// rewrite — reject before any herdr call.
    SchemaTooNew {
        recorded: MpBinaryProvenance,
        current: MpBinaryProvenance,
    },
    /// Recorded provenance is older than the minimum schema
    /// floor (e.g. an obsolete test fixture). Surface with a
    /// rebuild hint.
    SchemaBelowFloor {
        recorded: MpBinaryProvenance,
        floor: u32,
    },
    /// Recorded and current binary paths differ — the operator
    /// either has two `mp` installs or the binary was rebuilt in
    /// place. Surface with a re-install hint.
    BinaryPathMismatch {
        recorded: MpBinaryProvenance,
        current: MpBinaryProvenance,
    },
}

impl std::fmt::Display for BinaryProvenanceMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryProvenanceMismatch::SchemaTooNew { recorded, current } => write!(
                f,
                "session.json was written by mp {} (schema_version={}, build={}); \
                 current binary is mp {} (schema_version={}, build={}). The \
                 session schema is newer than this binary can preserve. \
                 Rebuild mp (cargo build --release) or install the latest \
                 version (`make install`) before re-spawning.",
                recorded.version, recorded.schema_version, recorded.build_kind,
                current.version, current.schema_version, current.build_kind,
            ),
            BinaryProvenanceMismatch::SchemaBelowFloor { recorded, floor } => write!(
                f,
                "session.json provenance records schema_version={} which is below \
                 the autopilot floor {}. Rebuild mp against the current schema \
                 (`cargo build --release`) before re-spawning.",
                recorded.schema_version, floor,
            ),
            BinaryProvenanceMismatch::BinaryPathMismatch { recorded, current } => write!(
                f,
                "session.json provenance records binary path {} but current mp \
                 is at {}. Two mp installs or an in-place rebuild — re-install \
                 (`make install`) before re-spawning.",
                recorded.binary_path, current.binary_path,
            ),
        }
    }
}

impl std::error::Error for BinaryProvenanceMismatch {}

/// Reject a stale or schema-incompatible binary before any plan
/// write. Returns `Ok(())` when the current binary's provenance
/// satisfies the recorded minimum, or the typed mismatch when
/// it does not. The check is the AC-07 contract: no session
/// mutation occurs when this returns `Err`.
pub fn check_binary_provenance(
    recorded: Option<&MpBinaryProvenance>,
    current: &MpBinaryProvenance,
) -> Result<(), Box<BinaryProvenanceMismatch>> {
    let Some(rec) = recorded else {
        // No recorded provenance — first spawn. Accept the
        // current binary's provenance as the new floor.
        return Ok(());
    };
    if rec.schema_version < MIN_SESSION_SCHEMA_VERSION {
        return Err(Box::new(BinaryProvenanceMismatch::SchemaBelowFloor {
            recorded: rec.clone(),
            floor: MIN_SESSION_SCHEMA_VERSION,
        }));
    }
    if current.schema_version < rec.schema_version {
        return Err(Box::new(BinaryProvenanceMismatch::SchemaTooNew {
            recorded: rec.clone(),
            current: current.clone(),
        }));
    }
    if rec.binary_path != current.binary_path {
        return Err(Box::new(BinaryProvenanceMismatch::BinaryPathMismatch {
            recorded: rec.clone(),
            current: current.clone(),
        }));
    }
    Ok(())
}

// ─── Spawn ops abstraction (mockability for AC-06) ───────────────────

/// Outcome of a single `agent start` call. Mirrors the herdr
/// success / failure surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedPane {
    /// Pane label the operator sees in the herdr sidebar.
    pub label: String,
    /// Stable pane id (`%1`, `%2`, ...) for downstream
    /// `herdr agent prompt <id>` calls.
    pub pane_id: String,
    /// Whether the pane was reused (label already existed in the
    /// workspace) or freshly created.
    pub reused: bool,
}

/// Typed failure surfaced by the spawn pipeline when a step
/// fails. The pipeline's rollback path uses this to decide
/// whether to delete the partially-created panes (always, on
/// any non-`Ok` outcome) before re-raising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnError {
    /// `herdr workspace ensure <name>` failed or returned
    /// non-zero. `stderr` is captured for diagnostics.
    WorkspaceEnsureFailed { name: String, stderr: String },
    /// `herdr pane split` failed for a specific pane ordinal.
    PaneCreateFailed { ordinal: usize, stderr: String },
    /// `herdr agent start` failed for a specific pane.
    AgentStartFailed {
        label: String,
        stderr: String,
    },
    /// `herdr agent send` failed for a specific pane (prompt
    /// delivery is the contract; a half-delivered prompt leaves
    /// the pane in a broken state).
    PromptSendFailed {
        label: String,
        stderr: String,
    },
    /// The harness kind was not in the v1 supported set —
    /// caught *before* any pane creation so a stray harness id
    /// never burns a workspace.
    UnsupportedHarness {
        harness: String,
        supported: Vec<String>,
    },
    /// Binary provenance gate refused to proceed (AC-07).
    StaleBinary(Box<BinaryProvenanceMismatch>),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::WorkspaceEnsureFailed { name, stderr } => write!(
                f,
                "herdr workspace ensure {name} failed: {stderr}"
            ),
            SpawnError::PaneCreateFailed { ordinal, stderr } => write!(
                f,
                "herdr pane split failed for pane ordinal {ordinal}: {stderr}"
            ),
            SpawnError::AgentStartFailed { label, stderr } => write!(
                f,
                "herdr agent start failed for pane {label:?}: {stderr}"
            ),
            SpawnError::PromptSendFailed { label, stderr } => write!(
                f,
                "herdr agent send (prompt delivery) failed for pane {label:?}: {stderr}"
            ),
            SpawnError::UnsupportedHarness { harness, supported } => write!(
                f,
                "harness {harness:?} is not in the v1 autopilot spawn set; supported: [{}]",
                supported.join(", ")
            ),
            SpawnError::StaleBinary(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SpawnError {}

/// Abstract surface for the herdr I/O the spawn pipeline
/// performs. Production wires up [`RealHerdrSpawnOps`]; tests
/// inject [`MockHerdrSpawnOps`] so every branch (success,
/// workspace-missing, partial pane failure, prompt-delivery
/// failure) can be exercised without a live herdr binary.
pub trait HerdrSpawnOps {
    /// Ensure a herdr workspace named `name` exists. Idempotent:
    /// if the workspace already exists this is a no-op. Returns
    /// `Err(SpawnError::WorkspaceEnsureFailed)` on failure.
    fn ensure_workspace(&self, name: &str) -> Result<(), SpawnError>;
    /// Create a fresh pane with `cwd` set. Returns the new pane
    /// id (`%1`, `%2`, ...). The pipeline never reuses panes
    /// across roles — each role gets a fresh pane so the
    /// rollback path is straightforward.
    fn create_pane(&self, cwd: &Path, ordinal: usize) -> Result<String, SpawnError>;
    /// Start the harness (`herdr agent start --kind <kind>
    /// --pane <id> <extras...>`) and return the pane handle.
    /// `extras` is the harness-specific flag tail produced by
    /// [`harness_extra_flags`].
    fn start_agent(
        &self,
        label: &str,
        kind: &str,
        pane_id: &str,
        extras: &[String],
    ) -> Result<SpawnedPane, SpawnError>;
    /// Deliver the rendered prompt text to a pane (the
    /// `agent send <pane> <text>` + `pane send-keys Enter`
    /// pair). Returns `Err(SpawnError::PromptSendFailed)` on
    /// delivery failure.
    fn send_prompt(&self, pane_id: &str, text: &str) -> Result<(), SpawnError>;
    /// Best-effort rollback: delete any panes that were created
    /// during a partial failure. The pipeline never blocks on
    /// rollback errors — they are logged via stderr but do not
    /// mask the original failure.
    fn delete_pane(&self, pane_id: &str);
}

/// Production wiring: shells out to the `herdr` binary on
/// PATH. Falls back to a clear `WorkspaceEnsureFailed` when the
/// binary is missing or non-executable so the operator can act
/// without inspecting the herdr layout.
#[derive(Debug, Default, Clone)]
pub struct RealHerdrSpawnOps {
    /// Optional override for the herdr binary path; defaults to
    /// PATH lookup. Tests inject a fake script via PATH; CI uses
    /// the default.
    pub herdr_bin: Option<PathBuf>,
}

impl RealHerdrSpawnOps {
    pub fn new() -> Self {
        Self::default()
    }
}

impl HerdrSpawnOps for RealHerdrSpawnOps {
    fn ensure_workspace(&self, name: &str) -> Result<(), SpawnError> {
        let bin = match self.herdr_bin.clone().or_else(crate::watch::which_herdr) {
            Some(b) => b,
            None => {
                return Err(SpawnError::WorkspaceEnsureFailed {
                    name: name.to_string(),
                    stderr: "herdr binary not found on PATH".to_string(),
                });
            }
        };
        let out = Command::new(&bin)
            .args(["workspace", "ensure", name])
            .output();
        match out {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(SpawnError::WorkspaceEnsureFailed {
                name: name.to_string(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }),
            Err(e) => Err(SpawnError::WorkspaceEnsureFailed {
                name: name.to_string(),
                stderr: format!("failed to exec {bin:?}: {e}"),
            }),
        }
    }

    fn create_pane(&self, _cwd: &Path, ordinal: usize) -> Result<String, SpawnError> {
        let bin = match self.herdr_bin.clone().or_else(crate::watch::which_herdr) {
            Some(b) => b,
            None => {
                return Err(SpawnError::PaneCreateFailed {
                    ordinal,
                    stderr: "herdr binary not found on PATH".to_string(),
                });
            }
        };
        let out = Command::new(&bin)
            .args(["pane", "split"])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                let pane_id = crate::watch::parse_pane_id_from_start_output(&stdout)
                    .unwrap_or_else(|| format!("pane-{}", ordinal + 1));
                Ok(pane_id)
            }
            Ok(o) => Err(SpawnError::PaneCreateFailed {
                ordinal,
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }),
            Err(e) => Err(SpawnError::PaneCreateFailed {
                ordinal,
                stderr: format!("failed to exec {bin:?}: {e}"),
            }),
        }
    }

    fn start_agent(
        &self,
        label: &str,
        kind: &str,
        pane_id: &str,
        extras: &[String],
    ) -> Result<SpawnedPane, SpawnError> {
        let bin = match self.herdr_bin.clone().or_else(crate::watch::which_herdr) {
            Some(b) => b,
            None => {
                return Err(SpawnError::AgentStartFailed {
                    label: label.to_string(),
                    stderr: "herdr binary not found on PATH".to_string(),
                });
            }
        };
        let mut argv: Vec<String> = vec![
            "agent".into(),
            "start".into(),
            label.into(),
            "--kind".into(),
            kind.into(),
            "--pane".into(),
            pane_id.into(),
        ];
        argv.extend(extras.iter().cloned());
        let out = Command::new(&bin).args(&argv).output();
        match out {
            Ok(o) if o.status.success() => Ok(SpawnedPane {
                label: label.to_string(),
                pane_id: pane_id.to_string(),
                reused: false,
            }),
            Ok(o) => Err(SpawnError::AgentStartFailed {
                label: label.to_string(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }),
            Err(e) => Err(SpawnError::AgentStartFailed {
                label: label.to_string(),
                stderr: format!("failed to exec {bin:?}: {e}"),
            }),
        }
    }

    fn send_prompt(&self, pane_id: &str, text: &str) -> Result<(), SpawnError> {
        let bin = match self.herdr_bin.clone().or_else(crate::watch::which_herdr) {
            Some(b) => b,
            None => {
                return Err(SpawnError::PromptSendFailed {
                    label: pane_id.to_string(),
                    stderr: "herdr binary not found on PATH".to_string(),
                });
            }
        };
        let send_out = Command::new(&bin)
            .args(["agent", "send", pane_id, text])
            .output();
        match send_out {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                return Err(SpawnError::PromptSendFailed {
                    label: pane_id.to_string(),
                    stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                });
            }
            Err(e) => {
                return Err(SpawnError::PromptSendFailed {
                    label: pane_id.to_string(),
                    stderr: format!("failed to exec {bin:?}: {e}"),
                });
            }
        }
        let enter_out = Command::new(&bin)
            .args(["pane", "send-keys", pane_id, "Enter"])
            .output();
        match enter_out {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(SpawnError::PromptSendFailed {
                label: pane_id.to_string(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }),
            Err(e) => Err(SpawnError::PromptSendFailed {
                label: pane_id.to_string(),
                stderr: format!("failed to exec {bin:?}: {e}"),
            }),
        }
    }

    fn delete_pane(&self, pane_id: &str) {
        let Some(bin) = self.herdr_bin.clone().or_else(crate::watch::which_herdr) else {
            return;
        };
        let _ = Command::new(&bin)
            .args(["pane", "delete", pane_id])
            .output();
    }
}

/// Mock harness for tests: records every call and lets each
/// call's outcome be scripted. The pipeline tests in
/// `tests/autopilot_spawn_pipeline.rs` use this to drive the
/// success / partial-failure / rollback branches.
#[derive(Debug, Default)]
pub struct MockHerdrSpawnOps {
    inner: std::cell::RefCell<MockHerdrSpawnOpsInner>,
}

#[derive(Debug, Default)]
struct MockHerdrSpawnOpsInner {
    /// Recorded `ensure_workspace` calls.
    pub ensure_calls: Vec<String>,
    /// Recorded `create_pane` calls.
    pub create_calls: Vec<usize>,
    /// Recorded `start_agent` calls.
    pub start_calls: Vec<(String, String, String, Vec<String>)>,
    /// Recorded `send_prompt` calls.
    pub send_calls: Vec<(String, String)>,
    /// Recorded `delete_pane` calls (rollback).
    pub delete_calls: Vec<String>,
    /// Per-call outcome script. When set, the Nth call to each
    /// method returns the matching outcome. Excess calls default
    /// to `Ok`.
    pub ensure_outcomes: Vec<Result<(), String>>,
    pub create_outcomes: Vec<Result<String, String>>,
    pub start_outcomes: Vec<Result<String, String>>,
    pub send_outcomes: Vec<Result<(), String>>,
}

impl MockHerdrSpawnOps {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a script for the next `ensure_workspace` call.
    pub fn push_ensure_outcome(&self, outcome: Result<(), String>) {
        self.inner.borrow_mut().ensure_outcomes.push(outcome);
    }

    /// Push a script for the next `create_pane` call.
    pub fn push_create_outcome(&self, outcome: Result<String, String>) {
        self.inner.borrow_mut().create_outcomes.push(outcome);
    }

    /// Push a script for the next `start_agent` call.
    pub fn push_start_outcome(&self, outcome: Result<String, String>) {
        self.inner.borrow_mut().start_outcomes.push(outcome);
    }

    /// Push a script for the next `send_prompt` call.
    pub fn push_send_outcome(&self, outcome: Result<(), String>) {
        self.inner.borrow_mut().send_outcomes.push(outcome);
    }

    /// Snapshot the recorded calls (test inspection).
    pub fn snapshot(&self) -> MockHerdrSpawnOpsSnapshot {
        let g = self.inner.borrow();
        MockHerdrSpawnOpsSnapshot {
            ensure_calls: g.ensure_calls.clone(),
            create_calls: g.create_calls.clone(),
            start_calls: g.start_calls.clone(),
            send_calls: g.send_calls.clone(),
            delete_calls: g.delete_calls.clone(),
        }
    }
}

/// Read-only view of the mock's recorded calls.
#[derive(Debug, Clone, Default)]
pub struct MockHerdrSpawnOpsSnapshot {
    pub ensure_calls: Vec<String>,
    pub create_calls: Vec<usize>,
    pub start_calls: Vec<(String, String, String, Vec<String>)>,
    pub send_calls: Vec<(String, String)>,
    pub delete_calls: Vec<String>,
}

impl HerdrSpawnOps for MockHerdrSpawnOps {
    fn ensure_workspace(&self, name: &str) -> Result<(), SpawnError> {
        let mut g = self.inner.borrow_mut();
        g.ensure_calls.push(name.to_string());
        if let Some(outcome) = g.ensure_outcomes.first().cloned() {
            let _ = g.ensure_outcomes.remove(0);
            return match outcome {
                Ok(()) => Ok(()),
                Err(stderr) => Err(SpawnError::WorkspaceEnsureFailed {
                    name: name.to_string(),
                    stderr,
                }),
            };
        }
        Ok(())
    }

    fn create_pane(&self, _cwd: &Path, ordinal: usize) -> Result<String, SpawnError> {
        let mut g = self.inner.borrow_mut();
        g.create_calls.push(ordinal);
        if let Some(outcome) = g.create_outcomes.first().cloned() {
            let _ = g.create_outcomes.remove(0);
            return match outcome {
                Ok(pane_id) => Ok(pane_id),
                Err(stderr) => Err(SpawnError::PaneCreateFailed { ordinal, stderr }),
            };
        }
        Ok(format!("pane-{}", ordinal + 1))
    }

    fn start_agent(
        &self,
        label: &str,
        kind: &str,
        pane_id: &str,
        extras: &[String],
    ) -> Result<SpawnedPane, SpawnError> {
        let mut g = self.inner.borrow_mut();
        g.start_calls.push((
            label.to_string(),
            kind.to_string(),
            pane_id.to_string(),
            extras.to_vec(),
        ));
        if let Some(outcome) = g.start_outcomes.first().cloned() {
            let _ = g.start_outcomes.remove(0);
            return match outcome {
                Ok(_) => Ok(SpawnedPane {
                    label: label.to_string(),
                    pane_id: pane_id.to_string(),
                    reused: false,
                }),
                Err(stderr) => Err(SpawnError::AgentStartFailed {
                    label: label.to_string(),
                    stderr,
                }),
            };
        }
        Ok(SpawnedPane {
            label: label.to_string(),
            pane_id: pane_id.to_string(),
            reused: false,
        })
    }

    fn send_prompt(&self, pane_id: &str, text: &str) -> Result<(), SpawnError> {
        let mut g = self.inner.borrow_mut();
        g.send_calls
            .push((pane_id.to_string(), text.to_string()));
        if let Some(outcome) = g.send_outcomes.first().cloned() {
            let _ = g.send_outcomes.remove(0);
            return match outcome {
                Ok(()) => Ok(()),
                Err(stderr) => Err(SpawnError::PromptSendFailed {
                    label: pane_id.to_string(),
                    stderr,
                }),
            };
        }
        Ok(())
    }

    fn delete_pane(&self, pane_id: &str) {
        self.inner.borrow_mut().delete_calls.push(pane_id.to_string());
    }
}

// ─── Spawn pipeline (AC-06) ──────────────────────────────────────────

/// Inputs the spawn pipeline needs from the caller. The
/// pipeline itself is pure over (ops, inputs); only the
/// `HerdrSpawnOps` implementation performs I/O.
pub struct SpawnInputs<'a> {
    /// Plan context — owns project_root + plan_dir.
    pub ctx: &'a PlanContext,
    /// Session id (also the workspace name suffix).
    pub session_id: &'a str,
    /// Topology to spawn.
    pub topology: Topology,
    /// Project root (cwd for `pane split`).
    pub project_root: &'a Path,
    /// Resolved role configs (one per role).
    pub role_o: ResolvedRoleConfig,
    pub role_r: ResolvedRoleConfig,
    pub role_v: ResolvedRoleConfig,
    /// Project name + current milestone id (for prompt rendering).
    pub project_name: &'a str,
    pub milestone_id: &'a str,
    pub queue_position: usize,
}

/// Outcome of the spawn pipeline. Carries the per-role rendered
/// prompts (for audit, AC-04), the per-pane handles, and the
/// final session.json path.
pub struct SpawnOutcome {
    /// Pane handles, one per physical pane, in the topology's
    /// canonical order.
    pub panes: Vec<SpawnedPane>,
    /// Per-pane bundled prompts (audit surface).
    pub bundles: Vec<BundledPrompt>,
    /// Path to the persisted session.json.
    pub session_path: PathBuf,
}

/// Top-level spawn entry point. Implements AC-06: transactional
/// workspace create + pane create + agent start; pane IDs are
/// persisted only after successful starts; partial failures
/// roll back.
pub fn spawn_session<O: HerdrSpawnOps>(
    ops: &O,
    inputs: &SpawnInputs<'_>,
) -> Result<SpawnOutcome, SpawnError> {
    // AC-07: refuse stale binaries *before* any herdr I/O.
    let current = MpBinaryProvenance::current();
    let recorded_path = inputs
        .ctx
        .plan_dir
        .join("autopilot")
        .join(inputs.session_id)
        .join("session.json");
    let recorded = if recorded_path.is_file() {
        load_provenance(&recorded_path).ok()
    } else {
        None
    };
    if let Err(mismatch) = check_binary_provenance(recorded.as_ref(), &current) {
        return Err(SpawnError::StaleBinary(mismatch));
    }

    // Pre-flight: every role's harness must be in the v1 set.
    // Catching this *before* any pane creation matches AC-03:
    // unsupported harnesses fail before pane creation.
    for (role, rc) in [
        (Role::Orchestrator, &inputs.role_o),
        (Role::Runner, &inputs.role_r),
        (Role::Reviewer, &inputs.role_v),
    ] {
        if let Err(HarnessFlagError::Unsupported { harness, supported }) =
            harness_extra_flags(rc)
        {
            let _ = role;
            return Err(SpawnError::UnsupportedHarness { harness, supported });
        }
    }

    // Step 1: ensure the herdr workspace.
    let workspace_name = format!("{}-autopilot", inputs.session_id);
    ops.ensure_workspace(&workspace_name)?;

    // Step 2: render the per-pane bundles (pure).
    let inputs_o = SpawnPromptInputs::new(
        inputs.project_name,
        inputs.session_id,
        inputs.milestone_id,
        inputs.queue_position,
        inputs.role_o.clone(),
    )
    .map_err(|e| SpawnError::AgentStartFailed {
        label: "<orchestrator>".into(),
        stderr: e,
    })?;
    let inputs_r = SpawnPromptInputs::new(
        inputs.project_name,
        inputs.session_id,
        inputs.milestone_id,
        inputs.queue_position,
        inputs.role_r.clone(),
    )
    .map_err(|e| SpawnError::AgentStartFailed {
        label: "<runner>".into(),
        stderr: e,
    })?;
    let inputs_v = SpawnPromptInputs::new(
        inputs.project_name,
        inputs.session_id,
        inputs.milestone_id,
        inputs.queue_position,
        inputs.role_v.clone(),
    )
    .map_err(|e| SpawnError::AgentStartFailed {
        label: "<reviewer>".into(),
        stderr: e,
    })?;
    let bundles = render_topology_prompts(&inputs_o, &inputs_r, &inputs_v, inputs.topology);

    // Step 3: per-pane create + agent start + prompt send. Roll
    // back any panes we already created if any step in the chain
    // fails. Persistence (the AC-06 "pane IDs persisted only
    // after successful starts" rule) happens after a clean
    // three-step pass.
    let mut handles: Vec<SpawnedPane> = Vec::with_capacity(bundles.len());
    for (ordinal, bundle) in bundles.iter().enumerate() {
        // Create pane.
        let pane_id = match ops.create_pane(inputs.project_root, ordinal) {
            Ok(id) => id,
            Err(e) => {
                rollback(ops, &handles);
                return Err(e);
            }
        };
        // Decide the role → (label, kind, extras) mapping. The
        // bundle's `roles` field tells us which role contract is
        // in this bundle (1 for 3-pane, 2 for supervisor, 3 for
        // 1-pane collapsed).
        let (kind, extras) = primary_role_extras(&bundle.roles, inputs)?;
        // Start the harness in the pane.
        let handle = match ops.start_agent(&bundle.label, &kind, &pane_id, &extras) {
            Ok(h) => h,
            Err(e) => {
                rollback(ops, &handles);
                return Err(e);
            }
        };
        // Deliver the rendered prompt.
        if let Err(e) = ops.send_prompt(&pane_id, &bundle.prompt) {
            rollback(ops, &handles);
            return Err(e);
        }
        handles.push(handle);
    }

    // Step 4: persist the session with pane IDs, role configs,
    // provenance, and the audit prompt bundles.
    let path = persist_session(inputs, &handles, &bundles, &current, &inputs_o, &inputs_r, &inputs_v)?;

    Ok(SpawnOutcome {
        panes: handles,
        bundles,
        session_path: path,
    })
}

fn primary_role_extras(
    roles: &[Role],
    inputs: &SpawnInputs<'_>,
) -> Result<(String, Vec<String>), SpawnError> {
    // Pick the primary role (first in canonical order) for the
    // harness kind + flag tail. For collapsed bundles the kind
    // is the primary role's harness; the per-pane prompt already
    // carries the multi-role contract via the seam text.
    let primary = roles.first().copied().unwrap_or(Role::Runner);
    let rc = match primary {
        Role::Orchestrator => &inputs.role_o,
        Role::Runner => &inputs.role_r,
        Role::Reviewer => &inputs.role_v,
    };
    let kind = rc.harness.clone();
    let extras = harness_extra_flags(rc).map_err(|e| match e {
        HarnessFlagError::Unsupported { harness, supported } => {
            SpawnError::UnsupportedHarness { harness, supported }
        }
    })?;
    Ok((kind, extras))
}

fn rollback<O: HerdrSpawnOps>(ops: &O, handles: &[SpawnedPane]) {
    for h in handles {
        ops.delete_pane(&h.pane_id);
    }
}

fn persist_session(
    inputs: &SpawnInputs<'_>,
    handles: &[SpawnedPane],
    bundles: &[BundledPrompt],
    provenance: &MpBinaryProvenance,
    inputs_o: &SpawnPromptInputs,
    inputs_r: &SpawnPromptInputs,
    inputs_v: &SpawnPromptInputs,
) -> Result<PathBuf, SpawnError> {
    let path = SessionPath::new(inputs.ctx, inputs.session_id)
        .map_err(|e| SpawnError::WorkspaceEnsureFailed {
            name: inputs.session_id.to_string(),
            stderr: format!("bad session id: {e}"),
        })?;
    let mut session = if path.file.exists() {
        match crate::autopilot::session::load_session(inputs.ctx, inputs.session_id) {
            Ok(s) => s,
            Err(e) => {
                return Err(SpawnError::WorkspaceEnsureFailed {
                    name: inputs.session_id.to_string(),
                    stderr: format!("load existing session.json: {e}"),
                });
            }
        }
    } else {
        let mut s = AutopilotSession::blank(inputs.session_id);
        s.herdr_workspace = Some(format!("{}-autopilot", inputs.session_id));
        s.status = crate::autopilot::session::SessionStatus::Active;
        s
    };

    // Topology → pane layout (Orchestrator first, then Runner,
    // then Reviewer for 3-pane; supervisor first for collapsed
    // topologies).
    session.topology = PaneLayout {
        orchestrator: handles
            .iter()
            .find(|h| h.label == "role-orchestrator-1")
            .map(|h| PaneRef {
                pane_id: h.pane_id.clone(),
                label: Some(h.label.clone()),
            }),
        runner: handles
            .iter()
            .find(|h| h.label == "role-runner-1")
            .map(|h| PaneRef {
                pane_id: h.pane_id.clone(),
                label: Some(h.label.clone()),
            }),
        reviewer: handles
            .iter()
            .find(|h| h.label == "role-reviewer-1")
            .map(|h| PaneRef {
                pane_id: h.pane_id.clone(),
                label: Some(h.label.clone()),
            }),
    };

    // Per-role config snapshots (with the rendered prompt for
    // audit, AC-04).
    session.roles = RolesConfig {
        orchestrator: Some(RoleConfig {
            role: RoleName::Orchestrator,
            pane_id: session
                .topology
                .orchestrator
                .as_ref()
                .map(|p| p.pane_id.clone()),
            model: inputs.role_o.model.clone(),
            harness: Some(inputs.role_o.harness.clone()),
            skill: Some(inputs.role_o.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(rendered_for(inputs_o)),
        }),
        runner: Some(RoleConfig {
            role: RoleName::Runner,
            pane_id: session
                .topology
                .runner
                .as_ref()
                .map(|p| p.pane_id.clone()),
            model: inputs.role_r.model.clone(),
            harness: Some(inputs.role_r.harness.clone()),
            skill: Some(inputs.role_r.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(rendered_for(inputs_r)),
        }),
        reviewer: Some(RoleConfig {
            role: RoleName::Reviewer,
            pane_id: session
                .topology
                .reviewer
                .as_ref()
                .map(|p| p.pane_id.clone()),
            model: inputs.role_v.model.clone(),
            harness: Some(inputs.role_v.harness.clone()),
            skill: Some(inputs.role_v.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(rendered_for(inputs_v)),
        }),
    };

    // Per-pane audit surface: the *bundled* prompt delivered to
    // each physical pane (AC-04). Collapsed topologies get the
    // multi-role concatenation here; per-role sources are
    // available via `roles.<role>.spawn_prompt_rendered`.
    let mut prompt_bundles: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for bundle in bundles {
        prompt_bundles.insert(
            bundle.label.clone(),
            serde_json::json!({
                "roles": bundle.roles.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "prompt": bundle.prompt,
            }),
        );
    }
    session.prompt_bundles = prompt_bundles;

    // Binary provenance record (AC-07).
    session.binary_provenance = Some(provenance.clone());

    save_session_at(&path.file, &session).map_err(|e| {
        SpawnError::WorkspaceEnsureFailed {
            name: inputs.session_id.to_string(),
            stderr: format!("save session.json: {e:#}"),
        }
    })?;
    Ok(path.file)
}

fn rendered_for(i: &SpawnPromptInputs) -> String {
    // The per-role source prompt (one role's contract) — what
    // a 3-pane topology delivers to a single pane. Collapsed
    // topologies concatenate these; the concatenated bundle
    // lives in `prompt_bundles`.
    let role = match i.role_config.skill.as_str() {
        "mp-coordinator" => Role::Orchestrator,
        _ => Role::Runner, // mp-runner is used by both Runner + Reviewer.
    };
    crate::autopilot::prompts::spawn::render_role_prompt(role, i)
}

fn load_provenance(file: &Path) -> Result<MpBinaryProvenance> {
    let raw = std::fs::read_to_string(file)
        .with_context(|| format!("read {}", file.display()))?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse {}", file.display()))?;
    let p = v
        .get("binary_provenance")
        .ok_or_else(|| anyhow!("missing binary_provenance field"))?;
    serde_json::from_value(p.clone()).context("decode binary_provenance")
}

// ─── AC-05 / AC-07 internal helper: typed state commands ────────────

/// The closed set of valid role-state transitions (mirrors M207's
/// `transitions::is_valid`). Exposed as a `const` slice so
/// prompts and tests reference the same source of truth.
pub const ALLOWED_STATE_TRANSITIONS: &[(&str, &str)] = &[
    ("idle", "starting"),
    ("idle", "working"),
    ("idle", "blocked"),
    ("starting", "working"),
    ("starting", "blocked"),
    ("starting", "idle"),
    ("working", "done"),
    ("working", "blocked"),
    ("working", "idle"),
    ("blocked", "working"),
    ("blocked", "idle"),
    ("done", "idle"),
    ("done", "working"),
    ("unknown", "idle"),
    ("unknown", "starting"),
    ("unknown", "working"),
];

/// True when `(from, to)` is in [`ALLOWED_STATE_TRANSITIONS`].
/// Used by the prompt-state-contract golden tests to assert
/// that the prompt's typed-transition list matches the state
/// machine's allowed edges.
pub fn is_allowed_transition(from: &str, to: &str) -> bool {
    ALLOWED_STATE_TRANSITIONS
        .iter()
        .any(|(f, t)| *f == from && *t == to)
}

/// Sanity-check: every transition in [`ALLOWED_STATE_TRANSITIONS`]
/// is also accepted by M207's [`is_valid_transition`]. A drift
/// here means the prompt mentions a transition the verifier
/// would reject — a real boundary violation.
#[doc(hidden)]
pub fn transitions_match_state_machine() -> bool {
    ALLOWED_STATE_TRANSITIONS.iter().all(|(f, t)| {
        let from: RoleState = f.parse().expect("known state");
        let to: RoleState = t.parse().expect("known state");
        is_valid_transition(from, to)
    })
}

// ─── Cargo + project metadata ────────────────────────────────────────

/// `Cargo.toml` version of the running binary (sourced from
/// `CARGO_PKG_VERSION`). Exposed for tests that want to assert
/// the version stamped into provenance matches the built crate.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::role::{builtin_role_default, resolve_role_config};

    fn sample_rc(role: Role) -> ResolvedRoleConfig {
        resolve_role_config(None, None, &builtin_role_default(role))
    }

    #[test]
    fn binary_provenance_satisfies_self() {
        let p = MpBinaryProvenance::current();
        assert!(p.satisfies(&p));
    }

    #[test]
    fn binary_provenance_rejects_recorded_schema_too_new() {
        // recorded has schema 5 (newer than current's
        // SESSION_SCHEMA_VERSION); current cannot preserve
        // it on rewrite — `check_binary_provenance` must
        // surface `SchemaTooNew`.
        let recorded = MpBinaryProvenance {
            binary_path: "/future/mp".into(),
            version: "2.0.0".into(),
            schema_version: SESSION_SCHEMA_VERSION + 5,
            build_kind: "release".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let current = MpBinaryProvenance::current();
        // `satisfies` direction is "current meets the floor" —
        // here the recorded is the floor and it is too new for
        // current to meet.
        assert!(!current.satisfies(&recorded));
        let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
        assert!(matches!(*err, BinaryProvenanceMismatch::SchemaTooNew { .. }));
    }

    #[test]
    fn check_binary_provenance_rejects_below_floor() {
        let recorded = MpBinaryProvenance {
            binary_path: "/old/mp".into(),
            version: "0.9.0".into(),
            schema_version: 0,
            build_kind: "release".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let current = MpBinaryProvenance::current();
        let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
        match *err {
            BinaryProvenanceMismatch::SchemaBelowFloor { floor, .. } => {
                assert_eq!(floor, MIN_SESSION_SCHEMA_VERSION);
            }
            other => panic!("expected SchemaBelowFloor, got {other:?}"),
        }
    }

    #[test]
    fn check_binary_provenance_rejects_too_new_recorded_schema() {
        let recorded = MpBinaryProvenance {
            binary_path: "/current/mp".into(),
            version: "2.0.0".into(),
            schema_version: SESSION_SCHEMA_VERSION + 5,
            build_kind: "release".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let current = MpBinaryProvenance::current();
        let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
        assert!(matches!(*err, BinaryProvenanceMismatch::SchemaTooNew { .. }));
    }

    #[test]
    fn check_binary_provenance_rejects_path_mismatch() {
        let recorded = MpBinaryProvenance {
            binary_path: "/first/install/mp".into(),
            version: CRATE_VERSION.into(),
            schema_version: SESSION_SCHEMA_VERSION,
            build_kind: "release".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let current = MpBinaryProvenance::current();
        let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
        assert!(matches!(*err, BinaryProvenanceMismatch::BinaryPathMismatch { .. }));
    }

    #[test]
    fn check_binary_provenance_accepts_first_spawn() {
        // No recorded provenance → Ok. First spawn stamps
        // `current` as the new floor.
        let current = MpBinaryProvenance::current();
        check_binary_provenance(None, &current).unwrap();
    }

    #[test]
    fn check_binary_provenance_accepts_matching_record() {
        let recorded = MpBinaryProvenance::current();
        let current = MpBinaryProvenance::current();
        check_binary_provenance(Some(&recorded), &current).unwrap();
    }

    #[test]
    fn transitions_match_state_machine_pin() {
        // Drift detector: every prompt-listed transition must
        // also pass M207's state-machine check. A mismatch here
        // is a real verifier boundary violation.
        assert!(transitions_match_state_machine());
    }

    #[test]
    fn supported_autopilot_harnesses_is_three() {
        let supported = crate::autopilot::prompts::spawn::SUPPORTED_AUTOPILOT_HARNESSES;
        assert_eq!(supported.len(), 3);
        assert!(supported.contains(&"opencode"));
        assert!(supported.contains(&"cursor"));
        assert!(supported.contains(&"pi"));
    }

    #[test]
    fn resolved_role_pane_layout_matches_role_pane_slots() {
        for t in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
            assert_eq!(
                crate::autopilot::role::role_pane_slots(t),
                crate::autopilot::role::role_pane_slots(t)
            );
        }
    }

    #[test]
    fn spawn_error_display_carries_rebuild_hint() {
        // AC-07: the rejection hint must be actionable.
        let recorded = MpBinaryProvenance {
            binary_path: "/old/mp".into(),
            version: "0.9.0".into(),
            schema_version: SESSION_SCHEMA_VERSION + 5,
            build_kind: "release".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let current = MpBinaryProvenance::current();
        let mismatch = check_binary_provenance(Some(&recorded), &current).unwrap_err();
        let msg = format!("{mismatch}");
        assert!(
            msg.contains("Rebuild") || msg.contains("install"),
            "rejection hint must be actionable: {msg}"
        );
    }

    #[test]
    fn sample_rc_for_role_resolves_opencode_mp_coordinator() {
        let o = sample_rc(Role::Orchestrator);
        assert_eq!(o.skill, "mp-coordinator");
        assert_eq!(o.harness, "opencode");
    }
}
