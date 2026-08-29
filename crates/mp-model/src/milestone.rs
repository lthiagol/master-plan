use serde::{Deserialize, Serialize};

// ── Milestone types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneFile {
    pub milestone: MilestoneMeta,
    #[serde(default)]
    pub intent: Intent,
    #[serde(default)]
    pub problem: Problem,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub design_decisions: Vec<DesignDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<OpenQuestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_packages: Vec<WorkPackage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub verification: Verification,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "skip_delta_on_disk")]
    pub delta: MilestoneDelta,
}

/// Linear lifecycle states (M100). A milestone is in EXACTLY ONE of these.
/// Orthogonal overlays (`blocked`, `deferred`, `needs_regrooming`, `cancelled`)
/// are stored as separate fields, not as lifecycle values.
// M100 ER-3: `self-reviewed` and `reviewed` are review-flow states
// recorded via the reviews registry (`mp reviews claim`, `mp reviews
// pass`). They are intentionally NOT produced by milestone transition
// setters (block/unblock/defer/complete/reopen); the reviews subcommand
// owns those rungs. The values remain in `LIFECYCLE_STATES` so readers
// (path_engine, raul views) can key on them when the reviews registry
// reports the corresponding review state and the milestone helper code
// maps via `review_state()` / `crate::reviews::review_state()`. Future
// wiring could collapse them out of the enum entirely if the registry
// becomes authoritative for review flow.
pub const LIFECYCLE_STATES: &[&str] = &[
    "draft",
    "groomed",
    "approved",
    "in-progress",
    "executed",
    "self-reviewed",
    "reviewed",
    "complete",
    "remediation",
];

/// Terminal lifecycle values: lifecycle cannot transition out of these.
pub const LIFECYCLE_TERMINAL: &[&str] = &["complete"];

/// Cancelled is also terminal (overlay-style flag, but treated as a terminal
/// lifecycle value for state-machine purposes).
pub const LIFECYCLE_CANCELLED: &str = "cancelled";

/// Lifecycles the watch driver may pick up as active drive targets.
/// Review-cycle aliases (`self-reviewed` / `reviewed`) are intentionally
/// absent because the reviews registry owns those rungs.
pub const WATCH_DRIVABLE_LIFECYCLES: &[&str] = &["approved", "in-progress", "remediation"];

/// True when `lifecycle` is one the watch driver will drive.
pub fn is_watch_drivable_lifecycle(lifecycle: &str) -> bool {
    WATCH_DRIVABLE_LIFECYCLES.contains(&lifecycle)
}

/// Canonical delivery phase. Review-cycle rungs are deliberately absent:
/// `self-reviewed` and `reviewed` are legacy read/migration aliases owned by
/// the reviews registry, not destinations in the active state machine.
///
/// M196: `Done` was renamed to `Executed` so the executor's end-state
/// (`milestone complete` before review) is clearly distinct from the
/// terminal-reviewed state (`complete`). The serde rename is unchanged
/// (`kebab-case` → "done" on disk), but the in-memory variant name and
/// the canonical string are now `Executed` / `"executed"`. Reading the
/// legacy `"done"` string is preserved via `from_lifecycle` for the
/// migration window; the on-disk write always emits `"executed"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MilestonePhase {
    Draft,
    Groomed,
    Approved,
    InProgress,
    Executed,
    Complete,
    Remediation,
}

impl MilestonePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Groomed => "groomed",
            Self::Approved => "approved",
            Self::InProgress => "in-progress",
            Self::Executed => "executed",
            Self::Complete => "complete",
            Self::Remediation => "remediation",
        }
    }

    /// Parse canonical phases plus legacy review aliases. Aliases collapse to
    /// their delivery phase; review state itself remains in reviews.json.
    ///
    /// M196: the legacy `"done"` string is accepted as an alias for
    /// `Executed` so the migration can re-parse pre-rename milestone files
    /// before rewriting them. New writes always emit `"executed"`.
    pub fn from_lifecycle(value: &str) -> Result<Self, String> {
        match value {
            "" | "draft" => Ok(Self::Draft),
            "groomed" => Ok(Self::Groomed),
            "approved" => Ok(Self::Approved),
            "in-progress" => Ok(Self::InProgress),
            // M196: `"done"` is the legacy alias for the executor's
            // end-state; the canonical name is `"executed"`. Both parse
            // to the same phase. `"self-reviewed"` is a review-flow
            // alias that also collapses here.
            "done" | "executed" | "self-reviewed" => Ok(Self::Executed),
            "reviewed" | "complete" => Ok(Self::Complete),
            "remediation" => Ok(Self::Remediation),
            other => Err(format!("unknown milestone lifecycle {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneOverlays {
    pub blocked: bool,
    pub deferred: bool,
    pub cancelled: bool,
    pub needs_regrooming: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneState {
    pub phase: MilestonePhase,
    pub overlays: MilestoneOverlays,
    pub remediation_pre_state: Option<MilestonePhase>,
}

impl MilestoneState {
    pub fn from_meta(meta: &MilestoneMeta) -> Result<Self, String> {
        // M174 followup: derive the underlying phase, not the
        // effective_lifecycle string. `effective_lifecycle` is the
        // *display* surface and returns `"cancelled"` when the overlay
        // is set (so the TUI Milestones lane shows "cancelled" instead
        // of the underlying phase); `MilestonePhase::from_lifecycle`
        // rejects `"cancelled"` because the overlay is orthogonal to
        // the phase — see `MilestoneState`. Without this split,
        // every transition against a cancelled milestone
        // (migrate, unblock, block, …) panicked at
        // `from_lifecycle("cancelled")`. We compute the phase from
        // the lifecycle field + legacy spec/exec fallback, then read
        // the overlay flags separately.
        let phase_str = if !meta.lifecycle.is_empty() && meta.lifecycle != "draft" {
            meta.lifecycle.clone()
        } else if !meta.execution_status.is_empty() || !meta.spec_status.is_empty() {
            // Legacy-shaped milestone (post-migration-window): derive
            // the phase from the legacy spec/exec fields the same way
            // `effective_lifecycle` does, but WITHOUT the cancelled
            // overlay short-circuit.
            let from_spec = legacy_spec_status_to_lifecycle(&meta.spec_status);
            let from_exec = legacy_execution_status_to_lifecycle(&meta.execution_status);
            // Pick the more-advanced value. `executed` wins over
            // `approved`; `in-progress` wins over `approved` (the
            // legacy max() mapping; M100 ER-7 handles the verified
            // + in-progress edge).
            legacy_max_phase(from_spec, from_exec)
        } else {
            "draft".to_string()
        };
        Ok(Self {
            phase: MilestonePhase::from_lifecycle(&phase_str)?,
            overlays: MilestoneOverlays {
                blocked: meta.blocked,
                deferred: meta.deferred,
                cancelled: meta.cancelled,
                needs_regrooming: meta.needs_regrooming,
            },
            remediation_pre_state: meta
                .remediation_pre_state
                .as_deref()
                .map(MilestonePhase::from_lifecycle)
                .transpose()?,
        })
    }
}

/// Pick the more-advanced of two legacy-derived lifecycle strings.
/// Mirrors the `max()` mapping in `effective_lifecycle` but operates
/// on plain phase strings (no overlay short-circuit). Used by
/// `MilestoneState::from_meta` for legacy-shaped milestones where
/// `effective_lifecycle` would return `"cancelled"` (the overlay
/// short-circuit) and confuse the phase parser.
fn legacy_max_phase(a: &str, b: &str) -> String {
    // The legacy ordering: draft < groomed < approved < in-progress <
    // executed < reviewed < complete. Both strings are guaranteed to
    // be valid phase strings (returned by `legacy_*_to_lifecycle`).
    fn rank(s: &str) -> u8 {
        match s {
            "draft" => 0,
            "groomed" => 1,
            "approved" => 2,
            "in-progress" => 3,
            "executed" | "self-reviewed" => 4,
            "reviewed" => 5,
            "complete" => 6,
            _ => 0,
        }
    }
    if rank(b) > rank(a) {
        b.to_string()
    } else {
        a.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneEvent {
    /// Recompute legacy projections without changing canonical state.
    Sync,
    Groom,
    Approve,
    Start,
    FinishExecution,
    Complete,
    Reopen,
    Block,
    Unblock,
    Defer,
    Resume,
    Cancel,
    SetNeedsRegrooming(bool),
    EnterRemediation,
    ExitRemediation,
    /// Explicit compatibility escape hatch for migration code only.
    MigrateRaw(MilestonePhase),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionContext {
    /// The durable executor has already run event-specific gates.
    pub gates_satisfied: bool,
}

impl Default for TransitionContext {
    fn default() -> Self {
        Self {
            gates_satisfied: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionEffects {
    pub phase: MilestonePhase,
    pub overlays: MilestoneOverlays,
    pub remediation_pre_state: Option<MilestonePhase>,
    pub spec_status: &'static str,
    pub execution_status: &'static str,
    pub phase_changed: bool,
}

/// Pure lifecycle transition table. It owns source/destination checks,
/// overlay consistency, remediation restoration, and legacy projections;
/// callers in `mp` remain responsible for durable writes and timestamps.
pub fn transition(
    current: &MilestoneState,
    event: MilestoneEvent,
    context: TransitionContext,
) -> Result<TransitionEffects, String> {
    if !context.gates_satisfied {
        return Err(format!("gates not satisfied for {event:?}"));
    }
    if current.overlays.cancelled
        && !matches!(
            event,
            MilestoneEvent::MigrateRaw(_) | MilestoneEvent::Unblock
        )
    {
        return Err("cancelled milestone is terminal".to_string());
    }

    let mut phase = current.phase;
    let mut overlays = current.overlays;
    let mut remediation_pre_state = current.remediation_pre_state;
    match event {
        MilestoneEvent::Sync => {}
        MilestoneEvent::Groom
            if matches!(phase, MilestonePhase::Draft | MilestonePhase::Groomed) =>
        {
            phase = MilestonePhase::Groomed;
        }
        MilestoneEvent::Approve
            if matches!(
                phase,
                MilestonePhase::Draft | MilestonePhase::Groomed | MilestonePhase::Approved
            ) =>
        {
            phase = MilestonePhase::Approved;
        }
        MilestoneEvent::Start
            if matches!(phase, MilestonePhase::Approved | MilestonePhase::InProgress) =>
        {
            phase = MilestonePhase::InProgress;
            overlays.blocked = false;
            overlays.deferred = false;
        }
        MilestoneEvent::FinishExecution
            if matches!(
                phase,
                MilestonePhase::Approved | MilestonePhase::InProgress | MilestonePhase::Executed
            ) =>
        {
            // M196: the executor's end-state. Distinct from `Complete`
            // (which requires independent review). `FinishExecution` is
            // what `mp milestone complete` emits when the review gate
            // refuses to promote a non-track milestone — the work is
            // finished but the lifecycle stays at `executed` until a
            // reviewer passes it.
            phase = MilestonePhase::Executed;
        }
        MilestoneEvent::Complete
            if matches!(
                phase,
                MilestonePhase::Approved
                    | MilestonePhase::InProgress
                    | MilestonePhase::Executed
                    | MilestonePhase::Complete
            ) =>
        {
            phase = MilestonePhase::Complete;
            overlays.blocked = false;
            overlays.deferred = false;
        }
        MilestoneEvent::Reopen
            if matches!(phase, MilestonePhase::Executed | MilestonePhase::Complete) =>
        {
            phase = MilestonePhase::InProgress;
            overlays.blocked = false;
            overlays.deferred = false;
            remediation_pre_state = None;
        }
        // Block/Cancel on Complete would leave terminal delivery + overlay
        // drift (lifecycle stays complete while exec becomes blocked/cancelled).
        // Mirrors Defer's Complete guard.
        MilestoneEvent::Block if phase != MilestonePhase::Complete => {
            overlays.blocked = true;
            overlays.deferred = false;
        }
        MilestoneEvent::Unblock if overlays.blocked => {
            overlays.blocked = false;
        }
        MilestoneEvent::Defer if phase != MilestonePhase::Complete => {
            overlays.deferred = true;
            overlays.blocked = false;
        }
        MilestoneEvent::Resume if overlays.deferred => {
            overlays.deferred = false;
        }
        MilestoneEvent::Cancel if phase != MilestonePhase::Complete => {
            overlays.cancelled = true;
            overlays.blocked = false;
            overlays.deferred = false;
        }
        MilestoneEvent::SetNeedsRegrooming(value) => overlays.needs_regrooming = value,
        MilestoneEvent::EnterRemediation
            if matches!(phase, MilestonePhase::Executed | MilestonePhase::Complete) =>
        {
            remediation_pre_state = Some(phase);
            phase = MilestonePhase::Remediation;
        }
        MilestoneEvent::ExitRemediation if phase == MilestonePhase::Remediation => {
            phase = remediation_pre_state
                .ok_or_else(|| "remediation exit requires captured pre-state".to_string())?;
            if !matches!(phase, MilestonePhase::Executed | MilestonePhase::Complete) {
                return Err(format!(
                    "invalid remediation pre-state {:?}",
                    phase.as_str()
                ));
            }
            remediation_pre_state = None;
        }
        MilestoneEvent::MigrateRaw(target) => {
            phase = target;
            remediation_pre_state = None;
        }
        _ => {
            return Err(format!(
                "invalid milestone transition: {} + {event:?}",
                current.phase.as_str()
            ));
        }
    }

    if overlays.blocked && overlays.deferred {
        return Err("blocked and deferred overlays are mutually exclusive".to_string());
    }
    let spec_status = lifecycle_to_legacy_spec_status(phase.as_str());
    let execution_status = if overlays.cancelled {
        "cancelled"
    } else if overlays.blocked {
        "blocked"
    } else if overlays.deferred {
        "deferred"
    } else {
        lifecycle_to_legacy_execution_status(phase.as_str())
    };
    Ok(TransitionEffects {
        phase,
        overlays,
        remediation_pre_state,
        spec_status,
        execution_status,
        phase_changed: phase != current.phase,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MilestoneMeta {
    pub id: String,
    pub title: String,
    pub slug: String,
    /// Linear lifecycle state (M100). Replaces spec_status + execution_status.
    /// One of: draft, groomed, approved, in-progress, done, self-reviewed,
    /// reviewed, complete, remediation.
    #[serde(default = "default_lifecycle")]
    pub lifecycle: String,
    /// Kept for read-only backward compatibility during M100 migration.
    /// New code should read `lifecycle` and map via `legacy lifecycle mapping`.
    /// Removed in a later milestone once all readers are converted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spec_status: String,
    /// Kept for read-only backward compatibility during M100 migration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub execution_status: String,
    /// Orthogonal overlay (M100): milestone is blocked. Retains `blocked_at` /
    /// `block_reason` / `blocked_by` semantics from the legacy model.
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub needs_regrooming: bool,
    #[serde(default)]
    pub cancelled: bool,
    /// M174 fix: ISO-8601 timestamp the milestone was cancelled
    /// (e.g. `2026-07-15T17:49:50+00:00`). Optional audit field;
    /// `None` for milestones that were never cancelled or were
    /// cancelled before this field landed. Surfaces in the
    /// Milestones lane and the on-disk JSON so a reader can tell
    /// *when* the cancellation happened without consulting the
    /// dogfood log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<String>,
    /// M174 fix: free-text reason the milestone was cancelled
    /// (e.g. `"work shipped via M169-rev with different design; see
    /// dogfood log Entry 31"`). Same lifecycle as `cancelled_at` —
    /// both are optional audit fields that travel with the
    /// milestone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<String>,
    /// Optional deferred overlay (orthogonal).
    #[serde(default)]
    pub deferred: bool,
    #[serde(default)]
    pub deferred_reason: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub effort: String,
    pub risk: String,
    #[serde(default)]
    pub change_kind: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub created: String,
    pub updated: String,
    #[serde(default)]
    pub blocked_at: String,
    #[serde(default)]
    pub block_reason: String,
    #[serde(default)]
    pub blocked_by: String,
    #[serde(default)]
    pub target_version: String,
    #[serde(default)]
    pub executed_by: String,
    /// Lifecycle captured when remediation begins so exit restores the exact
    /// pre-remediation state independent of finding resolution order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation_pre_state: Option<String>,
    /// M144: RFC3339 timestamp of the last lifecycle transition. Set by
    /// every site that writes `m.milestone.lifecycle = ...` so the TUI's
    /// "since" column can render a relative time ("3d ago"). Omit when
    /// unset (`skip_serializing_if = "Option::is_none"`), keeping healthy
    /// milestone JSON byte-identical to pre-M144.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_at: Option<String>,
}

fn default_lifecycle() -> String {
    "draft".to_string()
}

/// Map a legacy spec_status value to its M100 lifecycle equivalent.
/// Used by migration + read paths during the transition window.
///
/// M196: the executor's end-state was renamed from `done` to `executed`.
/// `legacy_spec_status_to_lifecycle("implemented")` now returns the
/// canonical lifecycle string `"executed"` (legacy `done` is also
/// accepted as an alias for `executed` by `MilestonePhase::from_lifecycle`
/// during the migration window).
pub fn legacy_spec_status_to_lifecycle(legacy: &str) -> &'static str {
    match legacy {
        "draft" => "draft",
        "interview" => "groomed",
        "review" => "groomed",
        "ready" => "approved",
        "implemented" => "executed",
        "verified" => "complete",
        _ => "draft",
    }
}

/// M146: inverse mapping — derive the legacy `spec_status` view from
/// the canonical `lifecycle` value. Used by the new `set-lifecycle`
/// mutator (and its bulk variant) to keep `spec_status` in sync
/// with `lifecycle` on every transition. Mirrors
/// `legacy_spec_status_to_lifecycle` but goes the other way: pre-M146
/// the only mutator that wrote `lifecycle` was the workflow handler
/// (`set-spec-status`), and it did the spec-side alias via
/// `m.milestone.spec_status = status.to_string()` (preserving the
/// input value rather than re-deriving). `set-lifecycle` is the first
/// mutator that *starts* from lifecycle and re-derives spec, so the
/// mapping lives in the model layer for symmetry with the existing
/// reverse helper.
///
/// M196: `executed`, `self-reviewed`, and `remediation` collapse to the
/// pre-M100 spec-side "implemented" bucket. The legacy `done` string is
/// kept as an alias during the migration window so a half-migrated
/// milestone that hasn't yet been rewritten still derives its spec-side
/// view correctly.
pub fn lifecycle_to_legacy_spec_status(lifecycle: &str) -> &'static str {
    match lifecycle {
        "draft" => "draft",
        "groomed" => "review",
        "approved" => "ready",
        // M100 ER-7: execution_status=in-progress implies spec=ready.
        // The lifecycle-side `in-progress` is the same state, so the
        // spec-side alias also lands on "ready" (the spec has been
        // approved and is being executed; the spec hasn't been
        // re-approved mid-execution).
        "in-progress" => "ready",
        // The "spec implemented" bucket — work is done but review is
        // pending. Both `executed` and `remediation` land here because
        // `remediation` is a state where the work was finished and is
        // being re-opened to address findings (the spec side
        // therefore still says "implemented, not yet verified").
        // `done` and `self-reviewed` are the legacy aliases still
        // accepted during the migration window.
        "done" | "executed" | "self-reviewed" | "remediation" => "implemented",
        // The "spec verified" bucket — terminal. `reviewed` and
        // `complete` both land here because `reviewed` is the
        // "external review passed, no findings" pre-`complete` state.
        "reviewed" | "complete" => "verified",
        _ => "draft",
    }
}

/// Map a legacy `execution_status` value to its canonical lifecycle equivalent.
///
/// M196: legacy `execution_status: "done"` meant "execution finished,
/// awaiting review/verification" — NOT the terminal `complete` state. The
/// lifecycle separates these: `executed` (work finished) → `self-reviewed`
/// → `reviewed` → `complete` (terminal, requires verified spec). The
/// rename on the lifecycle side (`done` → `executed`) does not change
/// the meaning of the legacy `execution_status` value; the legacy
/// execution_status `"done"` still maps to the executor's end-state,
/// which is now the canonical lifecycle string `"executed"`.
pub fn legacy_execution_status_to_lifecycle(legacy: &str) -> &'static str {
    match legacy {
        "planned" => "draft",
        "in-progress" => "in-progress",
        "done" => "executed",
        "blocked" => "draft",  // blocked is now an overlay, not a lifecycle
        "deferred" => "draft", // deferred is now an overlay, not a lifecycle
        "cancelled" => "cancelled",
        _ => "draft",
    }
}

/// M146: inverse mapping — derive the legacy `execution_status` view
/// from the canonical `lifecycle` value. Used by the new
/// `set-lifecycle` mutator (and its bulk variant) to keep
/// `execution_status` in sync with `lifecycle` on every transition.
///
/// The lifecycle-driven execution states partition cleanly:
///   * the planning states (draft, groomed, approved) all map to
///     "planned" — work hasn't started.
///   * `in-progress` and `remediation` map to "in-progress" — work is
///     actively under way. `remediation` is treated as still in
///     flight because the milestone is being re-opened to address
///     findings (per the M131 contract), and the legacy execution
///     side records that with `in-progress` rather than `done` so
///     the plan-health view's "active" bucket stays correct.
///   * `done` and `self-reviewed` map to "done" — work is finished
///     and waiting on review.
///   * `reviewed` and `complete` map to "done" too — terminal
///     states. The "done" alias is what the pre-M100 spec_status
///     "verified" bucket read as well (the legacy spec-exec pair had
///     `execution_status=done` mean "execution finished" — which
///     persisted as the review-flow backstop).
///
/// The blocked / deferred / cancelled overlays are NOT derived here
/// because they are orthogonal booleans (`blocked`, `deferred`,
/// `cancelled`) on the milestone meta. Setters that put the
/// milestone into a blocked / deferred / cancelled state (e.g.
/// `mp milestone block` / `defer`) flip the overlay AND the
/// `execution_status` via their own dedicated mutators; this
/// canonical lifecycle writer intentionally doesn't touch the
/// overlays because the user is asserting a lifecycle value (and
/// overlays are user-driven states, not lifecycle stages).
pub fn lifecycle_to_legacy_execution_status(lifecycle: &str) -> &'static str {
    match lifecycle {
        "draft" | "groomed" | "approved" => "planned",
        "in-progress" | "remediation" => "in-progress",
        // M196: the lifecycle-side rename (`done` → `executed`) does
        // NOT change the execution-side projection. `execution_status`
        // is a binary "is this still active or not?" field; the
        // "implemented" vs "verified" distinction lives on the spec
        // side. `executed` / `self-reviewed` / `reviewed` / `complete`
        // (and the legacy `done` alias) all project to the legacy
        // execution_status `"done"` — backward-compat preserved for
        // consumers that still read `execution_status`.
        "done" | "executed" | "self-reviewed" | "reviewed" | "complete" => "done",
        _ => "planned",
    }
}

/// M100 ER-9 / M1 remediation: derive a lifecycle value from legacy
/// spec/exec strings alone, without loading the full milestone. Mirrors
/// the order-rank combination logic in `effective_lifecycle` so the
/// migrated plan.json index entries match the read-side derivation.
/// Spec-side dominates for verified/done; exec-side dominates for
/// the in-progress short-circuit (ER-7).
///
/// M196: the canonical `executed` value and the legacy alias `done`
/// share the same order rank (4) so the rank table is unchanged in
/// behavior — a half-migrated milestone still picks the correct
/// lifecycle when both pre- and post-rename strings are present.
pub fn effective_lifecycle_from_legacy(spec: &str, exec: &str) -> String {
    if exec == "in-progress" {
        return "in-progress".to_string();
    }
    let from_exec = legacy_execution_status_to_lifecycle(exec);
    let from_spec = legacy_spec_status_to_lifecycle(spec);
    let order = |s: &str| -> u8 {
        match s {
            "draft" => 0,
            "groomed" => 1,
            "approved" => 2,
            "in-progress" => 3,
            // M196: `executed` (canonical) and `done` (legacy alias)
            // share rank 4 — the executor's end-state sits between
            // `in-progress` and `self-reviewed` in the advancement
            // order. The two strings are interchangeable for the
            // purpose of `effective_lifecycle` ranking.
            "done" | "executed" => 4,
            "self-reviewed" => 5,
            "reviewed" => 6,
            "complete" => 7,
            "remediation" => 4,
            _ => 0,
        }
    };
    let best = if order(from_exec) >= order(from_spec) {
        from_exec
    } else {
        from_spec
    };
    best.to_string()
}

/// Read-side helper: derive a lifecycle value when only legacy fields are present.
/// Used by every reader during the M100 transition. After migration, `lifecycle`
/// is always populated and this helper falls through to the first branch.
///
/// M196: the rank table now includes both canonical `"executed"` and
/// the legacy `"done"` alias at the same rank (4). The rest of the
/// logic is unchanged.
///
/// IMPORTANT: `MilestoneMeta.lifecycle` defaults to "draft" via serde, so the
/// raw field is never *empty* after deserialization. To distinguish a real
/// "draft" from the default-applied sentinel, this helper checks both the
/// lifecycle field AND the legacy fields. If lifecycle is the default ("draft")
/// and at least one legacy field is set, the milestone is on the legacy shape
/// and we derive the lifecycle from the legacy values.
pub fn effective_lifecycle(meta: &MilestoneMeta) -> String {
    // M174 fix: the cancellation overlay is terminal — once a
    // milestone is cancelled, the lifecycle column should read
    // `cancelled` regardless of where the milestone was when it
    // was cancelled. Without this, a milestone cancelled at
    // `lifecycle=approved` keeps showing `approved` in the TUI
    // Milestones lane, which is misleading (a cancelled milestone
    // is not approved; it was *closed* before it ran). The legacy
    // max() mapping below still picks the right value for the
    // pre-cancellation migration path; the overlay check here
    // simply short-circuits before that.
    if meta.cancelled {
        return crate::milestone::LIFECYCLE_CANCELLED.to_string();
    }
    // If lifecycle is set to something other than the default ("draft"), trust it.
    // The check below ("draft" + legacy fields) catches the case where the
    // milestone was migrated already (lifecycle="draft" means a real draft)
    // vs the case where serde filled in the default for a legacy file.
    if !meta.lifecycle.is_empty() && meta.lifecycle != "draft" {
        return meta.lifecycle.clone();
    }
    // If lifecycle is "draft" but legacy fields are populated, derive.
    if !meta.execution_status.is_empty() || !meta.spec_status.is_empty() {
        // M100 ER-7: when exec-side is `in-progress`, the milestone is
        // actively being built. The legacy max() mapping folded
        // `verified + in-progress → complete` (semantically wrong — a
        // verified spec during active execution is not terminal).
        // Execution stage dominates once started, so the lifecycle
        // stays `in-progress` regardless of spec-side progress.
        if meta.execution_status == "in-progress" {
            return "in-progress".to_string();
        }
        let from_exec = legacy_execution_status_to_lifecycle(&meta.execution_status);
        let from_spec = legacy_spec_status_to_lifecycle(&meta.spec_status);
        let order = |s: &str| -> u8 {
            match s {
                "draft" => 0,
                "groomed" => 1,
                "approved" => 2,
                "in-progress" => 3,
                "done" | "executed" => 4,
                "self-reviewed" => 5,
                "reviewed" => 6,
                "complete" => 7,
                "remediation" => 4,
                _ => 0,
            }
        };
        let best = if order(from_exec) >= order(from_spec) {
            from_exec
        } else {
            from_spec
        };
        return best.to_string();
    }
    // Default: lifecycle was actually "draft" (no legacy fields set).
    meta.lifecycle.clone()
}

fn default_priority() -> String {
    "normal".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Intent {
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Problem {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub given: String,
    pub when: String,
    pub then: String,
    #[serde(default)]
    pub independent_test: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InterfaceSpec {
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    #[serde(default)]
    pub config_keys: Vec<ConfigKey>,
    #[serde(default)]
    pub cli_commands: Vec<CliCommand>,
    #[serde(default)]
    pub entities: Vec<Entity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigKey {
    pub key: String,
    pub r#type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliCommand {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entity {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Scope {
    #[serde(default)]
    pub in_scope: Vec<String>,
    #[serde(default)]
    pub out_of_scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    pub verification: String,
    pub status: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DesignDecision {
    pub area: String,
    pub choice: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    pub status: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkPackage {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub rollback: String,
    /// Legacy on-disk shape; merged into top-level `steps` on load.
    #[serde(default, skip_serializing)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Step {
    pub id: String,
    #[serde(default)]
    pub work_package: String,
    #[serde(default)]
    pub order: u32,
    pub action: String,
    #[serde(default)]
    pub files: Vec<String>,
    pub tests: String,
    pub done_when: String,
    pub status: String,
    #[serde(default)]
    pub covers_ac: Vec<String>,
    #[serde(default)]
    pub depends_on_steps: Vec<String>,
    #[serde(default)]
    pub claimed_by: String,
    #[serde(default)]
    pub claimed_at: String,
    #[serde(default)]
    pub lease_expires_at: String,
    /// Per-step run evidence (last successful verification note). M111 fragment
    /// ergonomics — agents edit this via `mp milestone step update --evidence`,
    /// not by rebuilding the milestone document.
    #[serde(default)]
    pub evidence: String,
}

impl Default for MilestoneFile {
    fn default() -> Self {
        Self {
            milestone: MilestoneMeta {
                id: String::new(),
                title: String::new(),
                slug: String::new(),
                lifecycle: "draft".to_string(),
                lifecycle_at: None,
                spec_status: String::new(),
                execution_status: String::new(),
                blocked: false,
                needs_regrooming: false,
                cancelled: false,
                cancelled_at: None,
                cancel_reason: None,
                deferred: false,
                deferred_reason: String::new(),
                depends_on: vec![],
                effort: "S".to_string(),
                risk: "low".to_string(),
                change_kind: String::new(),
                priority: "normal".to_string(),
                created: String::new(),
                updated: String::new(),
                blocked_at: String::new(),
                block_reason: String::new(),
                blocked_by: String::new(),
                target_version: String::new(),
                executed_by: String::new(),
                remediation_pre_state: None,
            },
            intent: Intent::default(),
            problem: Problem::default(),
            scope: Scope::default(),
            acceptance_criteria: vec![],
            design_decisions: vec![],
            open_questions: vec![],
            work_packages: vec![],
            steps: vec![],
            verification: Default::default(),
            findings: vec![],
            delta: MilestoneDelta::default(),
        }
    }
}

fn skip_delta_on_disk(delta: &MilestoneDelta) -> bool {
    !delta.is_set()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Verification {
    pub date: String,
    pub branch: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: String,
    pub category: String,
    pub description: String,
    pub status: String,
    pub author: String,
    pub fixed_in: String,
    pub created: String,
    pub resolved: String,
    // Optional review phase. Empty values remain valid legacy self-review work.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phase: String,
    // M101: hunk-shaped anchor (optional). All sub-fields are optional —
    // a finding without an anchor behaves as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<FindingAnchor>,
    // M101: reviewer↔executor conversation thread.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread: Vec<FindingThreadEntry>,
    // M101: hunk-compatible metadata.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rationale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confidence: String, // low | medium | high
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// M101: optional hunk-shaped anchor for a finding. Mirrors hunk's
/// AgentAnnotation range shape (path, commit, new_range, old_range, hunk_index,
/// side) so a future hunk integration (ID-19) is a transform-free export.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingAnchor {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_range: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_range: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunk_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start_line: u32,
    pub end_line: u32,
}

impl Range {
    /// M101 R3: start_line must be <= end_line (no inverted ranges).
    /// The model doesn't enforce this on construction because
    /// deserialization needs to round-trip arbitrary on-disk shapes
    /// (legacy fixtures); validation runs on the write path.
    pub fn validate(&self) -> Result<(), String> {
        if self.start_line > self.end_line {
            return Err(format!(
                "Range inverted: start_line={} > end_line={}",
                self.start_line, self.end_line
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingThreadEntry {
    pub author: String,
    pub at: String, // RFC3339 timestamp
    pub body: String,
}

impl FindingThreadEntry {
    /// Validate the supported timestamp syntax when non-empty. The parser
    /// accepts `T` or space separators and validates calendar/time ranges.
    /// Empty remains the legacy "no timestamp recorded" sentinel.
    pub fn validate(&self) -> Result<(), String> {
        if self.at.is_empty() {
            return Ok(());
        }
        if !is_rfc3339(&self.at) {
            return Err(format!(
                "FindingThreadEntry.at is not RFC3339: {:?}",
                self.at
            ));
        }
        Ok(())
    }
}

fn is_rfc3339(s: &str) -> bool {
    crate::parse_rfc3339(s).is_ok()
}

/// M101 R3: anchor.side validator. Mirrors `is_valid_confidence` shape:
/// accepts only the documented side values (`old`, `new`); empty string
/// passes through (no side recorded).
pub fn is_valid_side(value: &str) -> bool {
    value.is_empty() || value == "old" || value == "new"
}

/// M101: phase values for findings.
pub const FINDING_PHASE_SELF: &str = "self";
pub const FINDING_PHASE_EXTERNAL: &str = "external";

/// M101 R4: FindingDraft — a single struct that bundles all the per-
/// finding knobs the model exposes, so `add_finding` callers stop
/// threading 8 positional parameters through a signature that already
/// outgrew its name. The CLI handler in `crates/mp/src/commands/reviews.rs`
/// builds one of these from the new --phase / --anchor / --summary /
/// --rationale / --confidence / --tags flags.
#[derive(Debug, Clone, Default)]
pub struct FindingDraft {
    pub milestone_id: String,
    pub severity: String,
    pub category: String,
    pub description: String,
    pub author: String,
    pub phase: String,
    pub summary: String,
    pub rationale: String,
    pub confidence: String,
    pub tags: Vec<String>,
    pub anchor: Option<FindingAnchor>,
    pub thread: Vec<FindingThreadEntry>,
}

impl FindingDraft {
    pub fn validate(&self) -> Result<(), String> {
        if !["high", "medium", "low"].contains(&self.severity.as_str()) {
            return Err(format!("invalid severity: {:?}", self.severity));
        }
        if !self.phase.is_empty()
            && !matches!(
                self.phase.as_str(),
                FINDING_PHASE_SELF | FINDING_PHASE_EXTERNAL
            )
        {
            return Err(format!("invalid phase: {:?}", self.phase));
        }
        if !is_valid_confidence(&self.confidence) {
            return Err(format!("invalid confidence: {:?}", self.confidence));
        }
        if let Some(a) = &self.anchor {
            if !is_valid_side(a.side.as_deref().unwrap_or("")) {
                return Err(format!("invalid anchor side: {:?}", a.side));
            }
            if let Some(r) = &a.new_range {
                if let Err(msg) = r.validate() {
                    return Err(format!("anchor new_range: {msg}"));
                }
            }
            if let Some(r) = &a.old_range {
                if let Err(msg) = r.validate() {
                    return Err(format!("anchor old_range: {msg}"));
                }
            }
        }
        for entry in &self.thread {
            if let Err(msg) = entry.validate() {
                return Err(format!("thread entry: {msg}"));
            }
        }
        Ok(())
    }
}

/// Accept documented confidence values. Empty is valid because it represents
/// an authored absence and is omitted during serialization.
pub fn is_valid_confidence(value: &str) -> bool {
    matches!(value, "low" | "medium" | "high") || value.is_empty()
}

/// M133: validate that a string is shaped like a finding id (`F-01`,
/// `F-42`, …). Mirrors `next_finding_id`'s strip+parse step. Returns
/// false for empty strings, strings without the `F-` prefix, and
/// strings where the suffix is not all-digit (`F-` alone is rejected).
pub fn is_valid_finding_id(value: &str) -> bool {
    value.len() > 2 && value.starts_with("F-") && value[2..].chars().all(|c| c.is_ascii_digit())
}

// ── M133: review comments and handoffs (BF-06) ───────────────────────────────

/// M133 AC-01: a review comment on a milestone. Carries the author, body,
/// optional finding link (so a comment can anchor on a specific finding
/// like `F-03`), and an RFC3339 timestamp. Persisted in `reviews.json`
/// alongside the existing review-verdict records so one durable trail
/// per milestone covers both the verdict-driven and the conversation-
/// driven review surfaces.
///
/// M154 AC-02: also carries an optional [`FindingAnchor`] so a comment
/// can attach to a file location (the same shape hunk consumes).
/// `#[serde(default, skip_serializing_if = "Option::is_none")]` keeps
/// pre-M154 comment records round-trip-clean: an absent anchor field
/// on disk deserializes to `None` and a `None` anchor doesn't write a
/// noisy `"anchor": null` field back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub milestone_id: String,
    pub author: String,
    pub body: String,
    /// Optional link to a finding id (`F-NN`). Empty string = unlinked.
    /// We use `skip_serializing_if = "String::is_empty"` so an unlinked
    /// comment does not surface a noisy `"finding_id": ""` field — the
    /// default-equal-empty invariant keeps on-disk shape compact.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub finding_id: String,
    /// RFC3339 timestamp. The CLI auto-fills with `store::now_rfc3339()`
    /// but accepts a `--at <rfc3339>` override for backfill / replay.
    pub created_at: String,
    /// M154 AC-02: optional file/line/side anchor so the comment can be
    /// exported to hunk alongside findings. Absent on legacy records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<FindingAnchor>,
}

/// M133 AC-02: a coordinator/runner hand-off record. Mirrors the
/// four-point hand-off protocol documented in `mp-flow`'s Hand-off
/// protocol section (direction / data / session-boundary / evidence)
/// so the persisted shape and the skill documentation stay in
/// lockstep — agents reading the SKILL.md can record the hand-off with
/// the same field names.
///
/// M142 AC-05: `from_role` / `to_role` carry the structured role
/// label (`coordinator` | `runner`) independently from the free-form
/// `from_session` / `to_session`. The session string is whatever the
/// harness chooses to identify its session (UUID, conversation id,
/// etc.); the role is the protocol-level role of the producing /
/// receiving side. The two are populated independently: a manual
/// `mp reviews handoff --from-session s1 --to-session s2` does NOT
/// auto-fill the role; the harness does so via `MP_SESSION_ROLE`.
/// Pre-M142 records round-trip with empty strings (#[serde(default)]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewHandoff {
    pub id: String,
    pub milestone_id: String,
    /// Authoring session id (harness conversation / UUID). Distinct
    /// from `from_role` (coordinator | runner).
    pub from_session: String,
    /// Receiving session id. Distinct from `to_role`. Must differ from
    /// `from_session` at a cross-role hand-off (see `mp reviews l5-check`).
    pub to_session: String,
    /// M142: structured role of the producing side (`coordinator` |
    /// `runner`). Independent from `from_session` (which is the
    /// harness-level session id). `#[serde(default)]` so pre-M142
    /// records round-trip as empty.
    #[serde(default)]
    pub from_role: String,
    /// M142: structured role of the receiving side.
    #[serde(default)]
    pub to_role: String,
    /// What data passes at this hand-off point (free-form text). The
    /// skill recommends a structured shape (e.g. `data: <json>`) but
    /// the CLI does not enforce it.
    pub data: String,
    /// The session-boundary note (e.g. "coordinator's planning session
    /// closes; runner's execution session opens in a fresh session").
    pub session_boundary: String,
    /// Evidence the producing side leaves behind (registry entries,
    /// milestone file state, commit chain, etc.).
    pub evidence: String,
    /// RFC3339 timestamp; same `--at` override semantics as
    /// `ReviewComment.created_at`.
    pub created_at: String,
}

impl ReviewComment {
    /// Validate the structural invariants of a comment draft.
    /// `id` is optional (the CLI auto-generates `C-NN`); `body` and
    /// `author` are required; `created_at` is required and must be
    /// RFC3339 when non-empty (the CLI never writes empty; the helper
    /// preserves the empty-as-no-timestamp sentinel for parity with
    /// `FindingThreadEntry::validate`).
    pub fn validate_draft(
        author: &str,
        body: &str,
        finding_id: &str,
        created_at: &str,
    ) -> Result<(), String> {
        if author.trim().is_empty() {
            return Err("comment author is required".to_string());
        }
        if body.trim().is_empty() {
            return Err("comment body is required".to_string());
        }
        if !finding_id.is_empty() && !is_valid_finding_id(finding_id) {
            return Err(format!(
                "finding_id must be F-NN (digits only after F-); got {finding_id:?}"
            ));
        }
        if !created_at.is_empty() && !is_rfc3339(created_at) {
            return Err(format!("created_at is not RFC3339: {created_at:?}"));
        }
        Ok(())
    }
}

impl ReviewHandoff {
    /// Validate a handoff draft. `from_session` and `to_session` are
    /// free-form text but at least one of them must be non-empty so a
    /// typo doesn't silently record a no-op handoff. `data` is
    /// required (the contract is that something must pass). The
    /// session-boundary and evidence fields default to empty strings
    /// (they're optional in the skill's contract — the skill
    /// recommends them but does not require them).
    pub fn validate_draft(
        from_session: &str,
        to_session: &str,
        data: &str,
        created_at: &str,
    ) -> Result<(), String> {
        if from_session.trim().is_empty() && to_session.trim().is_empty() {
            return Err("at least one of --from-session / --to-session is required".to_string());
        }
        if data.trim().is_empty() {
            return Err("handoff data is required (what passes at this hand-off)".to_string());
        }
        if !created_at.is_empty() && !is_rfc3339(created_at) {
            return Err(format!("created_at is not RFC3339: {created_at:?}"));
        }
        Ok(())
    }
}

/// M133: next-id helper for review comments. Mirrors the F-NN
/// scheme used by findings in shape, but the scope is **plan-wide**:
/// the helper scans every comment in `reviews.json` (not filtered by
/// milestone_id) so `C-01` is unique across the whole plan. Findings
/// use the milestone-scoped `F-NN` counter — the C vs F prefix is
/// what prevents collisions between the two counter spaces, not
/// shared scoping. The plan-wide counter is intentional: threaded
/// review conversations across milestones are listed in one review
/// trail (`mp show milestone`) and a per-milestone restart would
/// produce duplicate ids.
pub fn next_comment_id(comments: &[ReviewComment]) -> String {
    let max = comments
        .iter()
        .filter_map(|c| {
            c.id.strip_prefix("C-")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("C-{:02}", max + 1)
}

/// M133: next-id helper for review handoffs. `H-01`, `H-02`, ...
/// (H for "handoff"). Like [`next_comment_id`], this is plan-scoped
/// rather than milestone-scoped — the H vs F prefix is what prevents
/// collisions with the milestone-scoped findings counter.
pub fn next_handoff_id(handoffs: &[ReviewHandoff]) -> String {
    let max = handoffs
        .iter()
        .filter_map(|h| {
            h.id.strip_prefix("H-")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("H-{:02}", max + 1)
}

impl MilestoneFile {
    /// Return open findings matching the requested review phase.
    ///
    /// Phase semantics:
    /// - `phase == "self"`: returns findings with `f.phase == "self"` OR
    ///   `f.phase == ""` (empty). Empty-phase findings are legacy
    ///   self-review work.
    /// - `phase == "external"`: returns findings with `f.phase ==
    ///   "external"` only. Empty-phase findings do NOT count as
    ///   external — they are self.
    /// - `phase == ""` (caller passes empty): returns all open findings
    ///   regardless of phase tag.
    pub fn open_findings_by_phase(&self, phase: &str) -> Vec<&Finding> {
        if phase.is_empty() {
            return self
                .findings
                .iter()
                .filter(|f| f.status == "open")
                .collect();
        }
        if phase == FINDING_PHASE_SELF {
            return self
                .findings
                .iter()
                .filter(|f| {
                    f.status == "open" && (f.phase == FINDING_PHASE_SELF || f.phase.is_empty())
                })
                .collect();
        }
        // FINDING_PHASE_EXTERNAL (or any other explicit phase): exact match
        self.findings
            .iter()
            .filter(|f| f.status == "open" && f.phase == phase)
            .collect()
    }

    /// Return whether self-review work remains open.
    ///
    /// Empty-phase findings count as self-review work for compatibility;
    /// external review requires an explicit `external` phase.
    pub fn has_open_self_findings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.status == "open" && (f.phase == FINDING_PHASE_SELF || f.phase.is_empty()))
    }

    /// Return whether explicitly external review work remains open.
    ///
    /// See `has_open_self_findings` for the empty-phase rationale.
    /// Empty-phase findings do NOT count as external — only findings
    /// explicitly tagged `external` are external review work.
    pub fn has_open_external_findings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.status == "open" && f.phase == FINDING_PHASE_EXTERNAL)
    }

    /// Count open self-phase findings, including legacy empty-phase entries.
    pub fn open_self_findings_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.status == "open" && (f.phase == FINDING_PHASE_SELF || f.phase.is_empty()))
            .count()
    }

    /// Count open findings explicitly tagged `external`.
    pub fn open_external_findings_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.status == "open" && f.phase == FINDING_PHASE_EXTERNAL)
            .count()
    }
}

#[cfg(test)]
mod finding_tests {
    use super::*;

    fn finding(phase: &str, status: &str) -> Finding {
        Finding {
            id: "F-01".into(),
            severity: "high".into(),
            category: "correctness".into(),
            description: "test".into(),
            status: status.into(),
            author: "reviewer".into(),
            fixed_in: String::new(),
            created: "2026-07-04".into(),
            resolved: String::new(),
            phase: phase.into(),
            anchor: None,
            thread: vec![],
            summary: "short".into(),
            rationale: "long".into(),
            confidence: "high".into(),
            tags: vec!["bug".into()],
        }
    }

    #[test]
    fn open_self_findings_filters_correctly() {
        let m = MilestoneFile {
            findings: vec![
                finding("self", "open"),
                finding("external", "open"),
                finding("self", "fixed"),
            ],
            ..Default::default()
        };
        assert!(m.has_open_self_findings());
        assert!(m.has_open_external_findings());
        assert_eq!(m.open_findings_by_phase("self").len(), 1);
        assert_eq!(m.open_findings_by_phase("external").len(), 1);
        // All open findings regardless of phase
        assert_eq!(m.open_findings_by_phase("").len(), 2);
    }

    #[test]
    fn finding_roundtrips_through_serde() {
        let f = Finding {
            id: "F-01".into(),
            severity: "high".into(),
            category: "security".into(),
            description: "potential XSS".into(),
            status: "open".into(),
            author: "reviewer".into(),
            fixed_in: String::new(),
            created: "2026-07-04".into(),
            resolved: String::new(),
            phase: "external".into(),
            anchor: Some(FindingAnchor {
                path: "src/auth/login.rs".into(),
                commit: "abc123".into(),
                new_range: Some(Range {
                    start_line: 10,
                    end_line: 15,
                }),
                old_range: Some(Range {
                    start_line: 10,
                    end_line: 15,
                }),
                hunk_index: Some(0),
                side: Some("new".into()),
            }),
            thread: vec![FindingThreadEntry {
                author: "agent".into(),
                at: "2026-07-04T01:00:00Z".into(),
                body: "Working on it".into(),
            }],
            summary: "XSS in login form".into(),
            rationale: "User input not escaped before rendering.".into(),
            confidence: "high".into(),
            tags: vec!["security".into(), "p1".into()],
        };
        let json = serde_json::to_string(&f).unwrap();
        let f2: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(f2.phase, "external");
        assert!(f2.anchor.is_some());
        assert_eq!(f2.anchor.as_ref().unwrap().path, "src/auth/login.rs");
        assert_eq!(f2.thread.len(), 1);
        assert_eq!(f2.tags.len(), 2);
    }

    #[test]
    fn finding_without_optional_fields_roundtrips() {
        // A legacy finding (no phase/anchor/thread) must still deserialize.
        let json = r#"{
            "id": "F-01",
            "severity": "low",
            "category": "nit",
            "description": "old finding",
            "status": "fixed",
            "author": "",
            "fixed_in": "",
            "created": "",
            "resolved": ""
        }"#;
        let f: Finding = serde_json::from_str(json).unwrap();
        assert_eq!(f.id, "F-01");
        assert_eq!(f.phase, "");
        assert!(f.anchor.is_none());
        assert!(f.thread.is_empty());
    }

    #[test]
    fn has_open_self_false_when_only_external_open() {
        let m = MilestoneFile {
            findings: vec![finding("external", "open")],
            ..Default::default()
        };
        assert!(!m.has_open_self_findings());
        assert!(m.has_open_external_findings());
    }

    #[test]
    fn has_open_external_false_when_only_self_open() {
        let m = MilestoneFile {
            findings: vec![finding("self", "open")],
            ..Default::default()
        };
        assert!(m.has_open_self_findings());
        assert!(!m.has_open_external_findings());
    }

    #[test]
    fn open_findings_by_phase_returns_empty_when_none() {
        let m = MilestoneFile {
            findings: vec![finding("self", "fixed"), finding("external", "fixed")],
            ..Default::default()
        };
        assert_eq!(m.open_findings_by_phase("self").len(), 0);
        assert_eq!(m.open_findings_by_phase("external").len(), 0);
    }

    /// M125: empty-phase findings (the real-world default; the CLI never
    /// exposed a `--phase` flag until M125) must count as self-phase
    /// review work for the path / status / lifecycle helpers. Without
    /// this, every pre-M125 finding reads as `open_self_findings: 0`
    /// even when there are 7+ open findings, which misleads the
    /// review-lane rendering and the gating helpers downstream.
    #[test]
    fn empty_phase_open_finding_counts_as_self_not_external() {
        let m = MilestoneFile {
            findings: vec![
                finding("", "open"), // empty phase → self (M125)
                finding("self", "open"),
                finding("external", "open"),
                finding("", "fixed"), // closed; not counted
            ],
            ..Default::default()
        };
        // Self helpers: empty-phase + self-phase = 2 open self
        assert!(m.has_open_self_findings());
        // External helpers: only the explicit 'external' tag
        assert!(m.has_open_external_findings());
        // open_findings_by_phase("") catches the empty-phase entries too
        assert_eq!(m.open_findings_by_phase("self").len(), 2); // 1 self + 1 empty
        assert_eq!(m.open_findings_by_phase("external").len(), 1);
        // All open findings regardless of phase
        let all =
            m.open_findings_by_phase("self").len() + m.open_findings_by_phase("external").len();
        assert_eq!(all, 3);
    }
}

// ── Delta types ─────────────────────────────────────────────────────────────

/// Delta section on milestones with `change_kind: delta`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MilestoneDelta {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub base_version: u32,
    #[serde(default)]
    pub added: Vec<DeltaAdded>,
    #[serde(default)]
    pub modified: Vec<DeltaModified>,
    #[serde(default)]
    pub removed: Vec<DeltaRemoved>,
}

impl MilestoneDelta {
    pub fn is_set(&self) -> bool {
        !self.domain.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaAdded {
    #[serde(default)]
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub scenarios: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaModified {
    pub target: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRemoved {
    pub target: String,
    pub reason: String,
    #[serde(default)]
    pub replacement: String,
}

impl MilestoneFile {
    pub fn is_delta_kind(&self) -> bool {
        self.milestone.change_kind == "delta" || self.delta.is_set()
    }

    /// Effective lifecycle: prefer the new field, derive from legacy on
    /// legacy-only milestones.
    pub fn effective_lifecycle(&self) -> String {
        effective_lifecycle(&self.milestone)
    }

    /// B-65: effective execution_status (M125 follow-up). Like
    /// `effective_lifecycle`, prefers the canonical field, falls back to
    /// legacy derivation for pre-M100 milestones. The migration
    /// (commit `1f5aada`) cleared legacy fields, so for current
    /// milestones this is just `meta.execution_status`. The fallback
    /// keeps the helper correct for any pre-migration fixtures.
    pub fn effective_execution_status(&self) -> String {
        if !self.milestone.execution_status.is_empty() {
            return self.milestone.execution_status.clone();
        }
        if self.milestone.cancelled {
            return "cancelled".to_string();
        }
        if self.milestone.blocked {
            return "blocked".to_string();
        }
        if self.milestone.deferred {
            return "deferred".to_string();
        }
        match self.effective_lifecycle().as_str() {
            "draft" | "groomed" | "approved" => "planned".to_string(),
            "in-progress" => "in-progress".to_string(),
            // M196: the canonical lifecycle string is now `"executed"`
            // (the executor's end-state); the legacy `"done"` alias is
            // still accepted by `effective_lifecycle` for the
            // migration window. The projection to legacy
            // `execution_status` keeps `"done"` (the execution-side
            // view is unchanged).
            "done" | "executed" | "self-reviewed" | "reviewed" | "remediation" => {
                "done".to_string()
            }
            "complete" => "done".to_string(),
            _ => String::new(),
        }
    }

    /// Convenience: is this milestone at a terminal lifecycle value?
    pub fn is_terminal(&self) -> bool {
        let lc = self.effective_lifecycle();
        LIFECYCLE_TERMINAL.contains(&lc.as_str()) || self.milestone.cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // M144 AC-01: healthy milestone serializes without `lifecycle_at`
    // (skip_serializing_if = "Option::is_none"); a transitioned milestone
    // serializes with the key set.
    #[test]
    fn lifecycle_at_absent_when_none() {
        let mut meta = sample_meta();
        meta.lifecycle_at = None;
        let json = serde_json::to_value(&meta).expect("serialize");
        assert!(
            json.get("lifecycle_at").is_none(),
            "lifecycle_at should be omitted when None; got: {json}"
        );
    }

    #[test]
    fn lifecycle_at_present_when_set() {
        let mut meta = sample_meta();
        meta.lifecycle_at = Some("2026-07-10T04:00:00Z".to_string());
        let json = serde_json::to_value(&meta).expect("serialize");
        assert_eq!(json["lifecycle_at"], "2026-07-10T04:00:00Z");
    }

    #[test]
    fn healthy_milestone_byte_identical_to_pre_m144() {
        // Pin AC-01 contract: a milestone with `lifecycle_at = None` (the
        // healthy default) must serialize byte-identical to pre-M144
        // output — i.e. the json_shape goldens stay valid.
        let meta = sample_meta();
        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(!json.contains("lifecycle_at"));
    }

    /// M154 AC-02 backward-compat: a pre-M154 comment record (no
    /// `anchor` field on disk) deserializes to `anchor: None`. The
    /// `#[serde(default, skip_serializing_if = "Option::is_none")]`
    /// annotation on the field preserves the round-trip — a comment
    /// with `anchor = None` re-serializes WITHOUT an anchor key, so
    /// pre-M154 comment files written by older `mp reviews comment
    /// add` invocations don't accumulate a noisy `"anchor": null`
    /// field over time.
    #[test]
    fn review_comment_pre_m154_round_trips_with_no_anchor() {
        // Pre-M154 on-disk shape: no anchor field at all.
        let pre_m154 = r#"{
            "id": "C-01",
            "milestone_id": "01",
            "author": "reviewer",
            "body": "legacy comment without anchor",
            "created_at": "2026-07-10T00:00:00+00:00"
        }"#;
        let c: ReviewComment = serde_json::from_str(pre_m154).expect("legacy comment deserializes");
        assert!(
            c.anchor.is_none(),
            "legacy comment must deserialize with anchor=None; got {:?}",
            c.anchor
        );

        // Round-trip back: a None anchor must NOT emit the key.
        let json = serde_json::to_value(&c).expect("serialize");
        assert!(
            json.get("anchor").is_none(),
            "anchor=None must skip the key on serialize; got: {json}"
        );
    }

    /// M154 AC-02 forward-compat: a comment with anchor serializes
    /// the anchor (path + range) and round-trips through deserialization.
    /// Pinned to catch silent shape drift in the on-disk contract.
    #[test]
    fn review_comment_with_anchor_round_trips() {
        let c = ReviewComment {
            id: "C-02".to_string(),
            milestone_id: "01".to_string(),
            author: "mp-coordinator".to_string(),
            body: "file-level note".to_string(),
            finding_id: String::new(),
            created_at: "2026-07-15T00:00:00+00:00".to_string(),
            anchor: Some(FindingAnchor {
                path: "crates/mp/src/install.rs".to_string(),
                commit: String::new(),
                new_range: Some(Range {
                    start_line: 42,
                    end_line: 42,
                }),
                old_range: None,
                hunk_index: None,
                side: Some("new".to_string()),
            }),
        };
        let json = serde_json::to_value(&c).expect("serialize");
        assert_eq!(json["anchor"]["path"], "crates/mp/src/install.rs");
        assert_eq!(json["anchor"]["new_range"]["start_line"], 42);
        assert_eq!(json["anchor"]["new_range"]["end_line"], 42);
        assert_eq!(json["anchor"]["side"], "new");

        let round: ReviewComment = serde_json::from_value(json).expect("deserialize");
        let a = round.anchor.expect("anchor survives round-trip");
        assert_eq!(a.path, "crates/mp/src/install.rs");
        assert_eq!(a.side.as_deref(), Some("new"));
    }

    fn sample_meta() -> MilestoneMeta {
        MilestoneMeta {
            id: "01".to_string(),
            title: "Sample".to_string(),
            slug: "sample".to_string(),
            lifecycle: "draft".to_string(),
            lifecycle_at: None,
            spec_status: String::new(),
            execution_status: String::new(),
            blocked: false,
            needs_regrooming: false,
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            deferred: false,
            deferred_reason: String::new(),
            depends_on: vec![],
            effort: "S".to_string(),
            risk: "low".to_string(),
            change_kind: String::new(),
            priority: "normal".to_string(),
            created: "2026-07-10".to_string(),
            updated: "2026-07-10".to_string(),
            blocked_at: String::new(),
            block_reason: String::new(),
            blocked_by: String::new(),
            target_version: String::new(),
            executed_by: String::new(),
            remediation_pre_state: None,
        }
    }

    #[test]
    fn lifecycle_states_lists_all_nine() {
        // 8 linear + remediation (M101)
        assert_eq!(LIFECYCLE_STATES.len(), 9);
        for s in &[
            "draft",
            "groomed",
            "approved",
            "in-progress",
            "executed",
            "self-reviewed",
            "reviewed",
            "complete",
            "remediation",
        ] {
            assert!(LIFECYCLE_STATES.contains(s), "{s} missing");
        }
    }

    #[test]
    fn legacy_spec_status_mapping() {
        // M196: the canonical executor end-state is now `"executed"`.
        // Legacy `spec_status=implemented` maps to the canonical
        // lifecycle string `"executed"` (was `"done"`).
        assert_eq!(legacy_spec_status_to_lifecycle("draft"), "draft");
        assert_eq!(legacy_spec_status_to_lifecycle("interview"), "groomed");
        assert_eq!(legacy_spec_status_to_lifecycle("review"), "groomed");
        assert_eq!(legacy_spec_status_to_lifecycle("ready"), "approved");
        assert_eq!(legacy_spec_status_to_lifecycle("implemented"), "executed");
        assert_eq!(legacy_spec_status_to_lifecycle("verified"), "complete");
        assert_eq!(legacy_spec_status_to_lifecycle("bogus"), "draft");
    }

    #[test]
    fn legacy_execution_status_mapping() {
        assert_eq!(legacy_execution_status_to_lifecycle("planned"), "draft");
        assert_eq!(
            legacy_execution_status_to_lifecycle("in-progress"),
            "in-progress"
        );
        // M196: legacy `execution_status="done"` (work finished,
        // awaiting review) maps to the canonical lifecycle
        // `"executed"` (renamed from `"done"`). The legacy
        // `execution_status` value `"done"` is preserved on disk;
        // only the lifecycle-side projection is renamed.
        assert_eq!(legacy_execution_status_to_lifecycle("done"), "executed");
        assert_eq!(legacy_execution_status_to_lifecycle("blocked"), "draft");
        assert_eq!(legacy_execution_status_to_lifecycle("deferred"), "draft");
        assert_eq!(
            legacy_execution_status_to_lifecycle("cancelled"),
            "cancelled"
        );
    }

    #[test]
    fn effective_lifecycle_done_exec_with_verified_spec_is_complete() {
        // The real-world common case: spec=verified + exec=done → complete.
        // exec=done alone maps to lifecycle "done" (order 4); spec=verified
        // maps to "complete" (order 7); the max picks complete. Correct.
        let m = MilestoneMeta {
            lifecycle: "draft".into(), // sentinel: derive from legacy
            spec_status: "verified".into(),
            execution_status: "done".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&m), "complete");
    }

    #[test]
    fn effective_lifecycle_done_exec_with_implemented_spec_is_executed_not_complete() {
        // The bug the wrong mapping caused: spec=implemented (work done, not
        // verified) + exec=done must be lifecycle "executed" (awaiting
        // review), NOT terminal "complete". The old mapping (exec done →
        // complete) would have reported this as complete — skipping review
        // entirely. M196 renamed the executor end-state from "done" to
        // "executed" so the spec-side "verified" → "complete" distinction
        // is unambiguous.
        let m = MilestoneMeta {
            lifecycle: "draft".into(),
            spec_status: "implemented".into(),
            execution_status: "done".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&m), "executed");
    }

    #[test]
    fn effective_lifecycle_prefers_new_field() {
        let meta = MilestoneMeta {
            lifecycle: "in-progress".into(),
            spec_status: "verified".into(), // legacy says complete
            execution_status: "done".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&meta), "in-progress");
    }

    #[test]
    fn effective_lifecycle_derives_when_legacy_only() {
        // legacy spec_status=ready, execution_status=planned → approved
        let meta = MilestoneMeta {
            lifecycle: "draft".into(), // serde default; not actually set on disk
            spec_status: "ready".into(),
            execution_status: "planned".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&meta), "approved");
    }

    #[test]
    fn effective_lifecycle_picks_most_advanced_of_two_legacy() {
        // M100 ER-7: previously this test pinned `spec=verified +
        // exec=in-progress → complete`, but ER-7 argued (and the review
        // agreed) that execution stage dominates once started — a
        // verified spec on a still-running milestone is `in-progress`,
        // not terminal. The fix short-circuits to `in-progress` when
        // exec-side is `in-progress` regardless of spec-side progress.
        let meta = MilestoneMeta {
            lifecycle: "draft".into(), // serde default
            spec_status: "verified".into(),
            execution_status: "in-progress".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&meta), "in-progress");
    }

    #[test]
    fn effective_lifecycle_done_exec_with_verified_spec_stays_complete_after_er7() {
        // Companion to `effective_lifecycle_picks_most_advanced_of_two_legacy`:
        // when exec-side is `done` (not `in-progress`), the original
        // max() logic still applies and yields `complete`.
        let meta = MilestoneMeta {
            lifecycle: "draft".into(),
            spec_status: "verified".into(),
            execution_status: "done".into(),
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&meta), "complete");
    }

    #[test]
    fn effective_execution_status_prefers_canonical_field() {
        // B-65: the helper returns the canonical execution_status when set.
        // Post-migration this is the primary signal; legacy fields are
        // cleared by commit 1f5aada.
        let meta = MilestoneMeta {
            lifecycle: "done".into(),
            execution_status: "blocked".into(), // current state
            spec_status: "".into(),
            ..Default::default()
        };
        let m = MilestoneFile {
            milestone: meta,
            ..Default::default()
        };
        assert_eq!(m.effective_execution_status(), "blocked");
    }

    #[test]
    fn effective_execution_status_falls_back_to_lifecycle_derivation() {
        // When execution_status is empty, derive from lifecycle + boolean flags.
        let meta = MilestoneMeta {
            lifecycle: "approved".into(),
            execution_status: "".into(),
            spec_status: "".into(),
            ..Default::default()
        };
        let m = MilestoneFile {
            milestone: meta,
            ..Default::default()
        };
        assert_eq!(m.effective_execution_status(), "planned");
    }

    #[test]
    fn effective_execution_status_derives_blocked_from_flag() {
        let meta = MilestoneMeta {
            lifecycle: "approved".into(),
            execution_status: "".into(),
            blocked: true,
            ..Default::default()
        };
        let m = MilestoneFile {
            milestone: meta,
            ..Default::default()
        };
        assert_eq!(m.effective_execution_status(), "blocked");
    }

    #[test]
    fn overlays_default_to_false() {
        let m = MilestoneFile::default();
        assert!(!m.milestone.blocked);
        assert!(!m.milestone.needs_regrooming);
        assert!(!m.milestone.cancelled);
        assert!(!m.milestone.deferred);
    }

    #[test]
    fn default_lifecycle_is_draft() {
        let m = MilestoneFile::default();
        assert_eq!(m.milestone.lifecycle, "draft");
    }

    #[test]
    fn legacy_fields_skipped_on_disk_when_empty() {
        // Backward-compat: a milestone written via Default has empty
        // spec_status/execution_status; those should not serialize.
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "01".into(),
                title: "T".into(),
                slug: "t".into(),
                lifecycle: "complete".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("\"spec_status\""));
        assert!(!json.contains("\"execution_status\""));
    }

    #[test]
    fn overlays_persist_on_disk() {
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "01".into(),
                title: "T".into(),
                slug: "t".into(),
                lifecycle: "approved".into(),
                blocked: true,
                blocked_at: "2026-07-04T00:00:00Z".into(),
                block_reason: "deps not met".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        let m2: MilestoneFile = serde_json::from_str(&json).unwrap();
        assert!(m2.milestone.blocked);
        assert_eq!(m2.milestone.blocked_at, "2026-07-04T00:00:00Z");
        assert_eq!(m2.milestone.block_reason, "deps not met");
        assert_eq!(m2.milestone.lifecycle, "approved");
    }

    #[test]
    fn is_terminal_for_complete_or_cancelled() {
        let mut m = MilestoneFile::default();
        m.milestone.lifecycle = "complete".into();
        assert!(m.is_terminal());
        m.milestone.lifecycle = "approved".into();
        assert!(!m.is_terminal());
        m.milestone.cancelled = true;
        assert!(m.is_terminal());
    }

    #[test]
    fn debug_m02_lifecycle_matches_fixture() {
        // Mirrors tests/fixtures/projects/linear-deps/master-plan/milestones/02-core.json
        let meta = MilestoneMeta {
            id: "02".into(),
            title: "Core".into(),
            slug: "core".into(),
            lifecycle: String::new(),
            spec_status: "ready".into(),
            execution_status: "in-progress".into(),
            ..Default::default()
        };
        let lc = effective_lifecycle(&meta);
        assert_eq!(lc, "in-progress", "M02 should resolve to in-progress");
    }

    #[test]
    fn effective_lifecycle_cancelled_overlay_beats_lifecycle_field() {
        // M174 fix: the cancellation overlay is terminal — once a
        // milestone is cancelled, the lifecycle column should
        // read `cancelled` regardless of where the milestone was
        // when it was cancelled. Without this, M174 (cancelled
        // at lifecycle=approved) would keep showing `approved` in
        // the TUI Milestones lane, which is misleading.
        let approved_then_cancelled = MilestoneMeta {
            lifecycle: "approved".into(),
            spec_status: "ready".into(),
            execution_status: "cancelled".into(),
            cancelled: true,
            ..Default::default()
        };
        assert_eq!(
            effective_lifecycle(&approved_then_cancelled),
            "cancelled",
            "cancelled overlay must short-circuit before the max() mapping"
        );

        let executed_then_cancelled = MilestoneMeta {
            lifecycle: "executed".into(),
            spec_status: "implemented".into(),
            execution_status: "done".into(),
            cancelled: true,
            ..Default::default()
        };
        assert_eq!(
            effective_lifecycle(&executed_then_cancelled),
            "cancelled",
            "even a milestone that reached executed must show cancelled once the overlay is set"
        );
    }

    #[test]
    fn effective_lifecycle_without_cancelled_keeps_legacy_mapping() {
        // M174 fix negative case: when `cancelled: false`, the
        // existing max(spec, exec) mapping must be unchanged.
        // Pin the pre-M174 behavior so a future refactor can't
        // accidentally regress the unrelated branches.
        let just_approved = MilestoneMeta {
            lifecycle: "approved".into(),
            spec_status: "ready".into(),
            execution_status: "planned".into(),
            cancelled: false,
            ..Default::default()
        };
        assert_eq!(effective_lifecycle(&just_approved), "approved");
    }
}
