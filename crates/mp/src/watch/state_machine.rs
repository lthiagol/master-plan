//! S7 / AC-02, AC-03, AC-07: lifecycle state machine.
//!
//! The driver loop. Maps a milestone's current lifecycle state to a
//! [`PromptStage`], ensures the right role's pane exists, sends the
//! prompt, waits for the lifecycle transition (S5), records the
//! handoff, and loops until the milestone reaches `complete`.
//!
//! Layering: pure state-machine logic over an injectable [`DriveOps`]
//! trait. Tests pass mocks; production wraps `mp` + `herdr` binaries
//! into [`SystemDriveOps`]. Skip logic (AC-07) lives in
//! [`should_skip`]: milestones that are blocked, cancelled, deferred,
//! or not yet ready are skipped with a reason rather than attempted.

use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use serde::Serialize;

use crate::milestone::load_milestone_by_id;
use std::path::Path;

use crate::model::MilestoneFile;
use crate::paths::PlanContext;
use crate::watch::{
    clear_stage_done_sentinel, ensure_pane, lifecycle_advanced_past, pane_label_for,
    read_agent_status, read_custom_status_bounded, read_lifecycle_via_mp, send_prompt,
    sentinel_matches, LifecycleTarget, PaneHandle, PromptStage, ReadinessOptions, Role, RunOutcome,
    WaitOptions, WaitOutcome, WatchRunState, DEFAULT_BRIDGE_POLL_TIMEOUT_MS, DEFAULT_PANE_N,
};

/// Operations the state machine needs. Implementations:
/// - [`SystemDriveOps`] — production; spawns `mp` + `herdr` subprocesses.
/// - `MockDriveOps` (in tests/watch_execution.rs) — canned sequences.
pub trait DriveOps {
    /// Read the current on-disk milestone. Called at the top of each
    /// loop iteration to pick the next stage.
    fn read_milestone(&mut self) -> Result<MilestoneFile>;
    /// Ensure a pane exists for the given role. Returns the pane
    /// handle (may be reused from a prior iteration — AC-04).
    fn ensure_pane(&mut self, role: Role) -> Result<PaneHandle>;
    /// Send a prompt to a pane (readiness-gated). S4's `send_prompt`.
    fn send_prompt_to(&mut self, pane: &PaneHandle, text: &str) -> Result<()>;

    /// Emit a structured event for the operator. M153 S2 surfaces
    /// prompt-source attribution here (override vs default). Mock
    /// implementations can no-op or buffer. `&self` (not `&mut self`)
    /// so callers can borrow other fields (e.g., `rc` configs) across
    /// the call.
    fn log_event(&self, kind: &'static str, message: impl Into<String>);

    /// Absolute plan directory the ops is bound to, used by M153 S2
    /// to resolve `<plan_dir>/watch/<stage>.md` overrides. The plan
    /// dir already exists at this point — drives never run before
    /// the plan is on disk.
    fn plan_dir(&self) -> &Path;
    /// Wait for the milestone lifecycle to reach `target` (or advance
    /// past it). S5's `wait_for_lifecycle`.
    fn wait_for_lifecycle(&mut self, target: LifecycleTarget) -> Result<WaitOutcome>;
    /// Record a handoff entry after a successful transition (e.g. via
    /// `mp reviews handoff`). The `transition` arg is a short tag like
    /// `"approved->in-progress"`.
    fn record_handoff(&mut self, transition: &str) -> Result<()>;

    /// M178 external-review F-01: stamp the v2 control-plane
    /// state with the active stage + target so `mp watch-control
    /// status` reads `watch_stage`, `target_lifecycle`, and
    /// `active_role` during a live run (AC-01 contract). Called
    /// by `drive_milestone` once per iteration before `ensure_pane`.
    /// Default implementation is a no-op so test mocks
    /// (`MockDriveOps`) don't need to override it.
    fn set_active_stage(
        &mut self,
        stage: crate::watch::PromptStage,
        target: crate::watch::LifecycleTarget,
    ) -> Result<()> {
        let _ = (stage, target);
        Ok(())
    }
}

/// What `drive_milestone` decided for this milestone.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum DriveOutcome {
    /// Milestone reached lifecycle=complete.
    Complete,
    /// Milestone was skipped (AC-07) — see the reason for the verdict.
    Skipped { reason: String },
    /// The loop ran out of iterations before reaching complete.
    /// Defensive bound; should not fire in normal operation.
    MaxIterationsExhausted { iterations: usize },
    /// M152 S4: a SIGINT/SIGTERM was observed. The drive loop
    /// returned cleanly so the cli layer can flush the watch
    /// state file + write a flash note on the in-flight
    /// milestone. Distinct from `MaxIterationsExhausted` because
    /// shutdown is an expected termination, not a runaway.
    Shutdown,
    /// M197 WP3 / AC-04: a `pane split` or `agent start` call
    /// failed with a verified non-zero exit. The sequencer
    /// halts on this kind — retrying a known-bad launch
    /// would just pin the herdr pane in a stale state and
    /// waste the operator's time. The `run_outcome` payload
    /// is the same `RunOutcome::SpawnFailed` the sequencer
    /// forwards to the v2 control-plane state, so `mp watch
    /// status` shows the operator a single, consistent
    /// diagnostic in both the JSON output and the run
    /// summary.
    SpawnFailed {
        #[serde(rename = "run_outcome")]
        run_outcome: Box<RunOutcome>,
    },
}

/// Skip verdict for AC-07. Centralizes the readiness definition so
/// the state machine and the dry-run preview (S2/S9) agree.
pub fn should_skip(m: &MilestoneFile) -> Option<String> {
    let ms = &m.milestone;
    if ms.cancelled {
        return Some("cancelled".to_string());
    }
    if ms.deferred {
        return Some("deferred".to_string());
    }
    if ms.blocked {
        return Some(format!("blocked: {}", ms.block_reason.trim()));
    }
    // Already-complete milestones are skipped, not re-driven.
    if ms.lifecycle == "complete" {
        return Some("already complete".to_string());
    }
    // A milestone at lifecycle=approved that does not satisfy the
    // AC-02 readiness contract is skipped — mp watch only drives
    // ready milestones (Q-01 resolved: grooming stays manual).
    if ms.lifecycle == "approved" && !crate::commands::watch::is_ready(m) {
        let mut reasons = Vec::new();
        if ms.spec_status != "ready" {
            reasons.push(format!("spec_status={}", ms.spec_status));
        }
        if ms.execution_status != "planned" {
            reasons.push(format!("execution_status={}", ms.execution_status));
        }
        if reasons.is_empty() {
            reasons.push("not ready".to_string());
        }
        return Some(format!("approved but not ready ({})", reasons.join(", ")));
    }
    // Review aliases are migration/read compatibility only. Watch drives the
    // canonical delivery projection exported by mp-model.
    if !mp_model::is_watch_drivable_lifecycle(&ms.lifecycle) {
        return Some(format!(
            "lifecycle={} (mp watch only drives approved→complete)",
            ms.lifecycle
        ));
    }
    None
}

/// Map the current lifecycle state to the prompt stage the watch
/// loop should send next. Returns `None` if no stage applies (the
/// caller should skip or escalate).
pub fn next_stage(m: &MilestoneFile) -> Option<StagePlan> {
    let ms = &m.milestone;
    match ms.lifecycle.as_str() {
        "approved" => Some(StagePlan {
            stage: PromptStage::Execute,
            target: LifecycleTarget::Complete,
        }),
        "in-progress" => Some(StagePlan {
            stage: PromptStage::Execute,
            target: LifecycleTarget::Complete,
        }),
        "remediation" => Some(StagePlan {
            stage: PromptStage::Remediate,
            target: LifecycleTarget::Complete,
        }),
        _ => None,
    }
}

/// What the watch loop should do next for a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagePlan {
    pub stage: PromptStage,
    pub target: LifecycleTarget,
}

/// Drive a single milestone from its current state toward complete.
/// Loops at most `max_iterations` times (default 10) to bound runaway
/// loops — every successful transition consumes one iteration.
pub fn drive_milestone<O: DriveOps>(ops: &mut O, max_iterations: usize) -> Result<DriveOutcome> {
    let mut iter = 0;
    loop {
        iter += 1;

        // M152 S4: at the top of every iteration, check whether a
        // graceful shutdown was requested (SIGINT/SIGTERM). A
        // `Shutdown` outcome lets the sequencer + cli layer perform
        // cleanup (state-file flush + flash note) before exiting.
        // The check is cheap (one atomic load) and bounded to the
        // iteration cadence — every stage transition waits at
        // least one poll interval, so this fires within ~1s of the
        // signal under default config.
        if crate::watch::shutdown_requested() {
            return Ok(DriveOutcome::Shutdown);
        }
        if iter > max_iterations.max(1) {
            return Ok(DriveOutcome::MaxIterationsExhausted {
                iterations: iter - 1,
            });
        }

        let m = ops.read_milestone()?;
        // Re-check the shutdown flag right after the read_milestone
        // subprocess to also catch a signal that arrived while the
        // subprocess was spawning — the wait loop below also checks,
        // but adding one here shortens the worst-case latency.
        if crate::watch::shutdown_requested() {
            return Ok(DriveOutcome::Shutdown);
        }
        // Complete is the goal state — check it before skip verdicts so
        // arriving at an already-complete milestone yields `Complete`,
        // not `Skipped("already complete")`.
        if m.milestone.lifecycle == "complete" {
            return Ok(DriveOutcome::Complete);
        }
        if let Some(reason) = should_skip(&m) {
            return Ok(DriveOutcome::Skipped { reason });
        }

        let Some(plan) = next_stage(&m) else {
            bail!(
                "no stage plan for lifecycle='{}' on milestone {}",
                m.milestone.lifecycle,
                m.milestone.id
            );
        };

        // M178 external-review F-01: stamp the v2 control-plane
        // state with the current stage + target before ensure_pane
        // so `mp watch-control status` reads `watch_stage`,
        // `target_lifecycle`, and `active_role` during a live run
        // (AC-01 contract). ops is `&mut DriveOps` so we route
        // through the trait surface; for SystemDriveOps this hits
        // the v2 run_state attached in `cmd_watch_drive`.
        ops.set_active_stage(plan.stage, plan.target)?;

        // Don't re-send an in-progress prompt if the runner is already
        // mid-execute — just wait for the transition. The first
        // approved→in-progress transition sends the prompt; subsequent
        // iterations of in-progress only poll.
        let already_dispatched =
            m.milestone.lifecycle == "in-progress" && plan.stage == PromptStage::Execute;

        if !already_dispatched {
            let pane = ops.ensure_pane(plan.stage.role())?;
            // M153: thread the plan directory so the override loader
            // can resolve `<plan_dir>/watch/<stage>.md` and log which
            // surface (override vs compiled default) served the body.
            let plan_dir = ops.plan_dir();
            let req = crate::watch::BuildPromptRequest {
                stage: plan.stage,
                milestone: &m,
                options: &crate::watch::PromptRenderOptions::default(),
                override_dir: None,
                plan_dir: Some(plan_dir),
            };
            let rendered = crate::watch::build_prompt_full(&req, crate::watch::MAX_OVERRIDE_BYTES);
            // M153 S2 done_when: "the log records 'override' vs
            // 'default' per stage". Logged per stage so an operator
            // can see which surface was used. F-11: each refused
            // override rung becomes its own `override_refused`
            // structured event so a missing `{header}` or oversized
            // file is visible in the log instead of silently falling
            // through to the compiled default.
            for d in &rendered.override_diagnostics {
                ops.log_event(
                    "override_refused",
                    format!(
                        "{}: rung={:?} kind={:?} path={} message={}",
                        plan.stage.label(),
                        d.rung,
                        d.kind,
                        d.path.display(),
                        d.message,
                    ),
                );
            }
            ops.log_event(
                "prompt_source",
                format!(
                    "{} → {}",
                    plan.stage.label(),
                    match &rendered.source {
                        crate::watch::TemplateSource::ProjectOverride(p) =>
                            format!("override ({})", p.display()),
                        crate::watch::TemplateSource::CompiledDefault => "default".to_string(),
                        crate::watch::TemplateSource::Hardcoded(name) =>
                            format!("hardcoded ({name})"),
                    }
                ),
            );
            ops.send_prompt_to(&pane, &rendered.text)?;
        }

        let outcome = ops.wait_for_lifecycle(plan.target)?;
        let transition = format!("→{}", plan.target.as_str());
        ops.record_handoff(&transition)?;
        let _ = outcome; // S5 already validated the transition or escalated
    }
}

// ─── SystemDriveOps: production wrapper over mp + herdr binaries ──────────────

use std::path::PathBuf;

/// Production [`DriveOps`] implementation that calls `mp` and `herdr`
/// subprocesses. Built by [`SystemDriveOps::new`] with the binary
/// paths + project root.
///
/// Review finding #9: all fields are `pub(crate)` (not `pub`). External
/// callers go through the typed constructor and the `set_active_milestone`
/// / `set_logger` setters; this prevents library consumers from corrupting
/// the pane cache or swapping the binary path mid-run.
pub struct SystemDriveOps {
    pub(crate) mp_bin: PathBuf,
    pub(crate) herdr_bin: PathBuf,
    pub(crate) project_root: PathBuf,
    /// M153 S2: precomputed `<project_root>/master-plan` so the
    /// override loader can take a `&Path` borrow rather than
    /// reconstructing a temporary string every call.
    pub(crate) plan_dir: PathBuf,
    /// Currently-active milestone id. Updated by the sequencer between
    /// milestones via [`Self::set_active_milestone`].
    pub(crate) active_milestone_id: Option<String>,
    pub(crate) runner_config: crate::config::RoleConfig,
    pub(crate) coordinator_config: crate::config::RoleConfig,
    pub(crate) readiness: ReadinessOptions,
    pub(crate) wait: WaitOptions,
    /// Cached pane handles per role, so ensure_pane reuses across
    /// iterations (AC-04). First call spawns; subsequent calls hit
    /// the cache.
    pub(crate) pane_cache: std::collections::HashMap<Role, PaneHandle>,
    /// The pane that received the most recent prompt. The bridge
    /// fast-path polls this pane for the stage-done sentinel — it
    /// is the producer pane by construction, NOT the ambient
    /// `HERDR_PANE_ID` (the latter is the pane the watch process is
    /// running in, which may differ in the multi-pane topology).
    /// Set by [`Self::send_prompt_to`]; cleared on milestone switch
    /// via [`Self::set_active_milestone`].
    pub(crate) last_prompt_pane: Option<PaneHandle>,
    /// M152 ext-review F-02 (2026-07-14): the lifecycle target the
    /// current iteration is waiting for. Used by graceful-shutdown
    /// to persist the *actual* `target_lifecycle` to
    /// `watch.state.json` instead of a hardcoded placeholder.
    /// Set by [`Self::wait_for_lifecycle`] on every entry; cleared
    /// on milestone switch via [`Self::set_active_milestone`].
    pub(crate) current_target: Option<crate::watch::LifecycleTarget>,
    /// Optional structured logger. When `Some`, every herdr call +
    /// state-machine transition writes a JSONL entry. Review finding
    /// #2 / AC-08.
    pub(crate) logger: Option<crate::watch::WatchLogger>,
    /// M178 S2: latest-run control-plane state. Updated on every
    /// pane spawn, prompt dispatch, lifecycle wait, and handoff.
    /// Persisted to `.mp/watch.state.json` via `persist_state`
    /// so the `mp watch status / stop / output` subcommands can read
    /// the v2 contract fields (`active_milestone`, `watch_stage`,
    /// `target_lifecycle`, `pane_ids`, `run_outcome`, …). The struct
    /// is `Option` so legacy callers (tests that build
    /// `SystemDriveOps::new(...)` directly) keep compiling without
    /// a state file — they just get `None` for the live status.
    pub(crate) run_store: Option<crate::watch::WatchRunStore>,
    /// M226 F-01 wiring: autopilot session id used to consult the
    /// session event log for prior `AssignmentDispatched` events
    /// before spawning. Set via [`Self::set_session_id`] by the
    /// sequencer. When `None`, the dedup short-circuit is skipped
    /// (legacy callers and tests without an autopilot session).
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RoleConfigs {
    pub runner: crate::config::RoleConfig,
    pub coordinator: crate::config::RoleConfig,
}

impl SystemDriveOps {
    pub fn new(
        mp_bin: impl Into<PathBuf>,
        herdr_bin: impl Into<PathBuf>,
        project_root: impl Into<PathBuf>,
        milestone_id: impl Into<String>,
        role_configs: RoleConfigs,
    ) -> Self {
        let project_root: PathBuf = project_root.into();
        Self {
            mp_bin: mp_bin.into(),
            herdr_bin: herdr_bin.into(),
            plan_dir: project_root.join("master-plan"),
            project_root,
            active_milestone_id: Some(milestone_id.into()),
            runner_config: role_configs.runner,
            coordinator_config: role_configs.coordinator,
            readiness: ReadinessOptions::default(),
            wait: WaitOptions::default(),
            pane_cache: Default::default(),
            last_prompt_pane: None,
            current_target: None,
            logger: None,
            run_store: None,
            session_id: None,
        }
    }

    /// M178 S2: attach the v2 control-plane state to the ops. Called
    /// by `cmd_watch_drive` once at startup with a freshly-built
    /// [`WatchRunState`] carrying the supplied queue. From this
    /// point on, every pane spawn, prompt dispatch, lifecycle
    /// wait, and handoff updates the state, and callers can
    /// `persist_state` at any time to flush to disk.
    pub fn attach_run_state(&mut self, state: WatchRunState) {
        let path = crate::watch::default_run_state_path(&self.plan_dir);
        self.run_store = Some(crate::watch::WatchRunStore::new(path, state));
    }

    /// Borrow the attached v2 state (if any). `None` for legacy
    /// callers that haven't called [`Self::attach_run_state`].
    pub fn run_state(&self) -> Option<&WatchRunState> {
        self.run_store.as_ref().map(|store| store.state())
    }

    /// Borrow the attached v2 state mutably.
    pub fn transition(
        &mut self,
        event: crate::watch::WatchTransition,
    ) -> anyhow::Result<Option<&WatchRunState>> {
        match self.run_store.as_mut() {
            Some(store) => Ok(Some(store.transition(event)?)),
            None => Ok(None),
        }
    }

    /// Update the active milestone id before calling `drive_milestone`
    /// for the next milestone in the sequence. The sequencer calls
    /// this once per milestone. The previous milestone's
    /// `last_prompt_pane` is cleared so the bridge fast-path starts
    /// with an empty cache; `drive_milestone` will repopulate it on
    /// the first `send_prompt_to` of the new milestone.
    /// `current_target` is also cleared — a new milestone starts
    /// without an inherited wait target.
    ///
    /// M178 external-review F-01: also stamp the v2 control-plane
    /// state with the queue index so `mp watch-control status`
    /// reads `active_queue_index` and `active_milestone` during a
    /// live run (AC-01 contract). The queue index is recovered by
    /// locating `id` in `run_state.queue`; missing from the queue
    /// falls back to "no queue position known" (None).
    pub fn set_active_milestone(&mut self, id: impl Into<String>) -> Result<()> {
        let id = id.into();
        self.active_milestone_id = Some(id.clone());
        self.last_prompt_pane = None;
        self.current_target = None;
        let idx = self
            .run_state()
            .and_then(|state| state.queue.iter().position(|q| q == &id))
            .unwrap_or(0);
        self.transition(crate::watch::WatchTransition::ActiveMilestone { index: idx, id })?;
        Ok(())
    }

    /// Attach a structured logger. Every herdr call + state-machine
    /// transition writes a JSONL entry. The logger is cloned cheaply
    /// (Arc-shared) so callers can keep a handle for independent
    /// writes.
    pub fn set_logger(&mut self, logger: crate::watch::WatchLogger) {
        self.logger = Some(logger);
    }

    /// M226 F-01 wiring: attach the autopilot session id used for
    /// the dispatch dedup check inside `ensure_pane`. When set, the
    /// production spawn path consults the session event log via
    /// `was_already_applied` before spawning a fresh herdr
    /// `agent start`. Without a session id, the dedup short-circuit
    /// is skipped (legacy callers + library tests).
    pub fn set_session_id(&mut self, session_id: impl Into<String>) {
        self.session_id = Some(session_id.into());
    }

    /// Returns the path the logger writes to (if any). Used in the
    /// CLI report so callers know where to `tail -f` the trail.
    pub fn log_path(&self) -> Option<PathBuf> {
        self.logger.as_ref().map(|l| l.path())
    }

    /// Convenience accessor for tests + the standalone `ensure_pane`
    /// helper to read the configured runner role. Production code
    /// stays inside `ensure_pane` which uses this directly.
    pub fn runner_config(&self) -> &crate::config::RoleConfig {
        &self.runner_config
    }

    /// Set the wait options used by `wait_for_lifecycle`.
    pub fn set_wait_options(&mut self, opts: WaitOptions) {
        self.wait = opts;
    }

    /// Read the current wait options (used by the CLI to apply
    /// partial overrides without losing the other field).
    pub fn wait_options(&self) -> WaitOptions {
        self.wait
    }

    /// Look up a cached pane handle by role (without spawning).
    /// Used by tests to assert pane-reuse behavior.
    pub fn cached_pane(&self, role: Role) -> Option<&PaneHandle> {
        self.pane_cache.get(&role)
    }

    /// Pre-seed the pane cache for tests + ops migrations. Production
    /// callers should let `ensure_pane` populate the cache naturally.
    pub fn prefill_pane_cache(&mut self, role: Role, handle: PaneHandle) {
        self.pane_cache.insert(role, handle);
    }

    /// Read the most recently prompted pane (the producer pane the
    /// bridge fast-path polls). Returns `None` when no prompt has
    /// been dispatched in the current milestone — tests + ops
    /// migrations use this to assert F-12 (producer pane tracking).
    pub fn last_prompt_pane(&self) -> Option<&PaneHandle> {
        self.last_prompt_pane.as_ref()
    }

    /// Override the most-recent-prompted pane handle. Useful for
    /// tests that want to seed the fast-path without going through
    /// `send_prompt_to` (which shells out to herdr).
    pub fn set_last_prompt_pane(&mut self, pane: PaneHandle) {
        self.last_prompt_pane = Some(pane);
    }

    /// M152 ext-review F-02 (2026-07-14): the lifecycle target the
    /// current iteration is waiting for. Used by graceful-shutdown
    /// to persist the correct `target_lifecycle` to
    /// `watch.state.json`. `None` before any `wait_for_lifecycle`
    /// has run for the active milestone.
    pub fn current_target(&self) -> Option<LifecycleTarget> {
        self.current_target
    }

    /// Convenience: log an entry with optional milestone / role /
    /// pane context. No-op when no logger is attached.
    pub(crate) fn log_event(&self, kind: &'static str, message: impl Into<String>) {
        if let Some(logger) = &self.logger {
            let mut entry = crate::watch::WatchLogEntry::new(kind, message);
            if let Some(id) = &self.active_milestone_id {
                entry = entry.milestone(id);
            }
            let _ = logger.log(&entry);
        }
    }

    /// M150 S3: integrated bridge + lifecycle wait.
    ///
    /// Single loop that races the stage-done sentinel against the
    /// lifecycle poll. The sentinel is a sub-second wake-up that
    /// triggers an immediate lifecycle confirmation; the lifecycle
    /// poll is the authoritative source and runs on the same
    /// `WaitOptions::poll_interval_ms` cadence as M149 (no
    /// regression — F-11).
    ///
    /// Loop body, each tick:
    /// 1. **Lifecycle poll** (every `WaitOptions::poll_interval_ms`):
    ///    runs first so it cannot be blocked by a slow bridge poll.
    ///    Reads `mp show milestone <id> --fields milestone.lifecycle`.
    ///    Returns `Reached` / `AdvancedPast` on match; otherwise
    ///    runs stall detection (F-13: bounded subprocess calls).
    /// 2. **Sentinel poll** (every ~100ms when a producer pane is
    ///    tracked): deadline-protected `herdr pane get` — the
    ///    subprocess only starts when its bounded timeout plus the
    ///    tick margin fit before the next lifecycle tick. On match,
    ///    do an immediate lifecycle read; if confirmed, return
    ///    `Reached` / `AdvancedPast` and best-effort clear the
    ///    sentinel so the next stage starts clean. If the lifecycle
    ///    has NOT advanced, treat the sentinel as stale (F-10):
    ///    clear it best-effort and keep polling.
    /// 3. Tick sleep so neither call starves the main thread.
    ///
    /// The bridge is the wake-up hint; the lifecycle is the truth
    /// (F-15). On a silent / failing / missing bridge, the lifecycle
    /// poll drives completion on its normal cadence (F-11).
    fn integrated_wait(&mut self, target: LifecycleTarget) -> Result<WaitOutcome> {
        let id = self
            .active_milestone_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("SystemDriveOps: no active milestone id set"))?;
        let producer_pane_id = self
            .last_prompt_pane
            .as_ref()
            .map(|p| p.pane_id.clone())
            .or_else(|| {
                self.pane_cache
                    .get(&Role::Runner)
                    .map(|p| p.pane_id.clone())
            });
        let liveness_pane = self
            .last_prompt_pane
            .clone()
            .or_else(|| self.pane_cache.get(&Role::Runner).cloned())
            .or_else(|| self.pane_cache.get(&Role::Coordinator).cloned());

        let lifecycle_poll_ms = self.wait.poll_interval_ms.max(1);
        let sentinel_poll_ms: u64 = 100;
        let sentinel_call_timeout_ms =
            DEFAULT_BRIDGE_POLL_TIMEOUT_MS.min((lifecycle_poll_ms / 2).max(20));
        let lifecycle_poll = Duration::from_millis(lifecycle_poll_ms);
        let sentinel_poll = Duration::from_millis(sentinel_poll_ms);
        let tick = Duration::from_millis(lifecycle_poll_ms.min(20));

        let mut next_sentinel = Instant::now() + sentinel_poll;
        let mut next_lifecycle = Instant::now();
        let mut last_status_change = Instant::now();
        let mut prev_status = String::new();

        self.log_event(
            "wait_lifecycle",
            format!(
                "target={} pane={} lifecycle_poll={}ms sentinel_poll={}ms sentinel_timeout={}ms",
                target.as_str(),
                producer_pane_id.as_deref().unwrap_or("none"),
                lifecycle_poll_ms,
                sentinel_poll_ms,
                sentinel_call_timeout_ms
            ),
        );

        loop {
            let now = Instant::now();

            // M152 S4: bail on graceful shutdown. The drive loop
            // already polls at iteration boundaries; this is the
            // production wait path (used by SystemDriveOps::integrated_wait
            // — see M150 bridge fast-path). Without this check, a
            // Ctrl-C during a real run blocks until the 30-min stall
            // timeout fires. Same bail pattern as the parallel
            // `wait_for_lifecycle_with` (used by the unit-test path).
            if crate::watch::shutdown_requested() {
                bail!("graceful shutdown requested");
            }

            if now >= next_lifecycle {
                let lifecycle = read_lifecycle_via_mp(&self.mp_bin, &self.project_root, &id)
                    .unwrap_or_default();
                if lifecycle == target.as_str() {
                    return Ok(WaitOutcome::Reached);
                }
                if lifecycle_advanced_past(&lifecycle, target) {
                    return Ok(WaitOutcome::AdvancedPast);
                }

                if let Some(pane) = &liveness_pane {
                    let status = read_agent_status(&self.herdr_bin, pane)
                        .unwrap_or_else(|_| "unknown".to_string());
                    if status != prev_status {
                        prev_status = status;
                        last_status_change = Instant::now();
                    } else if self.wait.stall_timeout_ms > 0 {
                        let elapsed = last_status_change.elapsed();
                        if elapsed >= Duration::from_millis(self.wait.stall_timeout_ms) {
                            bail!(
                                "agent appears hung: agent-status='{}' unchanged for {}ms, \
                                 lifecycle='{}' (target='{}')",
                                prev_status,
                                self.wait.stall_timeout_ms,
                                lifecycle,
                                target.as_str()
                            );
                        }
                    }
                }

                next_lifecycle = Instant::now() + lifecycle_poll;
            }

            let sentinel_now = Instant::now();
            if sentinel_now >= next_sentinel {
                if let Some(pane_id) = &producer_pane_id {
                    // Deadline protection: do not start a sentinel
                    // subprocess if it cannot finish before the next
                    // lifecycle tick. A `pane get` that overruns would
                    // push the lifecycle poll past its deadline. The
                    // margin is the 20ms tick granularity — we re-sleep
                    // at most that long per loop, and the next lifecycle
                    // tick fires within `tick` of the deadline.
                    let budget = next_lifecycle.saturating_duration_since(sentinel_now);
                    if budget >= tick + Duration::from_millis(sentinel_call_timeout_ms) {
                        match read_custom_status_bounded(
                            &self.herdr_bin,
                            pane_id,
                            sentinel_call_timeout_ms,
                        ) {
                            Ok(Some(cs)) if sentinel_matches(&cs) => {
                                self.log_event(
                                    "bridge_sentinel",
                                    format!("observed on pane {pane_id}; confirming lifecycle"),
                                );
                                match read_lifecycle_via_mp(&self.mp_bin, &self.project_root, &id) {
                                    Ok(lifecycle) if lifecycle == target.as_str() => {
                                        let _ = clear_stage_done_sentinel(
                                            &self.herdr_bin,
                                            pane_id,
                                            sentinel_call_timeout_ms,
                                        );
                                        return Ok(WaitOutcome::Reached);
                                    }
                                    Ok(lifecycle)
                                        if lifecycle_advanced_past(&lifecycle, target) =>
                                    {
                                        let _ = clear_stage_done_sentinel(
                                            &self.herdr_bin,
                                            pane_id,
                                            sentinel_call_timeout_ms,
                                        );
                                        return Ok(WaitOutcome::AdvancedPast);
                                    }
                                    Ok(lifecycle) => {
                                        let _ = clear_stage_done_sentinel(
                                            &self.herdr_bin,
                                            pane_id,
                                            sentinel_call_timeout_ms,
                                        );
                                        self.log_event(
                                            "bridge_sentinel",
                                            format!(
                                                "stale sentinel on pane {pane_id}; \
                                                 lifecycle='{lifecycle}'; cleared"
                                            ),
                                        );
                                    }
                                    Err(e) => {
                                        self.log_event(
                                            "bridge_sentinel",
                                            format!("lifecycle confirm failed: {e}"),
                                        );
                                    }
                                }
                            }
                            Ok(_) | Err(_) => {}
                        }
                        next_sentinel = Instant::now() + sentinel_poll;
                    } else {
                        // No time for a safe sentinel poll before the
                        // next lifecycle tick. Skip this round; the
                        // lifecycle poll handles the wait.
                        self.log_event(
                            "bridge_sentinel",
                            format!(
                                "skipped sentinel poll on pane {pane_id}: \
                                 budget={}ms < timeout={}ms + tick={}ms",
                                budget.as_millis(),
                                sentinel_call_timeout_ms,
                                tick.as_millis()
                            ),
                        );
                        next_sentinel = next_lifecycle;
                    }
                } else {
                    next_sentinel = sentinel_now + sentinel_poll;
                }
            }

            // Interruptible tick sleep so a SIGINT lands within
            // ~20ms regardless of where in the loop we are. The
            // top-of-loop flag check + a 20ms granularity tick
            // keeps the shutdown latency well under the nextest
            // per-test timeout window.
            let mut remaining = tick;
            let slice = Duration::from_millis(5);
            while !remaining.is_zero() {
                if crate::watch::shutdown_requested() {
                    bail!("graceful shutdown requested");
                }
                let chunk = remaining.min(slice);
                std::thread::sleep(chunk);
                remaining = remaining.saturating_sub(chunk);
            }
        }
    }
}

impl DriveOps for SystemDriveOps {
    fn log_event(&self, kind: &'static str, message: impl Into<String>) {
        // Delegate to the inherent method on SystemDriveOps so the
        // logger lookup and milestone-id stamping stay in one place.
        SystemDriveOps::log_event(self, kind, message);
    }
    fn plan_dir(&self) -> &Path {
        &self.plan_dir
    }

    fn read_milestone(&mut self) -> Result<MilestoneFile> {
        // We construct a PlanContext-like lookup by loading the file
        // via the milestone io module. The milestone_id is normalized
        // by load_milestone_path internally.
        let id = self
            .active_milestone_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("SystemDriveOps: no active milestone id set"))?;
        // Review finding: discover is re-done per iteration. Cheap
        // (single stat) compared to the herdr round-trip, but a
        // future optimization could cache this on the ops struct.
        let ctx = crate::paths::PlanContext::discover(
            Some(self.plan_dir.clone()),
            Some(self.project_root.clone()),
        )?;
        self.log_event("read_milestone", format!("load {id}"));
        load_milestone_by_id(&ctx, id)
    }

    fn ensure_pane(&mut self, role: Role) -> Result<PaneHandle> {
        if let Some(mut existing) = self.pane_cache.get(&role).cloned() {
            // AC-04: returning from cache counts as reuse — flip the
            // flag so callers can distinguish spawn vs cache-hit.
            existing.reused = true;
            self.log_event(
                "ensure_pane",
                format!("{} pane cache hit → {}", role.label(), existing.pane_id),
            );
            return Ok(existing);
        }
        // ─── M226 F-01 wiring (dispatch dedup on production spawn) ──
        // Before issuing a fresh `herdr agent start`, consult the
        // autopilot session event log for a prior
        // `AssignmentDispatched` event whose `pane_label` matches
        // this role's pane. If one exists, the prior process
        // already delivered this prompt; this process must NOT
        // re-spawn (M225 AC-01). The check mirrors
        // `task_assign::dispatch_assignment`'s dedup short-circuit
        // — the production spawn path now consults the same
        // session event log so the typed contract holds end-to-end.
        if let Some(session_id) = self.session_id.clone() {
            let pane_label = pane_label_for(role, DEFAULT_PANE_N);
            let ctx = PlanContext {
                project_root: self.project_root.clone(),
                plan_dir: self.plan_dir.clone(),
            };
            match crate::autopilot::load_session(&ctx, &session_id) {
                Ok(session) => {
                    let dispatch_key = crate::autopilot::IdempotencyKey::Dispatch {
                        pane_label: pane_label.clone(),
                    };
                    if crate::autopilot::was_already_applied(&session, &dispatch_key) {
                        // Synthesize a "reused" PaneHandle from the
                        // recorded event's stored_pane_id (best
                        // effort — falls back to the label). The
                        // herdr spawn is skipped; downstream stages
                        // observe the cached handle.
                        let stored_pane_id = session
                            .events
                            .iter()
                            .rev()
                            .find(|e| e.kind == crate::autopilot::EventKind::AssignmentDispatched)
                            .and_then(|e| {
                                e.payload
                                    .as_ref()
                                    .and_then(|p| p.get("target_pane"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| pane_label.clone());
                        self.log_event(
                            "dispatch_dedup",
                            format!(
                                "{} AssignmentDispatched already on session; skipping herdr spawn (M226 F-01 / M225 AC-01)",
                                role.label()
                            ),
                        );
                        let handle = PaneHandle {
                            label: pane_label.clone(),
                            pane_id: stored_pane_id,
                            reused: true,
                        };
                        self.pane_cache.insert(role, handle.clone());
                        return Ok(handle);
                    }
                }
                Err(_) => {
                    // No session yet — fall through to the normal
                    // spawn path. This matches the legacy behavior
                    // when an autopilot session is absent (e.g.,
                    // `mp watch` invoked without prior `mp autopilot
                    // session create`).
                }
            }
        }
        let rc = match role {
            Role::Runner => &self.runner_config,
            Role::Coordinator => &self.coordinator_config,
        };
        self.log_event("ensure_pane", format!("{} spawning new pane", role.label()));
        match ensure_pane(
            &self.herdr_bin,
            role,
            crate::watch::DEFAULT_PANE_N,
            rc,
            self.project_root.as_path(),
        ) {
            Ok(handle) => {
                self.pane_cache.insert(role, handle.clone());
                // M178 S2: record the pane id in the v2 control-plane state
                // so `mp watch output` (S7) can address it without scanning
                // the legacy v1 panes array.
                self.transition(crate::watch::WatchTransition::PaneObserved {
                    role,
                    pane_id: handle.pane_id.clone(),
                })?;
                Ok(handle)
            }
            Err(err) => {
                // M197 WP3 / AC-04: emit a structured spawn_error
                // log entry so the operator can see the full argv
                // + stdout + stderr + exit code from a single log
                // line. The state machine re-raises the error so
                // the sequencer can convert it to
                // `RunOutcome::SpawnFailed` and stop the run.
                if let Some(failure) = crate::watch::herdr::extract_spawn_failure(&err) {
                    let entry = crate::watch::WatchLogEntry::new(
                        "spawn_error",
                        format!(
                            "herdr {} failed (exit {:?}): {}",
                            failure.command, failure.exit_code, failure.stderr
                        ),
                    )
                    .role(role.label())
                    .spawn_error(
                        &failure.command,
                        failure.argv.clone(),
                        failure.exit_code,
                        failure.stdout.clone(),
                        failure.stderr.clone(),
                    );
                    if let Some(logger) = self.logger.as_ref() {
                        let _ = logger.log(&entry);
                    }
                }
                Err(err)
            }
        }
    }

    fn send_prompt_to(&mut self, pane: &PaneHandle, text: &str) -> Result<()> {
        self.log_event(
            "send_prompt",
            format!("→ {} ({} chars)", pane.pane_id, text.chars().count()),
        );
        // Track the producer pane so the bridge fast-path polls the
        // pane that actually received the prompt (F-12). Without
        // this, the consumer would compare the ambient HERDR_PANE_ID
        // (the watch process's pane) with the runner/coordinator pane
        // — and mismatch in the normal multi-pane topology.
        self.last_prompt_pane = Some(pane.clone());
        // M178 S2: refresh the v2 active-pane id so the status
        // surface reads through to whatever pane just got the
        // prompt. Resolve the role against the cache before
        // mutably borrowing `run_state`.
        let pane_role = self.pane_cache.iter().find_map(|(r, h)| {
            if h.pane_id == pane.pane_id {
                Some(*r)
            } else {
                None
            }
        });
        let pane_id = pane.pane_id.clone();
        if let Some(role) = pane_role {
            self.transition(crate::watch::WatchTransition::PaneObserved { role, pane_id })?;
        }
        send_prompt(&self.herdr_bin, pane, text, &self.readiness)
    }

    fn wait_for_lifecycle(&mut self, target: LifecycleTarget) -> Result<WaitOutcome> {
        // M152 ext-review F-02 (2026-07-14): record the target so a
        // graceful shutdown can persist the *actual* target the run
        // was waiting for, not a hardcoded placeholder.
        self.current_target = Some(target);
        // M178 S2: keep the v2 control-plane state in sync — the
        // target lifecycle is part of the AC-01 contract.
        // `set_active_stage` durably wrote the matching target before
        // dispatch. Do not create a second in-memory-only mutation here.
        // M150 S3: integrated race — the sentinel is a wake-up that
        // triggers a lifecycle confirm; the lifecycle is the source
        // of truth and drives completion on the configured cadence
        // even when the bridge is silent (F-11 / AC-03).
        self.integrated_wait(target)
    }

    fn record_handoff(&mut self, transition: &str) -> Result<()> {
        // Wired to the structured logger (review finding #11). The
        // hand-off trail lives in the watch log; plan.json lifecycle
        // transitions + AC evidence remain the source of truth for
        // review state. A future step could mirror this to
        // `mp reviews handoff` for the structured trail.
        self.log_event("handoff", format!("transition={transition}"));
        // Capture the inputs before mutably borrowing run_state so
        // the borrow checker is happy.
        let ctx_result =
            PlanContext::discover(Some(self.plan_dir.clone()), Some(self.project_root.clone()));
        let active_id = self.active_milestone_id.clone();
        let new_lifecycle = match (ctx_result, active_id) {
            (Ok(ctx), Some(id)) => load_milestone_by_id(&ctx, &id)
                .ok()
                .map(|m| m.milestone.lifecycle),
            _ => None,
        };
        if let Some(lc) = new_lifecycle {
            self.transition(crate::watch::WatchTransition::LifecycleObserved(lc))?;
        }
        Ok(())
    }

    fn set_active_stage(
        &mut self,
        stage: crate::watch::PromptStage,
        target: crate::watch::LifecycleTarget,
    ) -> Result<()> {
        // M178 external-review F-01: update the v2 control-plane
        // state with the active stage + target so `mp watch-control
        // status` reads them during a live run.
        self.transition(crate::watch::WatchTransition::ActiveStage { stage, target })?;
        Ok(())
    }
}

impl SystemDriveOps {
    /// M178 S2: record a per-milestone outcome on the v2 state and
    /// mark the run terminal if `outcome` is terminal. Used by the
    /// sequencer on every milestone boundary. Lives on the
    /// inherent impl (not the [`DriveOps`] trait) so callers don't
    /// have to thread `WatchRunState` through the trait surface.
    pub(crate) fn record_milestone_outcome(
        &mut self,
        milestone_id: impl Into<String>,
        outcome: RunOutcome,
    ) -> Result<()> {
        let milestone_id = milestone_id.into();
        use crate::watch::MilestoneRunOutcome;
        self.transition(crate::watch::WatchTransition::MilestoneOutcome(
            MilestoneRunOutcome {
                id: milestone_id.clone(),
                outcome: outcome.clone(),
            },
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MilestoneFile, MilestoneMeta};

    fn ms(id: &str, lifecycle: &str, spec: &str, exec: &str) -> MilestoneFile {
        MilestoneFile {
            milestone: MilestoneMeta {
                id: id.to_string(),
                lifecycle: lifecycle.to_string(),
                spec_status: spec.to_string(),
                execution_status: exec.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn skip_returns_none_for_ready_approved_milestone() {
        let m = ms("1", "approved", "ready", "planned");
        assert!(should_skip(&m).is_none());
    }

    #[test]
    fn skip_returns_reason_for_cancelled_deferred_blocked() {
        let mut m = ms("1", "approved", "ready", "planned");
        m.milestone.cancelled = true;
        assert_eq!(should_skip(&m).as_deref(), Some("cancelled"));
        m.milestone.cancelled = false;
        m.milestone.deferred = true;
        assert_eq!(should_skip(&m).as_deref(), Some("deferred"));
        m.milestone.deferred = false;
        m.milestone.blocked = true;
        assert!(should_skip(&m).unwrap().starts_with("blocked"));
    }

    #[test]
    fn skip_returns_reason_when_approved_but_not_ready() {
        let m = ms("1", "approved", "draft", "planned");
        let reason = should_skip(&m).unwrap();
        assert!(
            reason.contains("spec_status=draft"),
            "should mention the offending field: {reason}"
        );
    }

    #[test]
    fn skip_returns_none_for_canonical_drivable_lifecycles_only() {
        assert!(should_skip(&ms("1", "in-progress", "ready", "in-progress")).is_none());
        assert!(should_skip(&ms("1", "remediation", "ready", "in-progress")).is_none());
        assert!(should_skip(&ms("1", "self-reviewed", "ready", "in-progress")).is_some());
        assert!(should_skip(&ms("1", "reviewed", "ready", "complete")).is_some());
    }

    #[test]
    fn skip_rejects_draft_and_unknown_lifecycles() {
        let m = ms("1", "draft", "draft", "planned");
        let reason = should_skip(&m).unwrap();
        assert!(
            reason.contains("lifecycle=draft"),
            "should reject draft: {reason}"
        );
    }

    #[test]
    fn skip_flags_already_complete() {
        let m = ms("1", "complete", "ready", "complete");
        assert_eq!(should_skip(&m).as_deref(), Some("already complete"));
    }

    #[test]
    fn next_stage_maps_approved_to_execute_target_complete() {
        let m = ms("1", "approved", "ready", "planned");
        let plan = next_stage(&m).unwrap();
        assert_eq!(plan.stage, PromptStage::Execute);
        assert_eq!(plan.target, LifecycleTarget::Complete);
    }

    #[test]
    fn next_stage_maps_remediation_to_runner_target_complete() {
        let m = ms("1", "remediation", "ready", "in-progress");
        let plan = next_stage(&m).unwrap();
        assert_eq!(plan.stage, PromptStage::Remediate);
        assert_eq!(plan.target, LifecycleTarget::Complete);
    }

    // A tiny mock DriveOps that returns canned milestones in sequence.
    // The script advances on `wait_for_lifecycle` (the transition
    // marker), not on `send_prompt_to`, so the in-progress case that
    // skips re-prompting still advances the script.
    struct Scripted {
        milestones: Vec<MilestoneFile>,
        prompts_sent: Vec<String>,
        panes_ensured: Vec<Role>,
        handoffs: Vec<String>,
        plan_dir: PathBuf,
        /// M153 S2 MEDIUM-3: in-module events collector. Field is
        /// declared for parity with the `tests/watch_execution.rs`
        /// `Scripted` mock; the in-module `log_event` impl remains
        /// a no-op because the trait signature is `&self` (the
        /// field can't be mutated through `&self` without interior
        /// mutability). When a future test needs in-module event
        /// assertions, swap the in-module mock to `RefCell`-backed
        /// storage \u2014 or use the test-file mock.
        #[allow(dead_code)]
        events: Vec<(&'static str, String)>,
    }

    impl DriveOps for Scripted {
        fn read_milestone(&mut self) -> Result<MilestoneFile> {
            Ok(self
                .milestones
                .first()
                .cloned()
                .unwrap_or_else(|| ms("1", "complete", "ready", "complete")))
        }
        fn ensure_pane(&mut self, role: Role) -> Result<PaneHandle> {
            self.panes_ensured.push(role);
            Ok(PaneHandle {
                label: format!("role-{}-1", role.label()),
                pane_id: format!("role-{}-1", role.label()),
                reused: false,
            })
        }
        fn log_event(&self, _kind: &'static str, _message: impl Into<String>) {
            // Test mock: no-op. The in-module `events` field exists
            // for parity with the test-file mock (MEDIUM-3) but the
            // trait's `&self` signature can't mutate it without
            // interior mutability. Future tests that need in-module
            // event assertions should swap to the `tests/watch_execution.rs`
            // `Scripted` mock which collects events via `RefCell`.
        }
        fn plan_dir(&self) -> &Path {
            &self.plan_dir
        }
        fn send_prompt_to(&mut self, _pane: &PaneHandle, text: &str) -> Result<()> {
            self.prompts_sent.push(text.to_string());
            Ok(())
        }
        fn wait_for_lifecycle(&mut self, _target: LifecycleTarget) -> Result<WaitOutcome> {
            // Advance the script: drop the first milestone so the next
            // read returns the next state.
            if self.milestones.len() > 1 {
                self.milestones.remove(0);
            }
            Ok(WaitOutcome::Reached)
        }
        fn record_handoff(&mut self, transition: &str) -> Result<()> {
            self.handoffs.push(transition.to_string());
            Ok(())
        }
    }

    #[test]
    fn drive_completes_when_milestone_reaches_complete() {
        let mut ops = Scripted {
            milestones: vec![ms("1", "complete", "ready", "complete")],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 10).unwrap();
        assert_eq!(outcome, DriveOutcome::Complete);
        assert!(
            ops.prompts_sent.is_empty(),
            "should not prompt for complete"
        );
    }

    #[test]
    fn drive_skips_when_milestone_not_ready() {
        let mut ops = Scripted {
            milestones: vec![ms("1", "approved", "draft", "planned")],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 10).unwrap();
        match outcome {
            DriveOutcome::Skipped { reason } => assert!(reason.contains("spec_status=draft")),
            other => panic!("expected Skipped, got {other:?}"),
        }
        assert!(ops.prompts_sent.is_empty());
    }

    #[test]
    fn drive_sends_execute_prompt_for_ready_approved() {
        let mut ops = Scripted {
            milestones: vec![
                ms("1", "approved", "ready", "planned"),
                ms("1", "complete", "ready", "complete"),
            ],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 10).unwrap();
        assert_eq!(outcome, DriveOutcome::Complete);
        assert_eq!(
            ops.prompts_sent.len(),
            1,
            "should send exactly one execute prompt"
        );
        assert!(ops.prompts_sent[0].contains("runner"));
        assert!(ops.prompts_sent[0].contains("mp milestone set-status 1 in-progress"));
        assert_eq!(ops.panes_ensured, vec![Role::Runner]);
        assert!(ops.handoffs.iter().any(|h| h.contains("complete")));
    }

    #[test]
    fn drive_remediation_uses_runner_pane() {
        let mut ops = Scripted {
            milestones: vec![
                ms("1", "remediation", "ready", "in-progress"),
                ms("1", "complete", "ready", "complete"),
            ],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 10).unwrap();
        assert_eq!(outcome, DriveOutcome::Complete);
        assert_eq!(ops.panes_ensured, vec![Role::Runner]);
        assert!(ops.prompts_sent[0].contains("runner"));
    }

    #[test]
    fn drive_does_not_re_prompt_when_inprogress() {
        // Mid-execute: the runner is already running. The state machine
        // should poll, not re-prompt.
        let mut ops = Scripted {
            milestones: vec![
                ms("1", "in-progress", "ready", "in-progress"),
                ms("1", "complete", "ready", "complete"),
            ],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 10).unwrap();
        assert_eq!(outcome, DriveOutcome::Complete);
        assert!(
            ops.prompts_sent.is_empty(),
            "in-progress should not re-prompt the runner"
        );
    }

    #[test]
    fn drive_caps_iterations_to_prevent_infinite_loop() {
        // The script never advances past approved; the loop should
        // bail out after max_iterations.
        let mut ops = Scripted {
            milestones: vec![ms("1", "approved", "ready", "planned")],
            prompts_sent: vec![],
            panes_ensured: vec![],
            handoffs: vec![],
            plan_dir: PathBuf::new(),
            events: vec![],
        };
        let outcome = drive_milestone(&mut ops, 3).unwrap();
        match outcome {
            DriveOutcome::MaxIterationsExhausted { iterations } => {
                assert!(iterations <= 3, "should not exceed the cap: {iterations}");
            }
            other => panic!("expected MaxIterationsExhausted, got {other:?}"),
        }
    }
}
