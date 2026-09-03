//! M213 — headless cycle-flow engine for autopilot sessions.
//!
//! The cycle engine consumes the durable session events from M207,
//! dispatches typed assignments through M211, applies M209's
//! topology policy, and surfaces M212's verifier outcomes. It is
//! the *only* lane that drives progress; `raul` is a passive
//! observer (M201 / M207 lesson — a TUI-mediated relay stops when
//! the lane is unfocused, contradicting unattended autopilot).
//!
//! ## State machine (AC-01)
//!
//! The canonical cycle is:
//!
//! ```text
//!   Dispatching -> WaitingRunner -> Reviewing -> Deciding -> CycleNext -> Dispatching ...
//! ```
//!
//! Every transition is a typed [`CycleEvent`]. The `advance` method
//! is pure: same `(state, event)` always produces the same next
//! state, so tests feed scripted event sequences and pin the path.
//!
//! ## Topology tightening (AC-03)
//!
//! 1-pane topology skips the `Reviewing` state — there is no
//! independent reviewer channel. The state machine encodes the
//! "no external review" path as an explicit transition so a future
//! topology variant cannot quietly drop the rule.
//!
//! 2-pane topology forbids `ShipWithBacklog` — orchestrator +
//! reviewer share a supervisor pane, so the "review" surface is not
//! independent. Low-severity findings force `CycleNext`.
//!
//! ## Decision matrix (AC-02 / AC-04)
//!
//! [`apply_decision_matrix`] consumes a [`DecisionInput`] and
//! returns one of: `Complete`, `CycleNext`, `ShipWithBacklog`, or
//! `Escalate`. Topology and cycle cap are both enforced inside
//! the matrix; a clean pass at the cycle budget returns
//! `Complete`, and any non-pass at the budget returns
//! `Escalate { CycleCapExhausted }`.
//!
//! ## Reviewer activation (AC-05)
//!
//! [`build_reviewer_activation`] produces a M211
//! [`TaskAssignment`] with the documented `review_request` payload
//! (milestone id, cycle, evidence revision, reviewer actor token,
//! mode flag). The orchestrator appends an `AssignmentDispatched`
//! event after the herdr spawn outcome is known — the verifier
//! (M212) cross-checks the event against the session event log.
//!
//! ## Heartbeats + stale-state timeout (AC-06)
//!
//! [`classify_liveness`] returns `Healthy` whenever the most recent
//! heartbeat ack covers the current role state. `StaleStateTimeout`
//! fires only when *both* the heartbeat is missed **and** the role
//! state changed since the last ack — a responsive lane may stay
//! in the same state indefinitely without timing out.
//!
//! ## predict_next_action (AC-07)
//!
//! [`predict_next_action`] is the pure function the TUI consumes
//! to show "what's next" per milestone. It looks at the cycle
//! state's current position plus the most recent session events
//! and returns one of six named actions: `DispatchRunner`,
//! `DispatchReviewer`, `AwaitRunner`, `AwaitReviewer`,
//! `ApplyMatrix`, `EscalateUser` (plus a `NoOp` sentinel for
//! terminal states).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::autopilot::events::OrchestrationEvent;
use crate::autopilot::role::{MilestoneKind, Role, Topology, TopologyPolicy};
use crate::autopilot::session::{RoleName, WorkingOn};
use crate::autopilot::task_assign::{RoleDirection, TaskAssignment};

// ─── Cycle state machine ─────────────────────────────────────────────────

/// Closed set of cycle states. The cycle engine visits these in
/// the canonical order
/// `Dispatching -> WaitingRunner -> Reviewing -> Deciding -> CycleNext -> Dispatching`
/// until a terminal state (`Complete` / `Escalate`) is reached or
/// the topology tightens (1-pane skips `Reviewing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CycleState {
    /// The cycle engine is composing the runner (or reviewer)
    /// assignment for this cycle.
    Dispatching,
    /// The cycle engine has dispatched and is awaiting the
    /// runner's "completed-execute" notification.
    WaitingRunner,
    /// The runner notified and (in 3-pane / 2-pane) the cycle
    /// engine is now waiting for the reviewer verdict.
    Reviewing,
    /// A reviewer verdict (or 1-pane runner verdict) is in hand;
    /// the decision matrix is applied next.
    Deciding,
    /// The decision matrix returned `CycleNext` — bump the cycle
    /// counter and re-enter `Dispatching` for the next cycle.
    CycleNext,
    /// Terminal: the milestone is `complete`. The cycle engine
    /// stops dispatching and emits a `Complete` outcome.
    Complete,
    /// Terminal: the cycle engine exhausted the cycle budget,
    /// hit a topology-block, or observed a stale-state timeout.
    /// The orchestrator surfaces this to the human operator.
    Escalate,
}

impl CycleState {
    /// Stable kebab-case wire form (matches serde).
    pub const fn as_str(self) -> &'static str {
        match self {
            CycleState::Dispatching => "dispatching",
            CycleState::WaitingRunner => "waiting-runner",
            CycleState::Reviewing => "reviewing",
            CycleState::Deciding => "deciding",
            CycleState::CycleNext => "cycle-next",
            CycleState::Complete => "complete",
            CycleState::Escalate => "escalate",
        }
    }

    /// True when the state is terminal (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(self, CycleState::Complete | CycleState::Escalate)
    }
}

/// Typed input event for the cycle state machine. The cycle engine
/// never reads raw `herdr` output — it consumes these typed values
/// and the [`OrchestrationEvent`] journal for context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CycleEvent {
    /// Orchestrator finished composing the runner assignment;
    /// herdr argv was rendered and the spawn outcome is known.
    RunnerDispatched { pane: String },
    /// Runner notified "completed-execute" (or equivalent). Only
    /// fires after the M212 verifier accepted the lane's evidence.
    RunnerCompleted { pane: String },
    /// Orchestrator finished composing the reviewer assignment.
    ReviewerDispatched { pane: String },
    /// Reviewer notified a verdict. The payload is the typed
    /// outcome from M212's lane-notification cross-check.
    ReviewerVerdict {
        pane: String,
        verdict: ReviewerVerdict,
        findings: FindingSummary,
    },
    /// The cycle engine polled and the milestone state did not
    /// change since the last tick. Drives `Deciding -> CycleNext`.
    StateTick,
    /// Topology tightening: 1-pane has no external reviewer. The
    /// runner verdict stands in for the reviewer verdict, so the
    /// cycle skips `Reviewing` and goes straight to `Deciding`.
    NoExternalReview,
}

/// Typed review verdict. Mirrors the M212 verifier's verdict
/// contract (Pass / PassWithBacklog / Fail). Serialized as
/// kebab-case for cross-crate compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewerVerdict {
    /// Reviewer signed off — no findings.
    Pass,
    /// Reviewer signed off but low-severity findings remain; the
    /// milestone may ship with backlog under a permissive topology.
    PassWithBacklog,
    /// Reviewer found blockers; the cycle must continue.
    Fail,
}

impl ReviewerVerdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            ReviewerVerdict::Pass => "pass",
            ReviewerVerdict::PassWithBacklog => "pass-with-backlog",
            ReviewerVerdict::Fail => "fail",
        }
    }
}

/// Per-severity finding counts. The decision matrix uses the
/// counts to decide between `CycleNext` (high-severity or
/// correctness) and `ShipWithBacklog` (low-severity only, and
/// only under a topology that allows the path).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingSummary {
    #[serde(default)]
    pub high_severity_count: u32,
    #[serde(default)]
    pub correctness_count: u32,
    #[serde(default)]
    pub low_count: u32,
    #[serde(default)]
    pub blocker_count: u32,
}

impl FindingSummary {
    /// Convenience: total finding count across severities.
    pub fn total(&self) -> u32 {
        self.high_severity_count + self.correctness_count + self.low_count + self.blocker_count
    }

    /// Convenience: any high-severity or correctness finding.
    pub fn has_high_or_correctness(&self) -> bool {
        self.high_severity_count > 0 || self.correctness_count > 0
    }
}

/// Per-milestone cycle state machine snapshot. Drives both the
/// pure `advance` function and the `predict_next_action` TUI
/// helper. The struct holds the topology policy alongside the
/// state so a single read suffices for both transition rules and
/// decision-matrix inputs.
///
/// `TopologyPolicy` from M209 does not derive Serialize, so this
/// struct deliberately does not either — the cycle state machine
/// is built fresh from the typed topology + verdict inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleStateMachine {
    pub state: CycleState,
    pub milestone_id: String,
    pub cycle: u32,
    pub topology: Topology,
    pub policy: TopologyPolicy,
    /// Per-cycle verdict history. Used by AC-04's cap-enforcement
    /// ("4 prior Pass verdicts still escalate at cycle 4").
    pub cycle_history: Vec<CycleVerdictRecord>,
}

impl CycleStateMachine {
    /// Build the starting state machine for a milestone. The cycle
    /// counter begins at 1 (cycles are 1-indexed throughout the
    /// engine — matches the M207 `working_on.cycle` convention).
    pub fn new(milestone_id: impl Into<String>, topology: Topology) -> Self {
        let policy = crate::autopilot::role::topology_policy(topology);
        Self {
            state: CycleState::Dispatching,
            milestone_id: milestone_id.into(),
            cycle: 1,
            topology,
            policy,
            cycle_history: Vec::new(),
        }
    }

    /// Advance the state machine by one event. Pure function — no
    /// I/O, no globals. Invalid transitions return `self` unchanged
    /// so a misfired event is observable (caller can compare state
    /// before/after) rather than silently dropping.
    pub fn advance(&self, event: CycleEvent) -> Self {
        let mut updated = self.clone();
        let next = match (self.state, event) {
            // Canonical 3-pane path.
            (CycleState::Dispatching, CycleEvent::RunnerDispatched { .. }) => {
                CycleState::WaitingRunner
            }
            (CycleState::WaitingRunner, CycleEvent::RunnerCompleted { .. }) => {
                if self.topology == Topology::OneAgent {
                    // 1-pane skips the Reviewing state — runner
                    // verdict is the only verdict.
                    CycleState::Deciding
                } else {
                    CycleState::Reviewing
                }
            }
            (CycleState::Reviewing, CycleEvent::ReviewerDispatched { .. }) => CycleState::Reviewing,
            (
                CycleState::Reviewing,
                CycleEvent::ReviewerVerdict {
                    verdict, findings, ..
                },
            ) => {
                // Stash the verdict in history for the
                // cap-enforcement / predict-next-action paths.
                updated.cycle_history.push(CycleVerdictRecord {
                    cycle: updated.cycle,
                    verdict: verdict.as_str().to_string(),
                    findings,
                    topology_mode: self.policy.mode.as_str().to_string(),
                });
                CycleState::Deciding
            }
            (CycleState::Deciding, CycleEvent::StateTick) => CycleState::CycleNext,
            (CycleState::CycleNext, CycleEvent::StateTick) => {
                updated.cycle = updated.cycle.saturating_add(1);
                CycleState::Dispatching
            }
            // 1-pane skip path: explicit NoExternalReview from
            // the runner-done handler bypasses Reviewing. This
            // is also where the 1-pane path lands — see
            // `RunnerCompleted` above.
            (CycleState::Reviewing, CycleEvent::NoExternalReview) => CycleState::Deciding,
            // Terminal states are absorbing — no transitions out.
            (CycleState::Complete, _) | (CycleState::Escalate, _) => self.state,
            // Anything else is a misfire; state machine does not
            // move and the caller observes the unchanged snapshot.
            _ => self.state,
        };
        updated.state = next;
        updated
    }

    /// Convenience: bump the cycle counter and return to
    /// `Dispatching`. Equivalent to advancing through
    /// `CycleNext -> StateTick -> Dispatching` but written for the
    /// common case.
    pub fn next_cycle(&self) -> Self {
        let mut updated = self.clone();
        updated.cycle = updated.cycle.saturating_add(1);
        updated.state = CycleState::Dispatching;
        updated
    }
}

/// Per-cycle verdict record. The cycle engine appends one entry
/// per cycle to [`CycleStateMachine::cycle_history`]; the
/// predict-next-action helper uses the tail of that history to
/// decide between `ApplyMatrix` and `Complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleVerdictRecord {
    pub cycle: u32,
    pub verdict: String,
    pub findings: FindingSummary,
    pub topology_mode: String,
}

// ─── Decision matrix ────────────────────────────────────────────────────

/// Input to the cycle-flow decision matrix. The matrix is pure
/// over `(cycle, topology, policy, verdict, findings)` — no
/// globals, no I/O. Tests construct a literal `DecisionInput` and
/// pin the `CycleDecision` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    /// Current cycle number (1-indexed).
    pub cycle: u32,
    /// Topology the milestone is running under.
    pub topology: Topology,
    /// The policy derived from the topology (mode + cycle_budget).
    pub policy: TopologyPolicy,
    /// Reviewer verdict.
    pub verdict: ReviewerVerdict,
    /// Findings summary that came back with the verdict.
    pub findings: FindingSummary,
    /// History of prior cycle verdicts. Empty for cycle 1.
    pub cycle_history: Vec<CycleVerdictRecord>,
}

/// Output of the decision matrix. The cycle engine consumes this
/// directly to drive `CycleNext` (loop) / `Complete` (terminal) /
/// `ShipWithBacklog` (terminal with backlog) / `Escalate`
/// (terminal with operator handoff).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CycleDecision {
    /// Milestone is done. The cycle engine transitions to
    /// `CycleState::Complete` and stops dispatching.
    Complete,
    /// Continue to the next cycle. The cycle engine bumps the
    /// counter and re-enters `Dispatching`.
    CycleNext,
    /// Milestone may ship with backlog still open. Only valid
    /// under 3-pane / FullMatrix; the matrix enforces this.
    ShipWithBacklog,
    /// Terminal: human operator must take over. The `reason`
    /// carries the typed cause (cycle cap, topology block, stale
    /// state timeout).
    Escalate { reason: CycleEscalateReason },
}

/// Typed reason for an `Escalate` decision. Surfaced in the
/// `EscalateToUser` remediation payload (M212) and in the
/// orchestrator's TUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum CycleEscalateReason {
    /// The cycle budget was exhausted. Even a sequence of `Pass`
    /// verdicts returns Escalate at the cap — that is the
    /// anti-thrash guarantee (AC-04).
    CycleCapExhausted {
        cycle: u32,
        budget: u32,
        verdict_history: Vec<String>,
    },
    /// Topology forbids `ShipWithBacklog` (2-pane / 1-pane) and
    /// the verdict was `PassWithBacklog`; the matrix downgrades to
    /// `CycleNext` unless the cap is also reached, in which case
    /// this fires. `mode_label` is the wire form of the topology
    /// mode (TopologyMode does not derive Serialize).
    TopologyBlocksShipWithBacklog {
        topology: Topology,
        mode_label: String,
    },
    /// Lane heartbeat missed and the role state changed since the
    /// last ack — `StaleStateTimeout`.
    StaleStateTimeout {
        lane: RoleName,
        last_ack_at: String,
        state_change_at: String,
    },
}

impl CycleDecision {
    /// Convenience: true for terminal decisions (`Complete` /
    /// `ShipWithBacklog` / `Escalate`). The cycle engine uses
    /// this to decide whether to bump the cycle counter.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CycleDecision::Complete
                | CycleDecision::ShipWithBacklog
                | CycleDecision::Escalate { .. }
        )
    }

    /// Stable kebab-case wire form for the decision kind.
    pub fn kind_str(&self) -> &'static str {
        match self {
            CycleDecision::Complete => "complete",
            CycleDecision::CycleNext => "cycle-next",
            CycleDecision::ShipWithBacklog => "ship-with-backlog",
            CycleDecision::Escalate { .. } => "escalate",
        }
    }
}

/// Apply the cycle-flow decision matrix.
///
/// Pure function over `DecisionInput`. The matrix is the single
/// place the engine decides between `Complete`, `CycleNext`,
/// `ShipWithBacklog`, and `Escalate`; the topology-aware variants
/// of those rules live here too so the state machine itself
/// stays small.
///
/// Per spec:
/// - clean reviewer pass -> `Complete`
/// - low-severity findings, <=2 low, full-matrix topology ->
///   `ShipWithBacklog`
/// - high-severity / correctness findings -> `CycleNext`
/// - non-pass at the hard limit -> `Escalate(CycleCapExhausted)`
pub fn apply_decision_matrix(input: &DecisionInput) -> CycleDecision {
    let verdict_history: Vec<String> = input
        .cycle_history
        .iter()
        .map(|h| h.verdict.clone())
        .collect();

    // Topology tightening: 2-pane / 1-pane forbid ShipWithBacklog.
    // A PassWithBacklog verdict under those topologies is
    // downgraded to CycleNext unless the cap is also reached
    // (then the cap wins).
    if matches!(input.verdict, ReviewerVerdict::PassWithBacklog)
        && !input.policy.allows_ship_with_backlog()
    {
        if input.cycle >= input.policy.cycle_budget {
            return CycleDecision::Escalate {
                reason: CycleEscalateReason::CycleCapExhausted {
                    cycle: input.cycle,
                    budget: input.policy.cycle_budget,
                    verdict_history,
                },
            };
        }
        return CycleDecision::CycleNext;
    }

    // Cycle cap: even a sequence of clean passes escalates at the
    // hard limit. AC-04's headline guarantee.
    if input.cycle >= input.policy.cycle_budget {
        return CycleDecision::Escalate {
            reason: CycleEscalateReason::CycleCapExhausted {
                cycle: input.cycle,
                budget: input.policy.cycle_budget,
                verdict_history,
            },
        };
    }

    match input.verdict {
        ReviewerVerdict::Pass => CycleDecision::Complete,
        ReviewerVerdict::PassWithBacklog => {
            // 3-pane + <=2 low-severity findings -> ShipWithBacklog.
            // Otherwise CycleNext.
            if input.policy.allows_ship_with_backlog() && input.findings.low_count <= 2 {
                CycleDecision::ShipWithBacklog
            } else {
                CycleDecision::CycleNext
            }
        }
        ReviewerVerdict::Fail => {
            // High / correctness findings force CycleNext. The
            // matrix does not second-guess the verdict — the
            // runner lane owns the fix.
            CycleDecision::CycleNext
        }
    }
}

// ─── Topology preflight (cycle-flow entry point) ────────────────────────

/// Cycle-flow entry-point gate: validates the topology vs.
/// milestone kind before the state machine starts ticking.
/// Mirrors M209's `topology_preflight` but errors with the
/// cycle-flow surface (`CycleStartError`) so the runner can
/// distinguish a topology rejection from a downstream cycle
/// failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleStartError {
    /// 1-pane + full milestone without a recorded bypass.
    TopologyPreflight(crate::autopilot::role::TopologyPreflightError),
}

impl std::fmt::Display for CycleStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleStartError::TopologyPreflight(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CycleStartError {}

/// Start the cycle state machine for a milestone, after running
/// the topology preflight. Convenience constructor that wraps
/// [`CycleStateMachine::new`] with the topology gate so callers
/// don't need to import M209's preflight separately.
pub fn start_cycle(
    milestone_id: impl Into<String>,
    topology: Topology,
    kind: MilestoneKind,
    bypass: crate::autopilot::role::ReviewBypassPolicy,
) -> Result<CycleStateMachine, CycleStartError> {
    let _policy = crate::autopilot::role::topology_preflight(topology, kind, bypass)
        .map_err(CycleStartError::TopologyPreflight)?;
    Ok(CycleStateMachine::new(milestone_id, topology))
}

// ─── Reviewer activation (M211 typed dispatch) ─────────────────────────

/// Mode the reviewer is invoked under. The cycle engine flips to
/// `BlockersOnly` at the soft-cap threshold (cycle 2 in
/// FullMatrix) so the reviewer prompt tightens without burning a
/// full pass on lower-priority findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewerMode {
    /// Full review pass. Default for cycle 1.
    Full,
    /// Tightened reviewer prompt: only blockers and correctness
    /// findings are surfaced. Engaged at cycle 2 (FullMatrix
    /// soft cap).
    BlockersOnly,
}

impl ReviewerMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            ReviewerMode::Full => "full",
            ReviewerMode::BlockersOnly => "blockers-only",
        }
    }
}

/// Typed review-request payload. Mirrors the M211
/// [`TaskAssignment`] schema but with reviewer-specific fields
/// (`evidence_revision`, `reviewer_actor_token`, `mode`). The
/// payload rides inside the TaskAssignment's `task` body so the
/// M211 shell-metacharacter validator catches the same smuggling
/// attempts the M211 regression gate already guards against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRequestPayload {
    pub milestone_id: String,
    pub cycle: u32,
    pub evidence_revision: String,
    pub reviewer_actor_token: String,
    pub mode: ReviewerMode,
}

impl ReviewRequestPayload {
    /// Render the deterministic review-request text. The same
    /// payload always produces the same text; the golden is
    /// pinned by the integration test.
    ///
    /// Rendered as a single line so the text passes the M211
    /// shell-metacharacter validator (`\n`, `|`, `;`, etc. are
    /// in the metachar set). Fields are separated by commas
    /// with spaces — commas are not in the metachar set.
    pub fn render(&self) -> String {
        format!(
            "review_request: milestone_id={milestone}, cycle={cycle}, \
             evidence_revision={rev}, reviewer_actor_token={tok}, mode={mode}",
            milestone = self.milestone_id,
            cycle = self.cycle,
            rev = self.evidence_revision,
            tok = self.reviewer_actor_token,
            mode = self.mode.as_str(),
        )
    }
}

/// Build the M211 typed dispatch for a reviewer activation. The
/// orchestrator appends an `AssignmentDispatched` event after the
/// herdr spawn outcome is known — see `task_assign::dispatch_assignment`.
///
/// `session_id` is the canonical session id the orchestrator is
/// driving; `target_pane` is the reviewer pane from M207's
/// `topology.reviewer.pane_id`. `evidence_revision` rides on
/// every activation so the verifier can cross-check the milestone
/// AC projection revision.
pub fn build_reviewer_activation(
    session_id: impl Into<String>,
    payload: &ReviewRequestPayload,
    target_pane: impl Into<String>,
) -> TaskAssignment {
    let mut assignment = TaskAssignment::new(
        session_id,
        payload.milestone_id.clone(),
        payload.cycle,
        RoleDirection::OrchestratorToReviewer,
        target_pane,
        payload.render(),
    );
    assignment
        .evidence_refs
        .push(format!("evidence_revision={}", payload.evidence_revision));
    assignment
        .boundary_reminders
        .push("report via mp autopilot session transition".to_string());
    assignment
}

/// Soft cap: cycle number at which the reviewer mode flips from
/// `Full` to `BlockersOnly`. Per spec, cycle 2 onward is the
/// blockers-only window (catches drift faster than cycle 3).
pub const REVIEWER_SOFT_CAP_CYCLE: u32 = 2;

/// Pick the reviewer mode for the supplied cycle. Cycle 1 is
/// always `Full`; cycle >= 2 is `BlockersOnly`.
pub fn reviewer_mode_for_cycle(cycle: u32) -> ReviewerMode {
    if cycle >= REVIEWER_SOFT_CAP_CYCLE {
        ReviewerMode::BlockersOnly
    } else {
        ReviewerMode::Full
    }
}

// ─── Heartbeat + stale-state timeout ────────────────────────────────────

/// Per-lane heartbeat tracker. The cycle engine reads the most
/// recent ack + the lane's role state and classifies liveness.
/// The struct is intentionally tiny — the only state the matrix
/// needs is "when did the lane last ack, and has its role state
/// moved since then?"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatTracker {
    pub lane: RoleName,
    pub last_ack_at: String,
    pub last_acknowledged_state: HeartbeatAckState,
    pub current_state: HeartbeatAckState,
    pub state_changed_at: String,
}

/// Compact view of a role's working state, used by the heartbeat
/// tracker. Mirrors [`crate::autopilot::transitions::RoleState`]
/// but flattened to the few values the heartbeat logic needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeartbeatAckState {
    Idle,
    Working,
    Blocked,
    Done,
}

impl HeartbeatAckState {
    pub const fn as_str(self) -> &'static str {
        match self {
            HeartbeatAckState::Idle => "idle",
            HeartbeatAckState::Working => "working",
            HeartbeatAckState::Blocked => "blocked",
            HeartbeatAckState::Done => "done",
        }
    }
}

/// Liveness verdict returned by [`classify_liveness`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LivenessStatus {
    /// Lane responded within the heartbeat window. The current
    /// state is acknowledged.
    Healthy,
    /// Lane has not acked recently AND the role state changed
    /// since the last ack. The cycle engine surfaces this as
    /// `CycleDecision::Escalate { StaleStateTimeout }`.
    StaleStateTimeout {
        lane: RoleName,
        last_ack_at: String,
        state_change_at: String,
    },
}

impl LivenessStatus {
    pub fn is_stale(&self) -> bool {
        matches!(self, LivenessStatus::StaleStateTimeout { .. })
    }
}

/// Classify a heartbeat tick. The function is pure over
/// `(tracker, now_ms, heartbeat_timeout_ms)`:
///
/// - `Healthy` when `now - last_ack_at <= heartbeat_timeout_ms`
///   **or** when the role state has not changed since the last
///   ack (a responsive lane may sit in the same state
///   indefinitely without timing out).
/// - `StaleStateTimeout` only when *both* the heartbeat is missed
///   **and** the role state changed since the last ack.
pub fn classify_liveness(
    tracker: &HeartbeatTracker,
    now_ms: u64,
    heartbeat_timeout_ms: u64,
) -> LivenessStatus {
    let last_ack_ms = parse_rfc3339_ms(&tracker.last_ack_at);

    let state_changed = tracker.current_state != tracker.last_acknowledged_state;
    let heartbeat_missed = match last_ack_ms {
        Some(ack) => now_ms.saturating_sub(ack) > heartbeat_timeout_ms,
        // No prior ack -> treat as missed.
        None => true,
    };

    if heartbeat_missed && state_changed {
        LivenessStatus::StaleStateTimeout {
            lane: tracker.lane,
            last_ack_at: tracker.last_ack_at.clone(),
            state_change_at: tracker.state_changed_at.clone(),
        }
    } else {
        LivenessStatus::Healthy
    }
}

/// Minimal RFC3339 -> ms-since-epoch parser. The cycle engine
/// does not need full RFC3339 parsing — only the timestamp
/// portion (digits + `T` + digits + `Z`). The function accepts
/// ISO-8601 timestamps of the form `YYYY-MM-DDTHH:MM:SS[.fff]Z`
/// or `YYYY-MM-DDTHH:MM:SS+00:00`.
fn parse_rfc3339_ms(s: &str) -> Option<u64> {
    // Best-effort: parse the leading YYYY-MM-DDTHH:MM:SS as a
    // calendar second, then add any fractional milliseconds.
    // The function is good enough for the heartbeat logic —
    // both timestamps in the tracker come from
    // `crate::store::now_rfc3339()` which is RFC3339 with
    // millisecond precision in the test harness.
    if s.len() < 19 {
        return None;
    }
    let y: i32 = s.get(0..4)?.parse().ok()?;
    let mo: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    let h: u32 = s.get(11..13)?.parse().ok()?;
    let mi: u32 = s.get(14..16)?.parse().ok()?;
    let se: u32 = s.get(17..19)?.parse().ok()?;
    let frac_ms: u64 = if s.len() >= 23 && s.as_bytes()[19] == b'.' {
        let ms_str = s.get(20..23).unwrap_or("000");
        ms_str.parse().unwrap_or(0)
    } else {
        0
    };
    Some(unix_seconds(y, mo, d, h, mi, se) * 1000 + frac_ms)
}

/// Convert a broken-down UTC timestamp to Unix epoch seconds.
/// Approximate (uses 30/31-day month rules); the heartbeat logic
/// tolerates ±1s drift.
fn unix_seconds(y: i32, mo: u32, d: u32, h: u32, mi: u32, se: u32) -> u64 {
    let days_from_civil = |y: i32, m: u32, d: u32| -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as u32; // [0, 399]
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
        era as i64 * 146097 + doe as i64 - 719468
    };
    let secs = days_from_civil(y, mo, d) * 86400 + (h as i64) * 3600 + (mi as i64) * 60 + se as i64;
    secs.max(0) as u64
}

// ─── predict_next_action (AC-07) ────────────────────────────────────────

/// Typed next-action enum returned by [`predict_next_action`].
/// The TUI consumes this directly so users can see "what's next"
/// for each queued milestone. The closed set is exactly the
/// 6 outcomes the spec mandates plus a `NoOp` sentinel for
/// terminal states (not advertised as a new outcome; it's the
/// "no transition pending" case the TUI renders as "—").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NextAction {
    /// Cycle is in `Dispatching` and the runner has not been
    /// notified yet — compose + send the runner assignment.
    DispatchRunner,
    /// 3-pane / 2-pane: runner done; compose + send the
    /// reviewer assignment.
    DispatchReviewer,
    /// Runner was dispatched; the cycle engine is waiting for
    /// the "completed-execute" notification.
    AwaitRunner,
    /// Reviewer was dispatched; the cycle engine is waiting for
    /// the reviewer's verdict.
    AwaitReviewer,
    /// Verdict (or 1-pane runner verdict) is in hand — apply
    /// the decision matrix.
    ApplyMatrix,
    /// Cycle cap or topology-block reached — escalate to the
    /// human operator.
    EscalateUser,
    /// Terminal or no transition pending — the TUI renders
    /// this as "—".
    NoOp,
}

impl NextAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            NextAction::DispatchRunner => "dispatch-runner",
            NextAction::DispatchReviewer => "dispatch-reviewer",
            NextAction::AwaitRunner => "await-runner",
            NextAction::AwaitReviewer => "await-reviewer",
            NextAction::ApplyMatrix => "apply-matrix",
            NextAction::EscalateUser => "escalate-user",
            NextAction::NoOp => "no-op",
        }
    }
}

/// Pure function: return the next action the cycle engine would
/// take for the supplied state and recent event journal. The
/// function is intentionally side-effect-free — callers feed it a
/// snapshot and get back a [`NextAction`].
///
/// The TUI consumes this directly so users see "what's next" for
/// each queued milestone.
pub fn predict_next_action(
    cycle: &CycleStateMachine,
    recent_events: &[OrchestrationEvent],
) -> NextAction {
    // Tail of the event journal — the cycle engine treats the
    // most recent event as authoritative when more than one
    // candidate applies.
    let last_event_kind = recent_events
        .last()
        .map(|e| e.kind)
        .unwrap_or(crate::autopilot::EventKind::Dispatch);

    match cycle.state {
        CycleState::Dispatching => {
            // If the journal shows a fresh runner dispatch in the
            // tail, the runner is now in flight — AwaitRunner.
            if last_event_kind == crate::autopilot::EventKind::AssignmentDispatched {
                NextAction::AwaitRunner
            } else {
                NextAction::DispatchRunner
            }
        }
        CycleState::WaitingRunner => NextAction::AwaitRunner,
        CycleState::Reviewing => {
            // The cycle state machine maps
            // `Reviewing -> WaitingReviewer` via the reviewer
            // dispatch event. From the predict-action helper's
            // perspective, `Reviewing` is the "compose +
            // dispatch the reviewer" state.
            if last_event_kind == crate::autopilot::EventKind::AssignmentDispatched {
                NextAction::AwaitReviewer
            } else {
                NextAction::DispatchReviewer
            }
        }
        CycleState::Deciding => NextAction::ApplyMatrix,
        CycleState::CycleNext => NextAction::DispatchRunner,
        CycleState::Complete => NextAction::NoOp,
        CycleState::Escalate => NextAction::EscalateUser,
    }
}

// ─── Diagnostic surface ─────────────────────────────────────────────────

/// Summary stats for the cycle engine. Surfaced through
/// `mp autopilot cycle status` and the TUI's per-milestone row.
/// Not part of the cycle-flow contract; read-only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleStats {
    pub cycle: u32,
    pub state: Option<CycleState>,
    pub verdict_count_by_kind: BTreeMap<String, u32>,
}

impl CycleStats {
    pub fn from_cycle_state_machine(cycle: &CycleStateMachine) -> Self {
        let mut verdict_count_by_kind = BTreeMap::new();
        for entry in &cycle.cycle_history {
            *verdict_count_by_kind
                .entry(entry.verdict.clone())
                .or_insert(0) += 1;
        }
        Self {
            cycle: cycle.cycle,
            state: Some(cycle.state),
            verdict_count_by_kind,
        }
    }

    /// Serialize as a JSON object (used by the `mp autopilot
    /// cycle status` command).
    pub fn to_json(&self) -> Value {
        json!({
            "cycle": self.cycle,
            "state": self.state.map(|s| s.as_str()),
            "verdict_count_by_kind": self.verdict_count_by_kind,
        })
    }
}

// ─── Test helpers (exported for the integration suite) ──────────────────

/// Build a JSON snapshot of the cycle state machine for the
/// integration tests. The schema is intentionally tiny — only the
/// fields the AC-01 / AC-04 / AC-07 tests pin.
pub fn cycle_state_machine_to_json(cycle: &CycleStateMachine) -> Value {
    json!({
        "state": cycle.state.as_str(),
        "milestone_id": cycle.milestone_id,
        "cycle": cycle.cycle,
        "topology": cycle.topology.as_str(),
        "policy_mode": cycle.policy.mode.as_str(),
        "policy_cycle_budget": cycle.policy.cycle_budget,
        "cycle_history": cycle.cycle_history,
    })
}

/// Convenience: build a per-role working-on snapshot used by the
/// reviewer-activation integration test.
pub fn working_on_for(milestone_id: impl Into<String>, cycle: u32, role: Role) -> WorkingOn {
    let role_name = match role {
        Role::Orchestrator => RoleName::Orchestrator,
        Role::Runner => RoleName::Runner,
        Role::Reviewer => RoleName::Reviewer,
    };
    WorkingOn {
        milestone_id: milestone_id.into(),
        cycle,
        role: Some(role_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(low: u32, high: u32, correctness: u32, blocker: u32) -> FindingSummary {
        FindingSummary {
            high_severity_count: high,
            correctness_count: correctness,
            low_count: low,
            blocker_count: blocker,
        }
    }

    fn input(
        cycle: u32,
        topology: Topology,
        verdict: ReviewerVerdict,
        f: FindingSummary,
    ) -> DecisionInput {
        let policy = crate::autopilot::role::topology_policy(topology);
        DecisionInput {
            cycle,
            topology,
            policy,
            verdict,
            findings: f,
            cycle_history: Vec::new(),
        }
    }

    #[test]
    fn cycle_state_machine_starts_in_dispatching() {
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        assert_eq!(s.state, CycleState::Dispatching);
        assert_eq!(s.cycle, 1);
        assert_eq!(s.topology, Topology::ThreeAgent);
    }

    #[test]
    fn happy_path_walks_through_one_full_cycle() {
        // Dispatching -> WaitingRunner -> Reviewing -> Deciding
        //   -> CycleNext -> Dispatching (cycle=2)
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        let s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
        assert_eq!(s.state, CycleState::WaitingRunner);
        let s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
        assert_eq!(s.state, CycleState::Reviewing);
        let s = s.advance(CycleEvent::ReviewerVerdict {
            pane: "%3".into(),
            verdict: ReviewerVerdict::Pass,
            findings: findings(0, 0, 0, 0),
        });
        assert_eq!(s.state, CycleState::Deciding);
        let s = s.advance(CycleEvent::StateTick);
        assert_eq!(s.state, CycleState::CycleNext);
        let s = s.advance(CycleEvent::StateTick);
        assert_eq!(s.state, CycleState::Dispatching);
        assert_eq!(s.cycle, 2);
    }

    #[test]
    fn one_pane_skips_reviewing_state() {
        let s = CycleStateMachine::new("M213", Topology::OneAgent);
        let s = s.advance(CycleEvent::RunnerDispatched { pane: "%1".into() });
        let s = s.advance(CycleEvent::RunnerCompleted { pane: "%1".into() });
        // 1-pane must skip Reviewing and land on Deciding.
        assert_eq!(s.state, CycleState::Deciding);
    }

    #[test]
    fn two_pane_keeps_reviewing_state() {
        // 2-pane is NoShipWithBacklog but Reviewing still exists.
        let s = CycleStateMachine::new("M213", Topology::TwoAgent);
        let s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
        let s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
        assert_eq!(s.state, CycleState::Reviewing);
    }

    #[test]
    fn terminal_states_are_absorbing() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        s.state = CycleState::Complete;
        let s1 = s.advance(CycleEvent::StateTick);
        assert_eq!(s1.state, CycleState::Complete);

        s.state = CycleState::Escalate;
        let s2 = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
        assert_eq!(s2.state, CycleState::Escalate);
    }

    #[test]
    fn invalid_transition_is_a_no_op() {
        // Dispatching + ReviewerVerdict is not a real path.
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        let s = s.advance(CycleEvent::ReviewerVerdict {
            pane: "%3".into(),
            verdict: ReviewerVerdict::Pass,
            findings: findings(0, 0, 0, 0),
        });
        assert_eq!(s.state, CycleState::Dispatching);
    }

    #[test]
    fn decision_matrix_complete_for_clean_pass() {
        let out = apply_decision_matrix(&input(
            1,
            Topology::ThreeAgent,
            ReviewerVerdict::Pass,
            findings(0, 0, 0, 0),
        ));
        assert_eq!(out, CycleDecision::Complete);
    }

    #[test]
    fn decision_matrix_cycle_next_for_high_severity() {
        let out = apply_decision_matrix(&input(
            1,
            Topology::ThreeAgent,
            ReviewerVerdict::Fail,
            findings(0, 1, 0, 0),
        ));
        assert_eq!(out, CycleDecision::CycleNext);
    }

    #[test]
    fn decision_matrix_ship_with_backlog_for_low_findings_three_pane() {
        let out = apply_decision_matrix(&input(
            1,
            Topology::ThreeAgent,
            ReviewerVerdict::PassWithBacklog,
            findings(2, 0, 0, 0),
        ));
        assert_eq!(out, CycleDecision::ShipWithBacklog);
    }

    #[test]
    fn decision_matrix_forces_cycle_next_for_pass_with_backlog_in_two_pane() {
        let out = apply_decision_matrix(&input(
            1,
            Topology::TwoAgent,
            ReviewerVerdict::PassWithBacklog,
            findings(2, 0, 0, 0),
        ));
        assert_eq!(out, CycleDecision::CycleNext);
    }

    #[test]
    fn decision_matrix_escalates_at_cycle_cap() {
        // Even a Pass at cycle=4 (full-matrix budget) escalates.
        let out = apply_decision_matrix(&input(
            4,
            Topology::ThreeAgent,
            ReviewerVerdict::Pass,
            findings(0, 0, 0, 0),
        ));
        assert!(matches!(out, CycleDecision::Escalate { .. }));
    }

    #[test]
    fn reviewer_mode_flips_to_blockers_only_at_cycle_2() {
        assert_eq!(reviewer_mode_for_cycle(1), ReviewerMode::Full);
        assert_eq!(reviewer_mode_for_cycle(2), ReviewerMode::BlockersOnly);
        assert_eq!(reviewer_mode_for_cycle(4), ReviewerMode::BlockersOnly);
    }

    #[test]
    fn heartbeat_healthy_when_state_unchanged_even_if_quiet() {
        // A responsive lane may stay in the same workflow
        // state without timing out (AC-06).
        let tracker = HeartbeatTracker {
            lane: RoleName::Runner,
            last_ack_at: "2026-01-01T00:00:00Z".to_string(),
            last_acknowledged_state: HeartbeatAckState::Working,
            current_state: HeartbeatAckState::Working,
            state_changed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        // No state change -> Healthy even at far-future `now`.
        let out = classify_liveness(&tracker, u64::MAX, 1000);
        assert!(matches!(out, LivenessStatus::Healthy));
    }

    #[test]
    fn heartbeat_stale_only_when_missed_and_state_changed() {
        let tracker = HeartbeatTracker {
            lane: RoleName::Runner,
            last_ack_at: "2026-01-01T00:00:00Z".to_string(),
            last_acknowledged_state: HeartbeatAckState::Working,
            current_state: HeartbeatAckState::Blocked,
            state_changed_at: "2026-01-01T00:00:30Z".to_string(),
        };
        // Compute "now" 60s after the ack. Heartbeat missed
        // (60_000ms > 1000ms) AND state changed (Working ->
        // Blocked) -> StaleStateTimeout.
        let ack_ms = parse_rfc3339_ms(&tracker.last_ack_at).unwrap();
        let now_ms = ack_ms + 60_000;
        let out = classify_liveness(&tracker, now_ms, 1000);
        assert!(out.is_stale());
    }

    #[test]
    fn heartbeat_healthy_when_ack_is_recent() {
        let tracker = HeartbeatTracker {
            lane: RoleName::Runner,
            last_ack_at: "2026-01-01T00:00:00Z".to_string(),
            last_acknowledged_state: HeartbeatAckState::Working,
            current_state: HeartbeatAckState::Working,
            state_changed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let ack_ms = parse_rfc3339_ms(&tracker.last_ack_at).unwrap();
        // now=ack+500ms, timeout=1000ms -> ack is fresh.
        let out = classify_liveness(&tracker, ack_ms + 500, 1000);
        assert!(matches!(out, LivenessStatus::Healthy));
    }

    #[test]
    fn predict_next_action_dispatches_runner_when_dispatching() {
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::DispatchRunner);
    }

    #[test]
    fn predict_next_action_awaits_runner_after_dispatch() {
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        let s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::AwaitRunner);
    }

    #[test]
    fn predict_next_action_dispatches_reviewer_under_three_pane() {
        let s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        let s = s.advance(CycleEvent::RunnerDispatched { pane: "%2".into() });
        let s = s.advance(CycleEvent::RunnerCompleted { pane: "%2".into() });
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::DispatchReviewer);
    }

    #[test]
    fn predict_next_action_applies_matrix_when_deciding() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        s.state = CycleState::Deciding;
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::ApplyMatrix);
    }

    #[test]
    fn predict_next_action_escalates_user_at_terminal_escalate() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        s.state = CycleState::Escalate;
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::EscalateUser);
    }

    #[test]
    fn predict_next_action_no_op_at_complete() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        s.state = CycleState::Complete;
        let out = predict_next_action(&s, &[]);
        assert_eq!(out, NextAction::NoOp);
    }

    #[test]
    fn predict_next_action_terminal_mapping() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        for (terminal, expected) in [
            (CycleState::Complete, NextAction::NoOp),
            (CycleState::Escalate, NextAction::EscalateUser),
        ] {
            s.state = terminal;
            assert_eq!(predict_next_action(&s, &[]), expected);
        }
    }

    #[test]
    fn reviewer_activation_carries_milestone_cycle_evidence_revision() {
        let payload = ReviewRequestPayload {
            milestone_id: "M213".into(),
            cycle: 1,
            evidence_revision: "rev-abc".into(),
            reviewer_actor_token: "reviewer-pane-1".into(),
            mode: ReviewerMode::Full,
        };
        let assignment = build_reviewer_activation("session-1", &payload, "%3");
        assert_eq!(assignment.milestone_id, "M213");
        assert_eq!(assignment.cycle, 1);
        assert_eq!(assignment.target_pane, "%3");
        assert_eq!(assignment.direction, RoleDirection::OrchestratorToReviewer);
        assert!(assignment.task.contains("milestone_id=M213"));
        assert!(assignment.task.contains("cycle=1"));
        assert!(assignment.task.contains("evidence_revision=rev-abc"));
        assert!(assignment
            .task
            .contains("reviewer_actor_token=reviewer-pane-1"));
        assert!(assignment.task.contains("mode=full"));
    }

    #[test]
    fn topology_preflight_blocks_1_pane_full_milestone() {
        let err = start_cycle(
            "M213",
            Topology::OneAgent,
            MilestoneKind::Full,
            crate::autopilot::role::ReviewBypassPolicy::None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CycleStartError::TopologyPreflight(
                crate::autopilot::role::TopologyPreflightError::FullMilestoneRequiresReviewer { .. }
            )
        ));
    }

    #[test]
    fn cycle_stats_count_verdicts_by_kind() {
        let mut s = CycleStateMachine::new("M213", Topology::ThreeAgent);
        s.cycle_history.push(CycleVerdictRecord {
            cycle: 1,
            verdict: "pass-with-backlog".into(),
            findings: findings(2, 0, 0, 0),
            topology_mode: "full_matrix".into(),
        });
        s.cycle_history.push(CycleVerdictRecord {
            cycle: 2,
            verdict: "pass".into(),
            findings: findings(0, 0, 0, 0),
            topology_mode: "full_matrix".into(),
        });
        let stats = CycleStats::from_cycle_state_machine(&s);
        assert_eq!(stats.cycle, 1);
        assert_eq!(stats.verdict_count_by_kind.get("pass"), Some(&1));
        assert_eq!(
            stats.verdict_count_by_kind.get("pass-with-backlog"),
            Some(&1)
        );
    }
}
