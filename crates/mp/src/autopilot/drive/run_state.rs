//! M178 S1 / AC-01, AC-06: latest-run control-plane state model.
//!
//! `WatchRunState` is the structured "what is the latest mp watch run
//! doing" surface that machine clients (and the upcoming Raul Watch
//! tab) read from `<plan_dir>/.mp/watch.state.json`. It extends the
//! M152 v1 `WatchState` with the AC-01 contract fields:
//!
//! ```text
//! {
//!   "schema_version": 2,
//!   "pid":                  12345,
//!   "started_at":           "2026-...",
//!   "last_updated_at":      "2026-...",
//!   "queue":                ["M170", "M171"],
//!   "active_queue_index":   0,
//!   "active_milestone":     "M170",
//!   "current_lifecycle":    "in-progress",
//!   "watch_stage":          "execute",
//!   "target_lifecycle":     "self-reviewed",
//!   "active_role":          "runner",
//!   "pane_ids":             {"runner": "%5", "coordinator": "%7"},
//!   "log_path":             "/abs/path/to/.mp/watch.log",
//!   "state_path":           "/abs/path/to/.mp/watch.state.json",
//!   "run_outcome":          null,                       // populated on terminal exit
//!   "milestone_outcomes":   [...],                     // per-milestone outcome log
//!   "panes":                [...],                     // v1 pane tracking, preserved
//!   "milestones":           [...]                       // v1 milestone tracking, preserved
//! }
//! ```
//!
//! ## Schema versioning
//!
//! [`WATCH_RUN_STATE_SCHEMA_VERSION`] is bumped to 2. A v1 file is
//! migrated on load (see [`WatchRunState::load_from`]) — the v1
//! `panes` and `milestones` arrays are preserved verbatim so existing
//! `--resume` reconciliation keeps working; the new v2 fields
//! default to `None` / empty when the source is v1 (the migration
//! must not fabricate live data — see AC-01 / "no fabrication").
//!
//! ## Why a separate module
//!
//! Keeping `WatchState` (v1) and `WatchRunState` (v2) in distinct
//! modules lets the legacy `--resume` path keep reading its struct
//! while the new control-plane surface consumes the wider one. The
//! crate's `mod.rs` re-exports both, and the migration helper in
//! `WatchRunState::load_from` is the only bridge.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::autopilot::drive::{Role, WatchState};
use crate::paths::PlanContext;
use crate::store::atomic_write;

use super::prompts::PromptStage;

/// v2 schema marker. The legacy v1 constant
/// ([`crate::autopilot::drive::WATCH_STATE_SCHEMA_VERSION`]) stays at `1`; a v1
/// file is migrated on load rather than refused.
pub const WATCH_RUN_STATE_SCHEMA_VERSION: u32 = 2;

/// Default path for the control-plane state file. Same directory and
/// filename as the v1 file so a v2 driver naturally reads whatever
/// the previous driver wrote.
pub fn default_run_state_path(plan_dir: &Path) -> PathBuf {
    plan_dir.join(".mp").join("watch.state.json")
}

/// Terminal outcome of a run. `None` means the run is still in flight
/// (or interrupted before reaching a terminal state). The discriminator
/// on the v2 wire format is `"running"` for `None`, plus the matching
/// variant name otherwise.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunOutcome {
    /// Every queued milestone reached `lifecycle: complete`.
    Completed,
    /// At least one milestone reached `complete`; another failed.
    PartialFailure,
    /// At least one milestone was skipped (blocked / cancelled / not
    /// ready). Sequential semantics: skip halts the run.
    Skipped { reason: String },
    /// Iteration cap exhausted before reaching `complete`.
    Exhausted { iterations: usize },
    /// SIGINT / SIGTERM surfaced during graceful shutdown.
    GracefullyStopped,
    /// M197 WP3 / AC-04: a `pane split` or `agent start` call
    /// failed with a verified exit code. The sequencer halts on
    /// this kind — retrying a known-bad launch would just waste
    /// the operator's time and pin the herdr pane in a stale
    /// state. Distinct from `PartialFailure` (which is a
    /// lifecycle-level failure) and from `Skipped` (which is a
    /// pre-execution decision). The `command` / `argv` / `exit_code` /
    /// `stdout` / `stderr` fields are the same payload the
    /// `spawn_error` watch log entry carries, so the operator sees the
    /// same diagnostic in the run summary and the watch log
    /// (M197 F-13: stdout was previously dropped here even though
    /// `SpawnFailure` carries it — herdr sometimes writes the real
    /// error reason to stdout and operators inspecting `mp watch
    /// status` could not see it).
    SpawnFailed {
        command: String,
        argv: Vec<String>,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl RunOutcome {
    pub fn is_terminal(&self) -> bool {
        true
    }

    pub fn label(&self) -> &'static str {
        match self {
            RunOutcome::Completed => "completed",
            RunOutcome::PartialFailure => "partial-failure",
            RunOutcome::Skipped { .. } => "skipped",
            RunOutcome::Exhausted { .. } => "exhausted",
            RunOutcome::GracefullyStopped => "gracefully-stopped",
            RunOutcome::SpawnFailed { .. } => "spawn-failed",
        }
    }
}

/// Per-milestone outcome entry. Mirrors the sequencer's
/// [`crate::autopilot::drive::MilestoneOutcome`] but serializes as a flat
/// tag-enum for easy consumption from outside the crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneRunOutcome {
    pub id: String,
    pub outcome: RunOutcome,
}

/// AC-01 control-plane state shape. The fields are exactly the
/// contract surfaced by `watch-control status`
/// (S4) plus the legacy v1 panes/milestones tracking (preserved
/// for `--resume` reconciliation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchRunState {
    pub schema_version: u32,
    /// Monotonic snapshot generation. Every durable transition increments
    /// this value so concurrent writers can never silently publish an older
    /// in-memory snapshot over a newer control-plane state.
    #[serde(default)]
    pub generation: u64,
    pub pid: u32,
    pub started_at: String,
    pub last_updated_at: String,

    // ── AC-01 contract fields (v2) ──────────────────────────────────
    /// Ordered milestone ids supplied to `mp watch start`. Stable
    /// across `--resume`; clients use this to display the queue.
    #[serde(default)]
    pub queue: Vec<String>,
    /// Index into `queue` of the milestone currently being driven.
    /// `None` while idle (no driver running).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_queue_index: Option<usize>,
    /// Milestone id at `queue[active_queue_index]` — duplicated so
    /// clients don't have to index into the array themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_milestone: Option<String>,
    /// Current lifecycle read from the on-disk milestone file. The
    /// state machine updates this on every flush.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_lifecycle: Option<String>,
    /// Stage the state machine is currently sending prompts for
    /// (e.g. `execute`, `self-review`, `external-review`, `remediate`,
    /// `approve`). Serializes as the lowercase label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_stage: Option<String>,
    /// Lifecycle target the current stage is waiting for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_lifecycle: Option<String>,
    /// Active pane role for the current stage. Used by
    /// `mp watch output` to pick the right pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_role: Option<Role>,
    /// Pane ids per role, recorded at spawn time. Distinct from the
    /// legacy v1 `panes` array so clients can read a flat
    /// role→pane_id map without scanning.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub pane_ids: std::collections::HashMap<Role, String>,
    /// Absolute path to the watch log (JSONL). Surfaced for clients
    /// that want to tail directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// Absolute path to this state file. Surfaced so `mp watch
    /// status` can advertise where to read the source-of-truth from
    /// (matters for clients that don't share a CWD with mp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_path: Option<String>,

    // ── AC-06 terminal outcomes (v2) ─────────────────────────────────
    /// `None` while the run is still in flight (or interrupted before
    /// reaching a terminal state). Set to `Some(...)` on every
    /// terminal exit — completed, partial failure, skipped,
    /// exhausted, gracefully stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_outcome: Option<RunOutcome>,
    /// Per-milestone outcome log. One entry per processed milestone;
    /// sequential semantics mean later milestones are absent after a
    /// halt (skip / exhaust / shutdown).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub milestone_outcomes: Vec<MilestoneRunOutcome>,

    // ── Legacy v1 fields (preserved for --resume reconciliation) ────
    #[serde(default)]
    pub panes: Vec<crate::autopilot::drive::PaneState>,
    #[serde(default)]
    pub milestones: Vec<crate::autopilot::drive::MilestoneState>,
}

impl WatchRunState {
    /// Build a fresh v2 state file with the current process's PID
    /// and "now" timestamps. The `queue` field carries the supplied
    /// milestone ids; everything else defaults to empty.
    pub fn fresh(active_milestones: &[String]) -> Self {
        let now = crate::store::now_rfc3339();
        let panes = Vec::new();
        let milestones = active_milestones
            .iter()
            .map(|id| crate::autopilot::drive::MilestoneState {
                id: id.clone(),
                last_lifecycle: "approved".to_string(),
                target_lifecycle: "in-progress".to_string(),
                last_action_at: now.clone(),
            })
            .collect();
        Self {
            schema_version: WATCH_RUN_STATE_SCHEMA_VERSION,
            generation: 0,
            pid: std::process::id(),
            started_at: now.clone(),
            last_updated_at: now,
            queue: active_milestones.to_vec(),
            active_queue_index: None,
            active_milestone: None,
            current_lifecycle: None,
            watch_stage: None,
            target_lifecycle: None,
            active_role: None,
            pane_ids: std::collections::HashMap::new(),
            log_path: None,
            state_path: None,
            run_outcome: None,
            milestone_outcomes: Vec::new(),
            panes,
            milestones,
        }
    }

    /// Default state-file path for a plan directory.
    pub fn path_for(plan_dir: &Path) -> PathBuf {
        default_run_state_path(plan_dir)
    }

    /// Atomic write through `atomic_write` so a SIGKILL mid-write
    /// never publishes a torn JSON document. The parent `<plan_dir>/.mp/`
    /// directory is created if missing.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create state parent {}", path.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .with_context(|| format!("serialize watch run state to {}", path.display()))?;
        atomic_write(path, bytes)
            .with_context(|| format!("atomic write watch run state {}", path.display()))
    }

    /// Atomic save under the default `.mp/watch.state.json` path for
    /// a plan context.
    pub fn save_to_plan(&self, ctx: &PlanContext) -> Result<PathBuf> {
        let path = Self::path_for(&ctx.plan_dir);
        self.save(&path)?;
        Ok(path)
    }

    /// Load the control-plane state. `Ok(None)` when the file is
    /// missing (fresh checkout — callers fall through to the
    /// non-resume path). A schema-version mismatch on a *future*
    /// schema returns `Ok(None)` with a warning so a downgrade never
    /// silently fabricates fields; v1 files migrate in place.
    pub fn load(ctx: &PlanContext) -> Result<Option<Self>> {
        let path = Self::path_for(&ctx.plan_dir);
        Self::load_from(&path)
    }

    /// Load the state file at an arbitrary path. Tries v2 first; on
    /// a v1 file, runs the migration and emits a `[M178-LOAD]`
    /// notice to stderr.
    pub fn load_from(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)
            .with_context(|| format!("read watch run state {}", path.display()))?;

        // Try v2 first. If the file parses as v2 with the current
        // schema_version we're done. Anything else — wrong version,
        // parse failure — falls through to a v1 migration attempt.
        match serde_json::from_slice::<WatchRunState>(&bytes) {
            Ok(v) if v.schema_version == WATCH_RUN_STATE_SCHEMA_VERSION => {
                return Ok(Some(v));
            }
            Ok(v) if v.schema_version > WATCH_RUN_STATE_SCHEMA_VERSION => {
                // Future-version file. The source of truth has newer
                // fields than we know about — don't fabricate them.
                // Treat as absent so the caller spawns fresh.
                eprintln!(
                    "[M178-LOAD] warning: watch run state {} has schema_version={}, \
                     known max is {}; treating as absent (forward-compat fallback)",
                    path.display(),
                    v.schema_version,
                    WATCH_RUN_STATE_SCHEMA_VERSION
                );
                return Ok(None);
            }
            // Older than v2 but parsed as a v2-shaped JSON document:
            // try the v1 migration path next.
            Ok(_) => {}
            // v2 parse failed entirely: same fallback — try v1.
            Err(_) => {}
        }

        // v2 parse failed — try v1 migration.
        match serde_json::from_slice::<WatchState>(&bytes) {
            Ok(v1) if v1.schema_version == 1 => {
                eprintln!(
                    "[M178-LOAD] notice: migrating watch state {} from v1 to v2 \
                     (panes + milestones preserved; v2 control fields default to None/empty)",
                    path.display()
                );
                Ok(Some(Self::migrate_from_v1(v1)))
            }
            Ok(other) => {
                eprintln!(
                    "[M178-LOAD] warning: watch state {} has unknown schema_version={}; treating as absent",
                    path.display(),
                    other.schema_version
                );
                Ok(None)
            }
            Err(e) => {
                eprintln!(
                    "[M178-LOAD] warning: ignoring unreadable watch state {}: {e}",
                    path.display()
                );
                Ok(None)
            }
        }
    }

    /// Migrate a v1 [`WatchState`] into a v2 [`WatchRunState`].
    /// Preserves panes and milestones verbatim; v2 control fields
    /// default to `None` / empty (no live fabrication).
    fn migrate_from_v1(v1: WatchState) -> Self {
        Self {
            schema_version: WATCH_RUN_STATE_SCHEMA_VERSION,
            generation: 0,
            pid: v1.pid,
            started_at: v1.started_at,
            last_updated_at: v1.last_updated_at,
            queue: v1.milestones.iter().map(|m| m.id.clone()).collect(),
            active_queue_index: None,
            active_milestone: None,
            current_lifecycle: None,
            watch_stage: None,
            target_lifecycle: None,
            active_role: None,
            pane_ids: std::collections::HashMap::new(),
            log_path: None,
            state_path: None,
            run_outcome: None,
            milestone_outcomes: Vec::new(),
            panes: v1.panes,
            milestones: v1.milestones,
        }
    }

    /// Bump `last_updated_at`. Cheap; callers are expected to call
    /// this on every meaningful mutation before persisting.
    pub fn touch(&mut self) {
        self.last_updated_at = crate::store::now_rfc3339();
    }

    /// Set the active stage. Convenience helper that records both
    /// the stage label and its target lifecycle in one call so the
    /// two fields stay in sync.
    pub fn set_active_stage(
        &mut self,
        stage: PromptStage,
        target: crate::autopilot::drive::LifecycleTarget,
    ) {
        self.watch_stage = Some(stage.label().to_string());
        self.target_lifecycle = Some(target.as_str().to_string());
        self.active_role = Some(stage.role());
        self.touch();
    }

    /// Record the currently-active milestone by queue index. `idx`
    /// is the position in [`Self::queue`]; the function stores both
    /// the index and the resolved id so clients can avoid an index
    /// lookup.
    pub fn set_active_milestone(&mut self, idx: usize, id: impl Into<String>) {
        self.active_queue_index = Some(idx);
        self.active_milestone = Some(id.into());
        self.touch();
    }

    /// Record the lifecycle read from the on-disk milestone.
    pub fn set_current_lifecycle(&mut self, lifecycle: impl Into<String>) {
        self.current_lifecycle = Some(lifecycle.into());
        self.touch();
    }

    /// Record a pane id for a role. Updates BOTH the flat
    /// `pane_ids` map (new control-plane read path) AND the legacy
    /// `panes` array (M152 reconciler read path) so the two stay
    /// in lockstep. M178 external-review F-08: previously only
    /// `pane_ids` was updated, so a production `mp watch` run
    /// produced `panes: []` and broke the legacy reconciler's
    /// `state.pane_for(role)` lookup. The legacy `PaneState` carries
    /// `pane_id`, `label` (derived from role), and `spawned_at`
    /// (the file write time); the production caller can re-spawn
    /// at any time so `spawned_at = now()` is the conservative
    /// choice (existing helper preserves `spawned_at` across
    /// re-saves via the same logic).
    pub fn record_pane(&mut self, role: Role, pane_id: impl Into<String>) {
        let pane_id = pane_id.into();
        let now = crate::store::now_rfc3339();
        self.pane_ids.insert(role, pane_id.clone());
        let label =
            crate::autopilot::drive::pane_label_for(role, crate::autopilot::drive::DEFAULT_PANE_N);
        if let Some(existing) = self.panes.iter_mut().find(|p| p.role == role) {
            // Preserve `spawned_at` on re-save (same contract as the
            // v1 PaneState::upsert_pane helper).
            if existing.pane_id != pane_id {
                existing.pane_id = pane_id;
                existing.label = label;
                existing.spawned_at = now;
            }
        } else {
            self.panes.push(crate::autopilot::drive::PaneState {
                role,
                label,
                pane_id,
                spawned_at: now,
                last_status: None,
            });
        }
        self.touch();
    }

    /// Append a per-milestone outcome. Used by the sequencer on every
    /// milestone terminal event (S2 / AC-06).
    pub fn push_milestone_outcome(&mut self, entry: MilestoneRunOutcome) {
        self.milestone_outcomes.push(entry);
        self.touch();
    }

    /// Set the run-level terminal outcome. Clears `active_milestone`
    /// and the live stage fields so a `status` reader distinguishes
    /// "terminal" from "live" by `run_outcome.is_some()`.
    pub fn set_run_outcome(&mut self, outcome: RunOutcome) {
        self.run_outcome = Some(outcome);
        self.active_milestone = None;
        self.active_queue_index = None;
        self.watch_stage = None;
        self.target_lifecycle = None;
        self.active_role = None;
        self.touch();
    }
}

/// One externally-observable Watch mutation. Lifecycle values are snapshots
/// read from the milestone store; this API never transitions a milestone.
#[derive(Debug, Clone)]
pub enum WatchTransition {
    ActiveMilestone {
        index: usize,
        id: String,
    },
    ActiveStage {
        stage: PromptStage,
        target: crate::autopilot::drive::LifecycleTarget,
    },
    LifecycleObserved(String),
    PaneObserved {
        role: Role,
        pane_id: String,
    },
    MilestoneOutcome(MilestoneRunOutcome),
    RunOutcome(RunOutcome),
    Pid(u32),
}

/// Durable transition store for `.mp/watch.state.json`.
///
/// A short-lived sibling lock serializes read/modify/write updates. Each
/// transition reloads the latest snapshot while holding that lock, applies
/// exactly one event, increments `generation`, and atomically publishes it.
/// The in-memory snapshot changes only after a successful write.
#[derive(Debug, Clone)]
pub struct WatchRunStore {
    path: PathBuf,
    state: WatchRunState,
}

impl WatchRunStore {
    pub fn new(path: PathBuf, state: WatchRunState) -> Self {
        Self { path, state }
    }

    pub fn state(&self) -> &WatchRunState {
        &self.state
    }

    pub fn transition(&mut self, event: WatchTransition) -> Result<&WatchRunState> {
        let lock_path = self.path.with_extension("json.lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let lock = loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => {
                    break StateLock {
                        path: lock_path,
                        file,
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&lock_path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(2));
                    if stale {
                        let _ = std::fs::remove_file(&lock_path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        anyhow::bail!(
                            "timed out acquiring watch state lock {}",
                            lock_path.display()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("acquire watch state lock {}", lock_path.display())
                    })
                }
            }
        };

        let mut next = match WatchRunState::load_from(&self.path)? {
            Some(state) => state,
            None if self.path.exists() => {
                anyhow::bail!(
                    "refusing watch transition because existing state is unreadable: {}",
                    self.path.display()
                )
            }
            None => self.state.clone(),
        };
        // CAS: RunOutcome is write-once under the lock. A concurrent stop
        // that reloads after Completed/PartialFailure must leave the newer
        // terminal snapshot untouched (no generation bump, no rewrite).
        if matches!(event, WatchTransition::RunOutcome(_)) && next.run_outcome.is_some() {
            drop(lock);
            self.state = next;
            return Ok(&self.state);
        }
        apply_transition(&mut next, event);
        next.generation = next.generation.saturating_add(1);
        next.touch();
        next.save(&self.path)?;
        drop(lock);
        self.state = next;
        Ok(&self.state)
    }
}

struct StateLock {
    path: PathBuf,
    #[allow(dead_code)]
    file: std::fs::File,
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn apply_transition(state: &mut WatchRunState, event: WatchTransition) {
    match event {
        WatchTransition::ActiveMilestone { index, id } => state.set_active_milestone(index, id),
        WatchTransition::ActiveStage { stage, target } => state.set_active_stage(stage, target),
        WatchTransition::LifecycleObserved(value) => state.set_current_lifecycle(value),
        WatchTransition::PaneObserved { role, pane_id } => state.record_pane(role, pane_id),
        WatchTransition::MilestoneOutcome(outcome) => state.push_milestone_outcome(outcome),
        // Terminal outcomes are write-once under the store lock: a late
        // GracefullyStopped (e.g. watch-control stop racing a Completed
        // flush) must not overwrite a newer terminal snapshot.
        WatchTransition::RunOutcome(outcome) => {
            if state.run_outcome.is_none() {
                state.set_run_outcome(outcome);
            }
        }
        WatchTransition::Pid(pid) => {
            state.pid = pid;
            state.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_state(ids: &[&str]) -> WatchRunState {
        WatchRunState::fresh(&ids.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn fresh_state_is_schema_v2_with_empty_control_fields() {
        let s = fresh_state(&["M170"]);
        assert_eq!(s.schema_version, WATCH_RUN_STATE_SCHEMA_VERSION);
        assert_eq!(s.schema_version, 2);
        assert_eq!(s.pid, std::process::id());
        assert_eq!(s.queue, vec!["M170"]);
        assert!(s.active_queue_index.is_none());
        assert!(s.active_milestone.is_none());
        assert!(s.run_outcome.is_none());
        assert!(s.milestone_outcomes.is_empty());
        assert!(s.pane_ids.is_empty());
        // legacy v1 fields preserved
        assert_eq!(s.milestones.len(), 1);
        assert_eq!(s.milestones[0].id, "M170");
    }

    #[test]
    fn round_trip_preserves_v2_contract_fields() {
        let dir = TempDir::new().unwrap();
        let path = WatchRunState::path_for(dir.path());
        let mut s = fresh_state(&["M170", "M171"]);
        s.set_active_milestone(0, "M170");
        s.set_current_lifecycle("in-progress");
        s.set_active_stage(
            PromptStage::Execute,
            crate::autopilot::drive::LifecycleTarget::SelfReviewed,
        );
        s.record_pane(Role::Runner, "%5");
        s.record_pane(Role::Coordinator, "%7");
        s.log_path = Some(dir.path().join("watch.log").display().to_string());
        s.state_path = Some(path.display().to_string());
        s.push_milestone_outcome(MilestoneRunOutcome {
            id: "M170".into(),
            outcome: RunOutcome::Completed,
        });
        s.set_run_outcome(RunOutcome::GracefullyStopped);

        s.save(&path).unwrap();
        let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.queue, vec!["M170", "M171"]);
        assert!(loaded.active_queue_index.is_none());
        assert!(loaded.active_milestone.is_none());
        assert!(loaded.run_outcome.is_some());
        assert_eq!(loaded.run_outcome.unwrap().label(), "gracefully-stopped");
        assert_eq!(loaded.milestone_outcomes.len(), 1);
        assert_eq!(
            loaded.pane_ids.get(&Role::Runner).map(String::as_str),
            Some("%5")
        );
        assert_eq!(
            loaded.pane_ids.get(&Role::Coordinator).map(String::as_str),
            Some("%7")
        );
    }

    #[test]
    fn set_active_stage_records_stage_target_and_role_in_sync() {
        let mut s = fresh_state(&["M170"]);
        s.set_active_stage(
            PromptStage::ExternalReview,
            crate::autopilot::drive::LifecycleTarget::Reviewed,
        );
        assert_eq!(s.watch_stage.as_deref(), Some("external-review"));
        assert_eq!(s.target_lifecycle.as_deref(), Some("reviewed"));
        assert_eq!(s.active_role, Some(Role::Coordinator));
    }

    #[test]
    fn set_run_outcome_clears_live_fields() {
        let mut s = fresh_state(&["M170"]);
        s.set_active_milestone(0, "M170");
        s.set_current_lifecycle("in-progress");
        s.set_active_stage(
            PromptStage::Execute,
            crate::autopilot::drive::LifecycleTarget::SelfReviewed,
        );
        s.set_run_outcome(RunOutcome::Completed);

        assert!(s.active_milestone.is_none());
        assert!(s.active_queue_index.is_none());
        assert!(s.watch_stage.is_none());
        assert!(s.target_lifecycle.is_none());
        assert!(s.active_role.is_none());
        // current_lifecycle is preserved — the final lifecycle is part
        // of the terminal record.
        assert!(s.run_outcome.is_some());
    }

    #[test]
    fn load_from_v1_migrates_without_fabricating_live_data() {
        let dir = TempDir::new().unwrap();
        // Write the file directly via default_run_state_path so the
        // migration test exercises the same path the CLI would
        // (default_run_state_path needs a plan_dir-shaped input).
        let path = default_run_state_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Hand-craft a v1 file: just the schema_version + panes +
        // milestones array (the v1 minimum surface).
        let v1_json = serde_json::json!({
            "schema_version": 1,
            "pid": 99999,
            "started_at": "2026-07-01T00:00:00+00:00",
            "last_updated_at": "2026-07-01T00:00:00+00:00",
            "panes": [{
                "role": "runner",
                "label": "role-runner-1",
                "pane_id": "%5",
                "spawned_at": "2026-07-01T00:00:00+00:00"
            }],
            "milestones": [{
                "id": "M170",
                "last_lifecycle": "approved",
                "target_lifecycle": "in-progress",
                "last_action_at": "2026-07-01T00:00:00+00:00"
            }]
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&v1_json).unwrap()).unwrap();

        let loaded = WatchRunState::load_from(&path).unwrap().unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.pid, 99999);
        // queue derived from the v1 milestones list
        assert_eq!(loaded.queue, vec!["M170"]);
        // panes / milestones preserved verbatim
        assert_eq!(loaded.panes.len(), 1);
        assert_eq!(loaded.panes[0].role, Role::Runner);
        assert_eq!(loaded.milestones.len(), 1);
        assert_eq!(loaded.milestones[0].id, "M170");
        // v2 control fields default to None / empty (no fabrication)
        assert!(loaded.active_queue_index.is_none());
        assert!(loaded.active_milestone.is_none());
        assert!(loaded.current_lifecycle.is_none());
        assert!(loaded.watch_stage.is_none());
        assert!(loaded.target_lifecycle.is_none());
        assert!(loaded.active_role.is_none());
        assert!(loaded.pane_ids.is_empty());
        assert!(loaded.run_outcome.is_none());
        assert!(loaded.milestone_outcomes.is_empty());
    }

    #[test]
    fn load_from_corrupt_file_returns_none_without_panicking() {
        let dir = TempDir::new().unwrap();
        let path = default_run_state_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"this is not json {").unwrap();
        let loaded = WatchRunState::load_from(&path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn load_from_future_schema_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = default_run_state_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let future = serde_json::json!({
            "schema_version": 999,
            "pid": 1,
            "started_at": "2026-07-01T00:00:00+00:00",
            "last_updated_at": "2026-07-01T00:00:00+00:00",
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&future).unwrap()).unwrap();
        let loaded = WatchRunState::load_from(&path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn save_creates_parent_dir_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = default_run_state_path(dir.path());
        assert!(
            !path.parent().unwrap().exists(),
            ".mp/ should not yet exist on a fresh dir"
        );
        fresh_state(&["M170"]).save(&path).unwrap();
        assert!(path.is_file());
    }
}
