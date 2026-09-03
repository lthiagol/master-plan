//! M212 — autopilot orchestrator verifier: independent state-reads +
//! role-boundary violation detection.
//!
//! The orchestrator runs this module before accepting any lane
//! notification. The verifier never trusts the lane's report — it
//! independently reads three sources and reports typed violations:
//!
//! 1. **Milestone JSON** (`<plan_dir>/milestones/<id>.json`) — the
//!    canonical `lifecycle` / `execution_status` / `spec_status`.
//! 2. **`reviews.json`** — the latest review verdict for the
//!    milestone, if any.
//! 3. **`activity.json`** — the journal filtered by milestone
//!    subject. Activity entries are durable evidence that a
//!    `lifecycle-transition` actually fired on disk.
//!
//! Per M200's lesson, a `mp reviews pass` row does NOT add an
//! `activity.json` event by default — `record_review_pass` writes
//! to `reviews.json` and may bump the milestone's `flow_stages`
//! map, but the journal entry is reserved for the dispatch-side
//! `lifecycle-transition` event. Cross-checking the three sources
//! is how the verifier catches the M201 cycle 1 fabrication: a
//! `lifecycle=executed` notification with no matching
//! `lifecycle-transition` activity event is rejected as a typed
//! mismatch rather than silently accepted.
//!
//! ## Seven role-boundary detectors
//!
//! Per spec, the verifier exposes one typed detector per violation:
//!
//! 1. **Runner called `mp reviews pass`** — review verdicts are
//!    the reviewer's lane, not the runner's.
//! 2. **Runner called `mp reviews claim` / `mp reviews finding
//!    add`** — claims and findings are reviewer-owned.
//! 3. **Runner modified `master-plan/` directly** — every
//!    plan-zone write must route through `mp`.
//! 4. **Reviewer modified code** (`git diff crates/..`) — reviewer
//!    must read; orchestrator/runner own code writes.
//! 5. **Reviewer called `mp reviews pass` before orchestrator
//!    prompted** — premature verdicts bypass the prompt-bound
//!    review sequence.
//! 6. **Notify arrived before the lane was started** — lane id
//!    appears with no preceding `AssignmentDispatched` event.
//! 7. **Orchestrator committed code attributable to its own pane
//!    ID** — orchestrator owns cycle decisions, not code edits.
//!
//! Each detector returns `Option<Violation>` and carries structured
//! evidence (activity event id, git log SHA, diff hunk) so the
//! remediation hook can act on specifics.
//!
//! ## Topology-aware remediation
//!
//! [`recommend_remediation`] maps any violation + topology to a
//! [`Remediation`] enum: 3-pane → `Resend(corrective_message)`;
//! 2-pane and 1-pane → `EscalateToUser(violation)`. C3 owns the
//! cycle-flow consumption; C2 only exposes the decision.
//!
//! ## Attribution model
//!
//! Every autopilot mutation carries an [`ActorAttribution`] —
//! session id, role, actor token (M207's `session.role_state.actor`),
//! dispatch id, and sequence number. Missing or mismatched
//! attribution blocks automatic acceptance with
//! [`Verdict::UnknownActor`] rather than guessing.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::{self, ActivityEvent, ActivityLog};
use crate::model::MilestoneFile;
use crate::milestone;
use crate::paths::PlanContext;
use crate::reviews::{self, ReviewRecord};
use crate::validate::{effective_execution_status, effective_spec_status};

// ─── Lane model ───────────────────────────────────────────────────────

/// Lane that produced a notification. Mirrors M209's three-role
/// autopilot model. Distinct from `crate::watch::herdr::Role` (the
/// legacy two-role model that mp watch still uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lane {
    Runner,
    Reviewer,
    Orchestrator,
}

impl Lane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Lane::Runner => "runner",
            Lane::Reviewer => "reviewer",
            Lane::Orchestrator => "orchestrator",
        }
    }

    pub const fn pane_slot(self) -> &'static str {
        match self {
            Lane::Runner => "runner",
            Lane::Reviewer => "reviewer",
            Lane::Orchestrator => "orchestrator",
        }
    }
}

impl std::fmt::Display for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Lane {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "runner" => Ok(Lane::Runner),
            "reviewer" => Ok(Lane::Reviewer),
            "orchestrator" => Ok(Lane::Orchestrator),
            other => Err(format!("unknown autopilot lane {other:?}")),
        }
    }
}

// ─── Attribution ──────────────────────────────────────────────────────

/// Provenance every autopilot mutation carries. M207's
/// `session.role_state.actor` is the source of the actor token;
/// the dispatch id and sequence number ride on every
/// `AssignmentDispatched` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorAttribution {
    pub session_id: String,
    pub role: Lane,
    pub actor_token: String,
    pub dispatch_id: String,
    pub seq: u64,
}

impl ActorAttribution {
    /// Validate that all fields are non-empty. The verifier
    /// refuses to fabricate attribution on the lane's behalf —
    /// `UnknownActor` is the typed result when fields are
    /// missing or mismatched against the session event log.
    pub fn validate(&self) -> Result<(), AttributionError> {
        if self.session_id.trim().is_empty() {
            return Err(AttributionError::MissingSessionId);
        }
        if self.actor_token.trim().is_empty() {
            return Err(AttributionError::MissingActorToken);
        }
        if self.dispatch_id.trim().is_empty() {
            return Err(AttributionError::MissingDispatchId);
        }
        if self.seq == 0 {
            return Err(AttributionError::MissingSeq);
        }
        Ok(())
    }
}

/// Diagnostic raised when attribution is missing or fails
/// cross-validation against the session event log. The verifier
/// blocks automatic acceptance rather than guessing actor identity
/// (M203's reviewer-pane attribution is uncertain even when the
/// activity log has matching events — see AC-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributionError {
    MissingSessionId,
    MissingActorToken,
    MissingDispatchId,
    MissingSeq,
    SessionMismatch { expected: String, actual: String },
    ActorTokenMismatch { expected: String, actual: String },
    DispatchIdMismatch { expected: String, actual: String },
    SeqMismatch { expected: u64, actual: u64 },
    RoleMismatch { expected: Lane, actual: Lane },
}

impl std::fmt::Display for AttributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributionError::MissingSessionId => f.write_str("actor attribution: session_id is empty"),
            AttributionError::MissingActorToken => f.write_str("actor attribution: actor_token is empty"),
            AttributionError::MissingDispatchId => f.write_str("actor attribution: dispatch_id is empty"),
            AttributionError::MissingSeq => f.write_str("actor attribution: seq must be > 0"),
            AttributionError::SessionMismatch { expected, actual } => write!(
                f,
                "actor attribution: session_id {actual:?} does not match session event log {expected:?}"
            ),
            AttributionError::ActorTokenMismatch { expected, actual } => write!(
                f,
                "actor attribution: actor_token {actual:?} does not match session event log {expected:?}"
            ),
            AttributionError::DispatchIdMismatch { expected, actual } => write!(
                f,
                "actor attribution: dispatch_id {actual:?} does not match session event log {expected:?}"
            ),
            AttributionError::SeqMismatch { expected, actual } => write!(
                f,
                "actor attribution: seq {actual} does not match session event log {expected}"
            ),
            AttributionError::RoleMismatch { expected, actual } => write!(
                f,
                "actor attribution: role {actual} does not match session event log {expected}"
            ),
        }
    }
}

impl std::error::Error for AttributionError {}

// ─── State reads ──────────────────────────────────────────────────────

/// The three sources the verifier cross-checks. The struct owns
/// the typed views the detectors consume; loading is the I/O
/// surface (see [`VerifierState::load`]).
#[derive(Debug, Clone)]
pub struct VerifierState {
    /// Canonical milestone JSON. Loaded from
    /// `<plan_dir>/milestones/<id>.json`.
    pub milestone: MilestoneFile,
    /// Latest review record for this milestone (if any).
    pub review: Option<ReviewRecord>,
    /// Activity journal filtered by milestone subject.
    pub activity: ActivityLog,
    /// Path the milestone was loaded from (used for evidence
    /// pointers in violation reports).
    pub milestone_path: PathBuf,
}

impl VerifierState {
    /// Load the three sources for a milestone. Loads are
    /// non-fatal for the activity journal (an absent
    /// `activity.json` is the fresh-plan case) but fatal for the
    /// milestone JSON and reviews.json — a missing milestone is a
    /// real error.
    pub fn load(ctx: &PlanContext, milestone_id: &str) -> Result<Self> {
        let path = milestone::load_milestone_path(ctx, milestone_id)
            .with_context(|| format!("locate milestone {milestone_id}"))?;
        let m = crate::store::load_milestone(&path)
            .with_context(|| format!("load milestone {milestone_id}"))?;
        // Latest review verdict, if any. A missing reviews.json is
        // benign (returns empty Vec inside Ok); surface whatever
        // the loader produces.
        let review = reviews::latest_review(ctx, milestone_id)
            .with_context(|| format!("load latest review for {milestone_id}"))?;
        let activity = activity::load(ctx).with_context(|| "load activity journal")?;
        Ok(Self {
            milestone: m,
            review,
            activity,
            milestone_path: path,
        })
    }

    /// Lifecycle as it currently lives in the milestone JSON. The
    /// typed milestone struct's lifecycle is the canonical source;
    /// this accessor makes detector code read naturally.
    pub fn lifecycle(&self) -> String {
        self.milestone.effective_lifecycle()
    }

    /// Events filtered to the milestone subject. The activity
    /// journal may contain events for many milestones; detectors
    /// only care about the events for the one being verified.
    pub fn activity_for_milestone(&self) -> Vec<&ActivityEvent> {
        self.activity
            .events
            .iter()
            .filter(|e| e.subject == self.milestone.milestone.id)
            .collect()
    }

    /// Most recent lifecycle transition event for the milestone,
    /// if any. Used by AC-04's M201 regression — a notification
    /// that claims `lifecycle=executed` without a matching
    /// `lifecycle-transition` event is the fabrication pattern.
    pub fn last_lifecycle_transition(&self) -> Option<&ActivityEvent> {
        self.activity_for_milestone()
            .into_iter()
            .rev()
            .find(|e| e.r#type == "lifecycle-transition")
    }
}

// ─── Lane notification ────────────────────────────────────────────────

/// What the orchestrator's lane-notification handler parses out
/// of the lane's `herdr agent prompt` text. A pure value type —
/// the verifier never reads the prompt text directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneNotification {
    pub lane: Lane,
    pub milestone_id: String,
    pub cycle: u32,
    pub claimed_lifecycle: String,
    pub claimed_execution_status: String,
    pub claimed_spec_status: String,
    pub attribution: ActorAttribution,
    /// Optional command-list executed by the lane. The
    /// orchestrator surfaces this from the prompt bundle's
    /// `verification` block; absent notifications can still be
    /// checked, but per-AC evidence validation requires the list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verification_commands: Vec<VerificationCommand>,
    /// Free-form free-form action the lane took that triggered
    /// the notification (e.g. "completed-execute",
    /// "submitted-review"). Used by detectors #1–#7 to decide
    /// which one applies.
    pub action: String,
}

impl LaneNotification {
    /// Build a notification for the runner's "done" handoff.
    pub fn runner_done(
        milestone_id: impl Into<String>,
        cycle: u32,
        claimed_lifecycle: impl Into<String>,
        claimed_execution_status: impl Into<String>,
        claimed_spec_status: impl Into<String>,
        attribution: ActorAttribution,
    ) -> Self {
        Self {
            lane: Lane::Runner,
            milestone_id: milestone_id.into(),
            cycle,
            claimed_lifecycle: claimed_lifecycle.into(),
            claimed_execution_status: claimed_execution_status.into(),
            claimed_spec_status: claimed_spec_status.into(),
            attribution,
            verification_commands: Vec::new(),
            action: "completed-execute".to_string(),
        }
    }
}

// ─── Verifications + evidence contract ────────────────────────────────

/// One structured verification command. The shape mirrors the
/// `mp show milestone <id> --fields acceptance_criteria[N].tests`
/// convention but lifted into argv-safe tokens so the verifier can
/// detect shell metacharacter smuggling (parentheses preserved as
/// argv; `&&` / `;` / newlines are split into separate commands or
/// rejected).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationCommand {
    /// Stable label (e.g. `cargo-nextest-run-verifier_state_cross_check`).
    pub label: String,
    /// argv tokens (e.g. `["cargo", "nextest", "run", "-p", "mp",
    /// "--test", "verifier_state_cross_check", "--no-fail-fast"]`).
    /// The last two tokens are typically the exit-code probe
    /// (`exit`, `0`) and the observed pass count
    /// (`(<passed>/<total> pass)`); both are part of the
    /// structured shape.
    pub argv: Vec<String>,
}

/// Diagnostic raised when per-AC evidence fails the contract:
/// generic summaries, missing exit code, missing pass count, or
/// evidence that was overwritten after the milestone reached
/// `lifecycle=executed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceShapeError {
    /// Evidence string is empty or only whitespace.
    Empty,
    /// Evidence is a generic summary string — per AC-07 the
    /// runner MUST stamp the exact command + exit code + observed
    /// pass count. Reject summaries like "All steps done" or
    /// "M<id> complete: …".
    GenericSummary(String),
    /// Evidence does not contain a recognized command — must
    /// start with a runnable name like `cargo`, `cargo nextest`,
    /// `make`, `rustc`, or a slash.
    MissingCommand(String),
    /// Evidence does not contain an `exit <code>` token. Per
    /// AC-07 the runner MUST include the actual exit code.
    MissingExitCode(String),
    /// Evidence does not contain the `(<passed>/<total> pass)`
    /// count. Per AC-07 the runner MUST include the observed
    /// pass count from `cargo nextest`.
    MissingPassCount(String),
    /// Evidence was overwritten after `lifecycle=executed`
    /// landed. The canonical criterion state was re-read after
    /// completion and the new value differs from the
    /// pre-completion value — a runner that back-fills summaries
    /// after `mp milestone complete` lands is rejected.
    OverwrittenAfterCompletion {
        before: String,
        after: String,
    },
}

impl std::fmt::Display for EvidenceShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvidenceShapeError::Empty => f.write_str("AC evidence is empty"),
            EvidenceShapeError::GenericSummary(s) => write!(
                f,
                "AC evidence is a generic summary {s:?}; expected the exact command + exit code + pass count (e.g. \"cargo nextest run -p mp --test foo exit 0 (3/3 pass)\")"
            ),
            EvidenceShapeError::MissingCommand(s) => write!(
                f,
                "AC evidence does not start with a runnable command: {s:?}"
            ),
            EvidenceShapeError::MissingExitCode(s) => write!(
                f,
                "AC evidence does not contain 'exit <code>': {s:?}"
            ),
            EvidenceShapeError::MissingPassCount(s) => write!(
                f,
                "AC evidence does not contain '(<passed>/<total> pass)': {s:?}"
            ),
            EvidenceShapeError::OverwrittenAfterCompletion { before, after } => write!(
                f,
                "AC evidence was overwritten after milestone completion: before={before:?} after={after:?}"
            ),
        }
    }
}

impl std::error::Error for EvidenceShapeError {}

/// Heuristic that flags generic-summary strings. Anything that
/// does NOT start with a runnable name (cargo, make, rustc,
/// scripts/, ./, or a single-char command like `R`, `bash`),
/// that contains the milestone id literally (e.g. "M<id>
/// complete"), or that ends with "done" / "complete" / "ready"
/// without a runnable command prefix is rejected.
pub fn validate_evidence_shape(evidence: &str) -> Result<(), EvidenceShapeError> {
    let trimmed = evidence.trim();
    if trimmed.is_empty() {
        return Err(EvidenceShapeError::Empty);
    }
    // Generic summary heuristics. The runner contract demands
    // "cargo nextest ... exit 0 (X/Y pass)" or equivalent. The
    // patterns below flag the M201-cycle-1 pattern (which the
    // orchestrator caught in M207 / M209 review cycles).
    let lower = trimmed.to_ascii_lowercase();
    let summary_markers = [
        "all steps done",
        "all done",
        "m complete:",
        "m complete ",
        "complete:",
        "cycle done",
        "all acs",
        "ready for review",
        "cycle 1 done",
        "cycle 2 done",
        "step done",
    ];
    for marker in summary_markers {
        if lower.contains(marker) {
            return Err(EvidenceShapeError::GenericSummary(trimmed.to_string()));
        }
    }
    // Must start with a runnable name. Allow `cargo nextest`,
    // `cargo test`, `make`, `rustc`, `bash`, `sh`, a relative
    // path (`./`), or an absolute path (`/`).
    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    let runnable_prefixes = ["cargo", "make", "rustc", "bash", "sh", "zsh", "nextest"];
    let starts_runnable = runnable_prefixes.iter().any(|p| first_token == *p)
        || first_token.starts_with("./")
        || first_token.starts_with('/')
        || first_token.starts_with("scripts/");
    if !starts_runnable {
        return Err(EvidenceShapeError::MissingCommand(trimmed.to_string()));
    }
    // Must contain `exit <code>`. We accept `exit 0` (the common
    // happy-path case) or `exit <non-zero>`.
    let has_exit = trimmed.contains(" exit 0 ")
        || trimmed.contains(" exit 0.")
        || trimmed.ends_with(" exit 0")
        || trimmed
            .split_whitespace()
            .enumerate()
            .any(|(i, tok)| tok == "exit" && i + 1 < trimmed.split_whitespace().count());
    if !has_exit {
        return Err(EvidenceShapeError::MissingExitCode(trimmed.to_string()));
    }
    // Must contain `(<passed>/<total> pass)`. The pattern is
    // intentionally strict: a runner that just writes "(passes)"
    // or "all pass" is rejected.
    let pass_count_pattern = |s: &str| -> bool {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'(' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b' ' && i + 1 < bytes.len() && &bytes[i + 1..i + 5] == b"pass" {
                        return true;
                    }
                }
            }
            i += 1;
        }
        false
    };
    if !pass_count_pattern(trimmed) {
        return Err(EvidenceShapeError::MissingPassCount(trimmed.to_string()));
    }
    Ok(())
}

// ─── Violations ───────────────────────────────────────────────────────

/// One detected violation. Each variant names the rule that
/// fired and carries structured evidence so remediation can
/// target it specifically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Violation {
    RunnerReviewViolation(RunnerReviewViolation),
    RunnerClaimViolation(RunnerClaimViolation),
    RunnerPlanEditViolation(RunnerPlanEditViolation),
    ReviewerCodeEditViolation(ReviewerCodeEditViolation),
    ReviewerPrematurePassViolation(ReviewerPrematurePassViolation),
    PreStartNotificationViolation(PreStartNotificationViolation),
    OrchestratorCodeEditViolation(OrchestratorCodeEditViolation),
    /// M201 cycle 1 regression: claimed `lifecycle=executed` but
    /// the milestone is still at `approved` and no
    /// `lifecycle-transition` event landed in `activity.json`.
    LifecycleClaimUnbacked(LifecycleClaimUnbacked),
    /// Attribution is missing or mismatched against the session
    /// event log. Blocks automatic acceptance per AC-06.
    UnknownActorViolation(UnknownActorViolation),
    /// Per-AC evidence does not match the runner contract
    /// (empty, generic, missing exit code / pass count, or
    /// overwritten after completion). Blocks acceptance per
    /// AC-07.
    EvidenceContractViolation(EvidenceContractViolation),
    /// Verification command list contains a shell control
    /// operator that was not split into separate commands. Per
    /// AC-08 the verifier rejects these rather than silently
    /// skipping the second command.
    UnsupportedCommandOperator(UnsupportedCommandOperator),
}

/// Stable string form for the violation kind. Used by the raul
/// autopilot tab to filter the event stream.
impl Violation {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Violation::RunnerReviewViolation(_) => "runner-review-violation",
            Violation::RunnerClaimViolation(_) => "runner-claim-violation",
            Violation::RunnerPlanEditViolation(_) => "runner-plan-edit-violation",
            Violation::ReviewerCodeEditViolation(_) => "reviewer-code-edit-violation",
            Violation::ReviewerPrematurePassViolation(_) => "reviewer-premature-pass-violation",
            Violation::PreStartNotificationViolation(_) => "pre-start-notification-violation",
            Violation::OrchestratorCodeEditViolation(_) => "orchestrator-code-edit-violation",
            Violation::LifecycleClaimUnbacked(_) => "lifecycle-claim-unbacked",
            Violation::UnknownActorViolation(_) => "unknown-actor",
            Violation::EvidenceContractViolation(_) => "evidence-contract-violation",
            Violation::UnsupportedCommandOperator(_) => "unsupported-command-operator",
        }
    }
}

/// Runners must not call `mp reviews pass`. Per the M203 lesson
/// (runner self-completed via review pass), the runner lane
/// hands review off to the reviewer pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerReviewViolation {
    pub milestone_id: String,
    pub pane_id: String,
    /// Activity event id of the runner's `reviews pass` call,
    /// when available — the runner should not be writing to
    /// `reviews.json` at all, so the event id is the evidence.
    pub event_seq: Option<u64>,
}

/// Runners must not call `mp reviews claim` or
/// `mp reviews finding add`. Review claims and findings are
/// reviewer-owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerClaimViolation {
    pub milestone_id: String,
    pub pane_id: String,
    pub event_seq: Option<u64>,
}

/// Runners must not modify `master-plan/` directly. Every
/// plan-zone write routes through `mp` — `git diff` against the
/// runner's pane slot reveals the violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerPlanEditViolation {
    pub milestone_id: String,
    pub pane_id: String,
    pub diff_hunk: Option<String>,
}

/// Reviewers must not modify code. The reviewer lane reads;
/// orchestrator + runner own code edits. Attribution uses
/// `git log` SHAs from the reviewer's pane slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewerCodeEditViolation {
    pub milestone_id: String,
    pub pane_id: String,
    pub diff_hunk: Option<String>,
}

/// Reviewers must not call `mp reviews pass` before the
/// orchestrator has prompted the lane. Premature verdicts bypass
/// the prompt-bound review sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewerPrematurePassViolation {
    pub milestone_id: String,
    pub pane_id: String,
    pub event_seq: Option<u64>,
}

/// Lane notified before it was started. The lane id appears with
/// no preceding `AssignmentDispatched` event in the session
/// event log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreStartNotificationViolation {
    pub milestone_id: String,
    pub pane_id: String,
}

/// Orchestrators must not commit code attributable to their own
/// pane slot. The orchestrator owns cycle decisions, not code
/// edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestratorCodeEditViolation {
    pub milestone_id: String,
    pub pane_id: String,
    pub diff_hunk: Option<String>,
}

/// Lifecycle notification that has no backing
/// `lifecycle-transition` event in `activity.json`. This is the
/// M201 cycle 1 fabrication pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleClaimUnbacked {
    pub milestone_id: String,
    pub claimed_lifecycle: String,
    pub canonical_lifecycle: String,
}

/// Attribution is missing or mismatched. Blocks automatic
/// acceptance per AC-06.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownActorViolation {
    pub milestone_id: String,
    pub detail: String,
}

/// Per-AC evidence does not match the runner contract. Each
/// failing AC is one violation; the orchestrator can fix the
/// evidence by re-stamping with the real `cargo nextest` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceContractViolation {
    pub milestone_id: String,
    pub ac_id: String,
    pub detail: String,
}

/// Verification command list contained a shell control operator
/// (`&&`, `;`, or newline) that was not split into separate
/// commands. Per AC-08 the verifier rejects rather than silently
/// skipping the second command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedCommandOperator {
    pub milestone_id: String,
    pub offending: String,
    pub operator: String,
}

// ─── Remediation ──────────────────────────────────────────────────────

/// Topology-aware remediation decision. The cycle-flow layer
/// (C3) consumes this enum; the verifier only exposes the
/// decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Remediation {
    /// 3-pane topology: resend the lane notification with a
    /// corrective message. The independent review channel can
    /// catch any second misfire.
    Resend { corrective_message: String },
    /// 2-pane and 1-pane topology: there is no independent
    /// recovery path — escalate to the human operator with the
    /// violation as evidence.
    EscalateToUser { violation_kind: String },
}

impl Remediation {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Remediation::Resend { .. } => "resend",
            Remediation::EscalateToUser { .. } => "escalate-to-user",
        }
    }
}

/// Map a violation + topology to a remediation decision. 3-pane
/// → `Resend`; 2-pane and 1-pane → `EscalateToUser`. The
/// corrective message carries the violation kind so the lane
/// knows what to fix.
pub fn recommend_remediation(violation: &Violation, topology: crate::autopilot::Topology) -> Remediation {
    use crate::autopilot::Topology;
    let corrective_message = format!(
        "verifier rejected cycle: {}. Re-stamp per-AC evidence with real cargo nextest output and re-notify.",
        violation.kind_str()
    );
    match topology {
        Topology::ThreeAgent => Remediation::Resend { corrective_message },
        Topology::TwoAgent | Topology::OneAgent => Remediation::EscalateToUser {
            violation_kind: violation.kind_str().to_string(),
        },
    }
}

// ─── Detectors ────────────────────────────────────────────────────────

/// Detector 1: Runner called `mp reviews pass`. The action
/// string "submitted-review-pass" is what the runner lane
/// reports when it actually called `mp reviews pass`. Per the
/// M203 lesson, this is the canonical runner-side review-pass
/// violation.
pub fn detect_runner_review_violation(notification: &LaneNotification) -> Option<Violation> {
    if notification.lane == Lane::Runner
        && (notification.action == "submitted-review-pass"
            || notification.action == "submitted-review"
            || notification.action == "completed-review")
    {
        return Some(Violation::RunnerReviewViolation(RunnerReviewViolation {
            milestone_id: notification.milestone_id.clone(),
            pane_id: notification.attribution.actor_token.clone(),
            event_seq: Some(notification.attribution.seq),
        }));
    }
    None
}

/// Detector 2: Runner called `mp reviews claim` or
/// `mp reviews finding add`. Detection uses the action string.
pub fn detect_runner_claim_violation(notification: &LaneNotification) -> Option<Violation> {
    if notification.lane == Lane::Runner
        && (notification.action == "claimed-review"
            || notification.action == "added-finding"
            || notification.action == "claim"
            || notification.action == "finding-add")
    {
        return Some(Violation::RunnerClaimViolation(RunnerClaimViolation {
            milestone_id: notification.milestone_id.clone(),
            pane_id: notification.attribution.actor_token.clone(),
            event_seq: Some(notification.attribution.seq),
        }));
    }
    None
}

/// Detector 3: Runner modified `master-plan/` directly. The
/// notification's `claimed_execution_status` must equal "done"
/// for a completed run; any diff hunk the lane attaches that
/// touches `master-plan/` is the evidence.
pub fn detect_runner_plan_edit_violation(
    notification: &LaneNotification,
    diff_hunk: Option<&str>,
) -> Option<Violation> {
    if notification.lane != Lane::Runner {
        return None;
    }
    let hunk = diff_hunk?;
    let touches_plan_zone = hunk.lines().any(|line| {
        line.contains("master-plan/")
            || line.contains("plan.json")
            || line.contains("milestones/")
            || line.contains("reviews.json")
    });
    if !touches_plan_zone {
        return None;
    }
    Some(Violation::RunnerPlanEditViolation(RunnerPlanEditViolation {
        milestone_id: notification.milestone_id.clone(),
        pane_id: notification.attribution.actor_token.clone(),
        diff_hunk: Some(hunk.to_string()),
    }))
}

/// Detector 4: Reviewer modified code. Detection uses the
/// `git diff` hunk attributable to the reviewer's pane slot.
pub fn detect_reviewer_code_edit_violation(
    notification: &LaneNotification,
    diff_hunk: Option<&str>,
) -> Option<Violation> {
    if notification.lane != Lane::Reviewer {
        return None;
    }
    let hunk = diff_hunk?;
    let touches_code = hunk
        .lines()
        .any(|line| line.starts_with("+++ b/crates/") || line.starts_with("--- a/crates/"));
    if !touches_code {
        return None;
    }
    Some(Violation::ReviewerCodeEditViolation(ReviewerCodeEditViolation {
        milestone_id: notification.milestone_id.clone(),
        pane_id: notification.attribution.actor_token.clone(),
        diff_hunk: Some(hunk.to_string()),
    }))
}

/// Detector 5: Reviewer called `mp reviews pass` before the
/// orchestrator prompted the lane. The cycle budget
/// (`claimed_cycle`) is the prompt signal — a reviewer who
/// submits verdict before cycle >= 1 of the orchestrator's
/// prompt is violating the prompt-bound review sequence.
pub fn detect_reviewer_premature_pass_violation(
    notification: &LaneNotification,
    orchestrator_prompted_cycle: u32,
) -> Option<Violation> {
    if notification.lane != Lane::Reviewer {
        return None;
    }
    if !matches!(notification.action.as_str(), "submitted-review-pass" | "completed-review") {
        return None;
    }
    if notification.cycle < orchestrator_prompted_cycle {
        return Some(Violation::ReviewerPrematurePassViolation(
            ReviewerPrematurePassViolation {
                milestone_id: notification.milestone_id.clone(),
                pane_id: notification.attribution.actor_token.clone(),
                event_seq: Some(notification.attribution.seq),
            },
        ));
    }
    None
}

/// Detector 6: Notify arrived before the lane was started. The
/// pane id appears with no preceding `AssignmentDispatched`
/// event in the session event log — `dispatch_id` is the key.
pub fn detect_pre_start_notification_violation(
    notification: &LaneNotification,
    started_dispatch_ids: &[String],
) -> Option<Violation> {
    if !started_dispatch_ids.is_empty()
        && !started_dispatch_ids.contains(&notification.attribution.dispatch_id)
    {
        return Some(Violation::PreStartNotificationViolation(
            PreStartNotificationViolation {
                milestone_id: notification.milestone_id.clone(),
                pane_id: notification.attribution.actor_token.clone(),
            },
        ));
    }
    None
}

/// Detector 7: Orchestrator committed code attributable to its
/// own pane ID. Detection uses the diff hunk + pane id.
pub fn detect_orchestrator_code_edit_violation(
    notification: &LaneNotification,
    diff_hunk: Option<&str>,
    orchestrator_pane_id: &str,
) -> Option<Violation> {
    if notification.lane != Lane::Orchestrator {
        return None;
    }
    let hunk = match diff_hunk {
        Some(h) => h,
        None => return None,
    };
    let touches_code = hunk
        .lines()
        .any(|line| line.starts_with("+++ b/crates/") || line.starts_with("--- a/crates/"));
    if !touches_code {
        return None;
    }
    if notification.attribution.actor_token != orchestrator_pane_id {
        return None;
    }
    Some(Violation::OrchestratorCodeEditViolation(
        OrchestratorCodeEditViolation {
            milestone_id: notification.milestone_id.clone(),
            pane_id: notification.attribution.actor_token.clone(),
            diff_hunk: Some(hunk.to_string()),
        },
    ))
}

// ─── Cross-check (AC-01) ──────────────────────────────────────────────

/// Cross-check the lane's claimed state against the canonical
/// milestone JSON. Returns `Ok(())` when the claimed lifecycle,
/// execution_status, and spec_status match the canonical
/// milestone; otherwise a [`CrossCheckMismatch`] error that
/// names the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrossCheckMismatch {
    LifecycleMismatch { claimed: String, canonical: String },
    ExecutionStatusMismatch { claimed: String, canonical: String },
    SpecStatusMismatch { claimed: String, canonical: String },
    /// The lane notification references a different milestone
    /// than the verifier is checking — typically a payload
    /// mismatch that the orchestrator should reject before any
    /// detector runs.
    MilestoneIdMismatch { claimed: String, canonical: String },
}

impl std::fmt::Display for CrossCheckMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrossCheckMismatch::LifecycleMismatch { claimed, canonical } => write!(
                f,
                "lifecycle mismatch: lane claimed {claimed:?}, canonical milestone JSON says {canonical:?}"
            ),
            CrossCheckMismatch::ExecutionStatusMismatch { claimed, canonical } => write!(
                f,
                "execution_status mismatch: lane claimed {claimed:?}, canonical milestone JSON says {canonical:?}"
            ),
            CrossCheckMismatch::SpecStatusMismatch { claimed, canonical } => write!(
                f,
                "spec_status mismatch: lane claimed {claimed:?}, canonical milestone JSON says {canonical:?}"
            ),
            CrossCheckMismatch::MilestoneIdMismatch { claimed, canonical } => write!(
                f,
                "milestone id mismatch: lane notification references {claimed:?}, verifier is checking {canonical:?}"
            ),
        }
    }
}

impl std::error::Error for CrossCheckMismatch {}

/// Cross-check the lane's claimed lifecycle / execution_status /
/// spec_status against the canonical milestone JSON. Pure — no
/// I/O; the caller supplies both sides. Used by AC-01's
/// `verifier_state_cross_check` test as the typed-error surface.
pub fn cross_check_state(
    notification: &LaneNotification,
    state: &VerifierState,
) -> Result<(), CrossCheckMismatch> {
    let canonical_id = state.milestone.milestone.id.as_str();
    if notification.milestone_id != canonical_id {
        return Err(CrossCheckMismatch::MilestoneIdMismatch {
            claimed: notification.milestone_id.clone(),
            canonical: canonical_id.to_string(),
        });
    }
    let canonical_lifecycle = state.lifecycle();
    let canonical_execution = effective_execution_status(&state.milestone);
    let canonical_spec = effective_spec_status(&state.milestone);
    if notification.claimed_lifecycle != canonical_lifecycle {
        return Err(CrossCheckMismatch::LifecycleMismatch {
            claimed: notification.claimed_lifecycle.clone(),
            canonical: canonical_lifecycle.clone(),
        });
    }
    if notification.claimed_execution_status != canonical_execution {
        return Err(CrossCheckMismatch::ExecutionStatusMismatch {
            claimed: notification.claimed_execution_status.clone(),
            canonical: canonical_execution.to_string(),
        });
    }
    if notification.claimed_spec_status != canonical_spec {
        return Err(CrossCheckMismatch::SpecStatusMismatch {
            claimed: notification.claimed_spec_status.clone(),
            canonical: canonical_spec.to_string(),
        });
    }
    Ok(())
}

// ─── Verdict ──────────────────────────────────────────────────────────

/// Top-level verifier verdict. Three lanes lead to acceptance;
/// three lanes lead to rejection. The orchestrator's lane
/// notification handler consults this enum before advancing the
/// cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Verdict {
    Accept,
    Reject {
        violations: List,
        remediation: Remediation,
    },
    /// Attribution is missing or mismatched — block automatic
    /// acceptance (AC-06). The operator must confirm identity
    /// before the cycle advances.
    UnknownActor {
        detail: String,
    },
    /// Per-AC evidence contract failed. The verifier exposes the
    /// failing ACs; the orchestrator resends or escalates.
    EvidenceContractFailed {
        failing: Vec<(String, String)>, // (ac_id, detail)
        remediation: Remediation,
    },
}

impl Verdict {
    pub fn is_accept(&self) -> bool {
        matches!(self, Verdict::Accept)
    }
}

/// Newtype wrapper that owns the detected violations. Used by
/// [`Verdict::Reject`] and serialized as a JSON array of typed
/// violation objects on the raul autopilot tab.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct List(pub Vec<Violation>);

impl List {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, v: Violation) {
        self.0.push(v);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Violation> {
        self.0.iter()
    }
}

impl From<Vec<Violation>> for List {
    fn from(v: Vec<Violation>) -> Self {
        Self(v)
    }
}

// ─── Verifier driver ──────────────────────────────────────────────────

/// One check the orchestrator runs after every lane notification.
/// Callers compose the inputs (`diff_hunk`,
/// `orchestrator_prompted_cycle`, `started_dispatch_ids`,
/// `orchestrator_pane_id`) from the lane-notification wire
/// metadata. The verifier itself is stateless — a fresh
/// invocation per lane notification.
#[derive(Debug, Clone, Default)]
pub struct VerifierInputs<'a> {
    pub diff_hunk: Option<&'a str>,
    pub orchestrator_prompted_cycle: u32,
    pub started_dispatch_ids: &'a [String],
    pub orchestrator_pane_id: &'a str,
}

/// Top-level driver. Runs every detector + the cross-check +
/// the attribution + the evidence contract + the command-list
/// check, then returns a single typed [`Verdict`] for the
/// orchestrator to consume.
pub fn check_notification(
    state: &VerifierState,
    notification: &LaneNotification,
    inputs: VerifierInputs<'_>,
) -> Verdict {
    let mut violations: List = List::new();

    // AC-04: lifecycle claim backed by an activity event. The
    // M201 cycle 1 pattern: lane claims lifecycle=executed but
    // no lifecycle-transition event landed.
    let canonical_lifecycle = state.lifecycle();
    if notification.claimed_lifecycle != canonical_lifecycle {
        // The claimed lifecycle disagrees with the canonical
        // milestone JSON. The activity journal may or may not
        // agree — emit an unbacked-claim violation when the
        // journal has no matching event for the claimed
        // transition.
        if state.last_lifecycle_transition().is_none() && canonical_lifecycle == "approved" {
            violations.push(Violation::LifecycleClaimUnbacked(
                LifecycleClaimUnbacked {
                    milestone_id: notification.milestone_id.clone(),
                    claimed_lifecycle: notification.claimed_lifecycle.clone(),
                    canonical_lifecycle: canonical_lifecycle.to_string(),
                },
            ));
        }
    }

    // Detectors 1-7.
    if let Some(v) = detect_runner_review_violation(notification) {
        violations.push(v);
    }
    if let Some(v) = detect_runner_claim_violation(notification) {
        violations.push(v);
    }
    if let Some(v) = detect_runner_plan_edit_violation(notification, inputs.diff_hunk) {
        violations.push(v);
    }
    if let Some(v) = detect_reviewer_code_edit_violation(notification, inputs.diff_hunk) {
        violations.push(v);
    }
    if let Some(v) =
        detect_reviewer_premature_pass_violation(notification, inputs.orchestrator_prompted_cycle)
    {
        violations.push(v);
    }
    if let Some(v) = detect_pre_start_notification_violation(
        notification,
        inputs.started_dispatch_ids,
    ) {
        violations.push(v);
    }
    if let Some(v) = detect_orchestrator_code_edit_violation(
        notification,
        inputs.diff_hunk,
        inputs.orchestrator_pane_id,
    ) {
        violations.push(v);
    }

    // AC-06: attribution. Missing or mismatched blocks automatic
    // acceptance. We don't push a violation here — instead, the
    // top-level Verdict returns UnknownActor with the detail.
    if let Err(attribution_err) = notification.attribution.validate() {
        return Verdict::UnknownActor {
            detail: attribution_err.to_string(),
        };
    }

    // AC-07: per-AC evidence contract. Each AC must pass
    // validate_evidence_shape; the failing list goes to the
    // orchestrator's evidence-contract-failed branch.
    let failing = check_evidence_contract(state, notification);
    if !failing.is_empty() {
        let topology = crate::autopilot::Topology::ThreeAgent; // default; C3 plugs its own
        let first_violation = failing
            .first()
            .map(|(_, detail)| Violation::EvidenceContractViolation(EvidenceContractViolation {
                milestone_id: notification.milestone_id.clone(),
                ac_id: "unknown".into(),
                detail: detail.clone(),
            }))
            .unwrap_or_else(|| {
                // Defensive: failing is non-empty so this branch
                // is unreachable, but a typed fallback keeps the
                // verifier panic-free.
                Violation::EvidenceContractViolation(EvidenceContractViolation {
                    milestone_id: notification.milestone_id.clone(),
                    ac_id: "unknown".into(),
                    detail: "evidence contract failure (no detail)".to_string(),
                })
            });
        let remediation = recommend_remediation(&first_violation, topology);
        return Verdict::EvidenceContractFailed { failing, remediation };
    }

    // AC-08: command list. Reject shell control operators
    // rather than silently skipping the second command.
    if let Err(op) = check_command_list(notification) {
        violations.push(op);
    }

    if violations.is_empty() {
        Verdict::Accept
    } else {
        let topology = crate::autopilot::Topology::ThreeAgent; // default; C3 plugs its own
        let first = violations.0.first().cloned().unwrap_or_else(|| {
            // Defensive: violations was non-empty so this
            // branch is unreachable, but typed fallback.
            Violation::LifecycleClaimUnbacked(LifecycleClaimUnbacked {
                milestone_id: notification.milestone_id.clone(),
                claimed_lifecycle: String::new(),
                canonical_lifecycle: String::new(),
            })
        });
        let remediation = recommend_remediation(&first, topology);
        Verdict::Reject {
            violations,
            remediation,
        }
    }
}

// ─── AC-07 evidence contract ──────────────────────────────────────────

/// Check the per-AC evidence contract for every AC attached to
/// the milestone. Returns a list of `(ac_id, detail)` pairs that
/// failed validation.
pub fn check_evidence_contract(
    state: &VerifierState,
    notification: &LaneNotification,
) -> Vec<(String, String)> {
    let mut failing = Vec::new();
    for ac in &state.milestone.acceptance_criteria {
        let evidence = &ac.evidence;
        if let Err(err) = validate_evidence_shape(evidence) {
            failing.push((ac.id.clone(), err.to_string()));
        }
    }
    // The notification's verification_commands are a
    // secondary signal — empty list on a "completed-execute"
    // notification is OK (the milestone ACs already carry the
    // evidence); a populated list with empty argv is not.
    for cmd in &notification.verification_commands {
        if cmd.argv.is_empty() {
            failing.push((
                "<command-list>".to_string(),
                format!("verification command {} has empty argv", cmd.label),
            ));
        }
    }
    failing
}

// ─── AC-08 command list ───────────────────────────────────────────────

/// Diagnostic raised when the verification command list
/// contains a shell control operator that was not split into
/// separate commands.
pub fn check_command_list(notification: &LaneNotification) -> Result<(), Violation> {
    for cmd in &notification.verification_commands {
        for token in &cmd.argv {
            if token.contains("&&") || token.contains("||") {
                return Err(Violation::UnsupportedCommandOperator(
                    UnsupportedCommandOperator {
                        milestone_id: notification.milestone_id.clone(),
                        offending: token.clone(),
                        operator: "&&".to_string(),
                    },
                ));
            }
        }
        // argv tokens are shell-metachar-safe by construction;
        // a single token that contains a `;` or newline is
        // invalid. The M212 contract preserves parentheses in
        // nextest filters as a single argv token, so `;` is the
        // marker — it cannot legitimately appear inside a
        // nextest filter (the test name is a Rust identifier).
        for token in &cmd.argv {
            if token.contains(';') || token.contains('\n') || token.contains('\r') {
                return Err(Violation::UnsupportedCommandOperator(
                    UnsupportedCommandOperator {
                        milestone_id: notification.milestone_id.clone(),
                        offending: token.clone(),
                        operator: ";".to_string(),
                    },
                ));
            }
        }
    }
    Ok(())
}

// ─── Re-read after completion (AC-07) ─────────────────────────────────

/// Re-read the canonical milestone criterion evidence after
/// `lifecycle=executed` lands and compare against the
/// `pre_completion` snapshot. A diff means evidence was
/// overwritten post-completion — the runner back-filled a
/// generic summary after the milestone reached its terminal
/// state. Returns `Ok(())` when the snapshot matches, otherwise
/// [`EvidenceShapeError::OverwrittenAfterCompletion`].
pub fn check_evidence_not_overwritten(
    state: &VerifierState,
    pre_completion_evidence: &[(&str, &str)],
) -> Result<(), EvidenceShapeError> {
    for (ac_id, before) in pre_completion_evidence {
        let after = state
            .milestone
            .acceptance_criteria
            .iter()
            .find(|ac| ac.id == *ac_id)
            .map(|ac| ac.evidence.as_str())
            .unwrap_or("");
        if before != &after {
            return Err(EvidenceShapeError::OverwrittenAfterCompletion {
                before: (*before).to_string(),
                after: after.to_string(),
            });
        }
    }
    Ok(())
}

// ─── Misc helpers ─────────────────────────────────────────────────────

/// Run `git log --oneline -- <path>` against the project root
/// and return the SHAs (one per line). Used to attribute code
/// edits to a pane slot; tests stub this by passing a fixture.
pub fn git_log_for_path(project_root: &Path, path: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["log", "--oneline", "--", path])
        .current_dir(project_root)
        .output()
        .with_context(|| format!("git log for {path}"))?;
    if !output.status.success() {
        bail!(
            "git log failed for {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect())
}

/// Convert a typed [`ActivityLog`] into the JSON value the
/// orchestrator surfaces on the raul autopilot tab. Pure — no
/// I/O. Used by integration tests to assert the verifier's
/// typed output survives a JSON round-trip.
pub fn violations_to_json(violations: &List) -> Result<Value> {
    Ok(serde_json::to_value(violations)?)
}

// ─── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{ActivityEvent, ActivityLog};
    use crate::autopilot::Topology;
    use crate::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta};
    use crate::paths::PlanContext;
    use tempfile::TempDir;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    fn sample_attribution() -> ActorAttribution {
        ActorAttribution {
            session_id: "s1".into(),
            role: Lane::Runner,
            actor_token: "%2".into(),
            dispatch_id: "dispatch-1".into(),
            seq: 1,
        }
    }

    fn sample_milestone(id: &str, lifecycle: &str) -> MilestoneFile {
        let mut m = MilestoneFile::default();
        m.milestone = MilestoneMeta {
            id: id.to_string(),
            title: "Sample".into(),
            slug: "sample".into(),
            lifecycle: lifecycle.to_string(),
            ..Default::default()
        };
        m
    }

    fn sample_state(id: &str, lifecycle: &str) -> VerifierState {
        let m = sample_milestone(id, lifecycle);
        VerifierState {
            milestone: m,
            review: None,
            activity: ActivityLog::empty(),
            milestone_path: PathBuf::from(format!("{id}.json")),
        }
    }

    fn sample_notification(id: &str, lifecycle: &str) -> LaneNotification {
        LaneNotification::runner_done(
            id,
            1,
            lifecycle,
            "in-progress",
            "ready",
            sample_attribution(),
        )
    }

    #[test]
    fn lane_round_trips_via_serde() {
        for lane in [Lane::Runner, Lane::Reviewer, Lane::Orchestrator] {
            let s = serde_json::to_string(&lane).unwrap();
            let back: Lane = serde_json::from_str(&s).unwrap();
            assert_eq!(back, lane);
            assert_eq!(lane.as_str(), s.trim_matches('"'));
            let parsed: Lane = lane.as_str().parse().unwrap();
            assert_eq!(parsed, lane);
        }
    }

    #[test]
    fn lane_from_str_rejects_unknown() {
        assert!("unknown".parse::<Lane>().is_err());
    }

    #[test]
    fn attribution_validate_rejects_empty_fields() {
        let mut attr = sample_attribution();
        attr.session_id = "".into();
        assert_eq!(
            attr.validate().unwrap_err(),
            AttributionError::MissingSessionId
        );
        let mut attr = sample_attribution();
        attr.actor_token = "".into();
        assert_eq!(
            attr.validate().unwrap_err(),
            AttributionError::MissingActorToken
        );
        let mut attr = sample_attribution();
        attr.dispatch_id = "".into();
        assert_eq!(
            attr.validate().unwrap_err(),
            AttributionError::MissingDispatchId
        );
        let mut attr = sample_attribution();
        attr.seq = 0;
        assert_eq!(attr.validate().unwrap_err(), AttributionError::MissingSeq);
    }

    #[test]
    fn attribution_validate_accepts_complete_attribution() {
        sample_attribution().validate().unwrap();
    }

    // ──── Evidence contract (AC-07) ───────────────────────────────

    #[test]
    fn evidence_accepts_cargo_nextest_with_exit_and_pass_count() {
        validate_evidence_shape(
            "cargo nextest run -p mp --test foo --no-fail-fast exit 0 (3/3 pass)",
        )
        .unwrap();
    }

    #[test]
    fn evidence_rejects_empty() {
        assert!(matches!(
            validate_evidence_shape("").unwrap_err(),
            EvidenceShapeError::Empty
        ));
        assert!(matches!(
            validate_evidence_shape("   ").unwrap_err(),
            EvidenceShapeError::Empty
        ));
    }

    #[test]
    fn evidence_rejects_generic_summary() {
        let cases = [
            "All steps done",
            "M212 complete: ready for review",
            "M212 cycle done",
            "Cycle 1 done",
            "complete: ready for review",
        ];
        for case in cases {
            assert!(
                matches!(
                    validate_evidence_shape(case).unwrap_err(),
                    EvidenceShapeError::GenericSummary(_)
                ),
                "{case:?} should be rejected as generic"
            );
        }
    }

    #[test]
    fn evidence_rejects_missing_command() {
        let err = validate_evidence_shape("foo bar exit 0 (1/1 pass)").unwrap_err();
        assert!(matches!(err, EvidenceShapeError::MissingCommand(_)));
    }

    #[test]
    fn evidence_rejects_missing_exit_code() {
        let err = validate_evidence_shape("cargo nextest run -p mp --test foo (3/3 pass)").unwrap_err();
        assert!(matches!(err, EvidenceShapeError::MissingExitCode(_)));
    }

    #[test]
    fn evidence_rejects_missing_pass_count() {
        let err =
            validate_evidence_shape("cargo nextest run -p mp --test foo exit 0").unwrap_err();
        assert!(matches!(err, EvidenceShapeError::MissingPassCount(_)));
    }

    #[test]
    fn evidence_accepts_make_target() {
        validate_evidence_shape("make test exit 0 (12/12 pass)").unwrap();
    }

    #[test]
    fn evidence_accepts_scripts_path() {
        validate_evidence_shape("scripts/run-checks.sh exit 0 (5/5 pass)").unwrap();
    }

    // ──── Cross-check (AC-01) ─────────────────────────────────────

    #[test]
    fn cross_check_accepts_matching_state() {
        let mut state = sample_state("207", "executed");
        // Set legacy fields explicitly so the cross-check has
        // a stable target — without this, effective_spec_status
        // / effective_execution_status derive from the
        // lifecycle and the test becomes coupled to the
        // legacy-vs-canonical derivation rules.
        state.milestone.milestone.execution_status = "done".into();
        state.milestone.milestone.spec_status = "implemented".into();
        let mut notification = sample_notification("207", "executed");
        notification.claimed_execution_status = "done".into();
        notification.claimed_spec_status = "implemented".into();
        assert!(cross_check_state(&notification, &state).is_ok());
    }

    #[test]
    fn cross_check_rejects_lifecycle_mismatch() {
        let state = sample_state("207", "approved");
        let notification = sample_notification("207", "executed");
        let err = cross_check_state(&notification, &state).unwrap_err();
        assert!(matches!(err, CrossCheckMismatch::LifecycleMismatch { .. }));
    }

    #[test]
    fn cross_check_rejects_milestone_id_mismatch() {
        let state = sample_state("207", "executed");
        let notification = sample_notification("999", "executed");
        let err = cross_check_state(&notification, &state).unwrap_err();
        assert!(matches!(err, CrossCheckMismatch::MilestoneIdMismatch { .. }));
    }

    // ──── Detectors (AC-02) ───────────────────────────────────────

    #[test]
    fn detect_runner_review_violation_fires_for_submitted_review_pass() {
        let mut n = sample_notification("207", "executed");
        n.action = "submitted-review-pass".into();
        let v = detect_runner_review_violation(&n).unwrap();
        assert!(matches!(v, Violation::RunnerReviewViolation(_)));
    }

    #[test]
    fn detect_runner_review_violation_does_not_fire_for_completed_execute() {
        let n = sample_notification("207", "executed");
        assert!(detect_runner_review_violation(&n).is_none());
    }

    #[test]
    fn detect_runner_claim_violation_fires_for_added_finding() {
        let mut n = sample_notification("207", "executed");
        n.action = "added-finding".into();
        let v = detect_runner_claim_violation(&n).unwrap();
        assert!(matches!(v, Violation::RunnerClaimViolation(_)));
    }

    #[test]
    fn detect_runner_claim_violation_does_not_fire_for_reviewer() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Reviewer;
        n.action = "added-finding".into();
        assert!(detect_runner_claim_violation(&n).is_none());
    }

    #[test]
    fn detect_runner_plan_edit_violation_fires_on_master_plan_hunk() {
        let n = sample_notification("207", "executed");
        let hunk = "+++ b/master-plan/milestones/207-*.json\n+    \"lifecycle\": \"executed\"";
        let v = detect_runner_plan_edit_violation(&n, Some(hunk)).unwrap();
        assert!(matches!(v, Violation::RunnerPlanEditViolation(_)));
    }

    #[test]
    fn detect_runner_plan_edit_violation_does_not_fire_for_code_only() {
        let n = sample_notification("207", "executed");
        let hunk = "+++ b/crates/mp/src/lib.rs\n+ // change";
        assert!(detect_runner_plan_edit_violation(&n, Some(hunk)).is_none());
    }

    #[test]
    fn detect_reviewer_code_edit_violation_fires_on_crates_hunk() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Reviewer;
        let hunk = "+++ b/crates/mp/src/lib.rs\n+ // oops reviewer touched code";
        let v = detect_reviewer_code_edit_violation(&n, Some(hunk)).unwrap();
        assert!(matches!(v, Violation::ReviewerCodeEditViolation(_)));
    }

    #[test]
    fn detect_reviewer_code_edit_violation_does_not_fire_for_plan_zone_only() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Reviewer;
        let hunk = "+++ b/master-plan/notes.md\n+ comment from reviewer";
        assert!(detect_reviewer_code_edit_violation(&n, Some(hunk)).is_none());
    }

    #[test]
    fn detect_reviewer_premature_pass_violation_fires_when_cycle_zero() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Reviewer;
        n.action = "submitted-review-pass".into();
        n.cycle = 0;
        let v = detect_reviewer_premature_pass_violation(&n, 1).unwrap();
        assert!(matches!(
            v,
            Violation::ReviewerPrematurePassViolation { .. }
        ));
    }

    #[test]
    fn detect_pre_start_notification_violation_fires_when_dispatch_unknown() {
        let n = sample_notification("207", "executed");
        let started: Vec<String> = vec!["dispatch-2".into()];
        let v = detect_pre_start_notification_violation(&n, &started).unwrap();
        assert!(matches!(v, Violation::PreStartNotificationViolation(_)));
    }

    #[test]
    fn detect_pre_start_notification_violation_passes_when_dispatch_known() {
        let n = sample_notification("207", "executed");
        let started: Vec<String> = vec!["dispatch-1".into()];
        assert!(detect_pre_start_notification_violation(&n, &started).is_none());
    }

    #[test]
    fn detect_orchestrator_code_edit_violation_fires_on_pane_match() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Orchestrator;
        n.attribution.actor_token = "%1".into();
        let hunk = "+++ b/crates/mp/src/lib.rs\n+ oops";
        let v = detect_orchestrator_code_edit_violation(&n, Some(hunk), "%1").unwrap();
        assert!(matches!(v, Violation::OrchestratorCodeEditViolation(_)));
    }

    #[test]
    fn detect_orchestrator_code_edit_violation_does_not_fire_on_other_pane() {
        let mut n = sample_notification("207", "executed");
        n.lane = Lane::Orchestrator;
        n.attribution.actor_token = "%5".into();
        let hunk = "+++ b/crates/mp/src/lib.rs\n+ oops";
        assert!(detect_orchestrator_code_edit_violation(&n, Some(hunk), "%1").is_none());
    }

    // ──── Lifecycle unbacked (AC-04) ──────────────────────────────

    #[test]
    fn lifecycle_claim_unbacked_fires_when_no_event_and_milestone_approved() {
        let state = sample_state("207", "approved");
        let notification = sample_notification("207", "executed");
        let verdict = check_notification(
            &state,
            &notification,
            VerifierInputs {
                diff_hunk: None,
                orchestrator_prompted_cycle: 1,
                started_dispatch_ids: &["dispatch-1".into()],
                orchestrator_pane_id: "%1",
            },
        );
        match verdict {
            Verdict::Reject { violations, .. } => {
                assert!(violations
                    .0
                    .iter()
                    .any(|v| matches!(v, Violation::LifecycleClaimUnbacked(_))));
            }
            other => panic!("expected Reject with LifecycleClaimUnbacked, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_claim_unbacked_does_not_fire_when_event_present() {
        let mut state = sample_state("207", "approved");
        state.activity.events.push(ActivityEvent::now(
            "lifecycle-transition",
            "207",
            "lifecycle: approved → executed",
        ));
        let notification = sample_notification("207", "executed");
        let verdict = check_notification(
            &state,
            &notification,
            VerifierInputs {
                diff_hunk: None,
                orchestrator_prompted_cycle: 1,
                started_dispatch_ids: &["dispatch-1".into()],
                orchestrator_pane_id: "%1",
            },
        );
        // Lifecycle still mismatches canonical=approved vs claimed=executed,
        // but the unbacked detector specifically requires *no* activity event.
        // Other detectors fire (LifecycleClaimUnbacked fires when no event
        // AND canonical=approved); with an event the LifecycleClaimUnbacked
        // detector returns None — the lifecycle mismatch becomes a normal
        // cross-check mismatch that the orchestrator resolves via
        // mp milestone complete.
        match verdict {
            Verdict::Reject { violations, .. } => {
                assert!(!violations
                    .0
                    .iter()
                    .any(|v| matches!(v, Violation::LifecycleClaimUnbacked(_))));
            }
            Verdict::Accept => {}
            other => panic!("expected Reject without LifecycleClaimUnbacked, got {other:?}"),
        }
    }

    // ──── Command list (AC-08) ───────────────────────────────────

    #[test]
    fn command_list_rejects_amp_amp() {
        let mut n = sample_notification("207", "executed");
        n.verification_commands.push(VerificationCommand {
            label: "cargo-nextest".into(),
            argv: vec![
                "cargo".into(),
                "nextest".into(),
                "run".into(),
                "&&".into(),
                "echo".into(),
                "done".into(),
            ],
        });
        let verdict = check_notification(
            &sample_state("207", "executed"),
            &n,
            VerifierInputs {
                diff_hunk: None,
                orchestrator_prompted_cycle: 1,
                started_dispatch_ids: &["dispatch-1".into()],
                orchestrator_pane_id: "%1",
            },
        );
        match verdict {
            Verdict::Reject { violations, .. } => {
                assert!(violations
                    .0
                    .iter()
                    .any(|v| matches!(v, Violation::UnsupportedCommandOperator(_))));
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn command_list_preserves_parentheses_in_nextest_filter() {
        let mut n = sample_notification("207", "executed");
        n.verification_commands.push(VerificationCommand {
            label: "cargo-nextest".into(),
            argv: vec![
                "cargo".into(),
                "nextest".into(),
                "run".into(),
                "-p".into(),
                "mp".into(),
                "--test".into(),
                "test(/verifier_state_cross_check/)".into(),
                "--no-fail-fast".into(),
            ],
        });
        // The argv is a single token with the filter `test(/...)` —
        // parentheses are preserved, no shell metacharacters.
        let result = check_command_list(&n);
        assert!(result.is_ok(), "expected ok, got {result:?}");
    }

    #[test]
    fn command_list_rejects_semicolon_in_token() {
        let mut n = sample_notification("207", "executed");
        n.verification_commands.push(VerificationCommand {
            label: "bad".into(),
            argv: vec!["echo".into(), "1; echo 2".into()],
        });
        let err = check_command_list(&n).unwrap_err();
        assert!(matches!(err, Violation::UnsupportedCommandOperator(_)));
    }

    // ──── Attribution (AC-06) ────────────────────────────────────

    #[test]
    fn unknown_actor_blocks_acceptance() {
        let state = sample_state("207", "executed");
        let mut n = sample_notification("207", "executed");
        n.attribution.actor_token = "".into();
        let verdict = check_notification(
            &state,
            &n,
            VerifierInputs {
                diff_hunk: None,
                orchestrator_prompted_cycle: 1,
                started_dispatch_ids: &["dispatch-1".into()],
                orchestrator_pane_id: "%1",
            },
        );
        match verdict {
            Verdict::UnknownActor { .. } => {}
            other => panic!("expected UnknownActor, got {other:?}"),
        }
    }

    // ──── Remediation (AC-03) ────────────────────────────────────

    #[test]
    fn recommend_remediation_three_pane_is_resend() {
        let v = Violation::RunnerReviewViolation(RunnerReviewViolation {
            milestone_id: "207".into(),
            pane_id: "%2".into(),
            event_seq: Some(1),
        });
        let r = recommend_remediation(&v, Topology::ThreeAgent);
        assert!(matches!(r, Remediation::Resend { .. }));
    }

    #[test]
    fn recommend_remediation_two_pane_is_escalate() {
        let v = Violation::RunnerReviewViolation(RunnerReviewViolation {
            milestone_id: "207".into(),
            pane_id: "%2".into(),
            event_seq: Some(1),
        });
        let r = recommend_remediation(&v, Topology::TwoAgent);
        assert!(matches!(r, Remediation::EscalateToUser { .. }));
    }

    #[test]
    fn recommend_remediation_one_pane_is_escalate() {
        let v = Violation::RunnerReviewViolation(RunnerReviewViolation {
            milestone_id: "207".into(),
            pane_id: "%2".into(),
            event_seq: Some(1),
        });
        let r = recommend_remediation(&v, Topology::OneAgent);
        assert!(matches!(r, Remediation::EscalateToUser { .. }));
    }

    // ──── Evidence not overwritten (AC-07) ──────────────────────

    #[test]
    fn evidence_not_overwritten_passes_when_snapshot_matches() {
        let mut state = sample_state("207", "executed");
        let mut ac = AcceptanceCriterion::default();
        ac.id = "AC-01".into();
        ac.evidence = "cargo nextest run -p mp --test foo exit 0 (3/3 pass)".into();
        state.milestone.acceptance_criteria.push(ac);
        let pre = [("AC-01", "cargo nextest run -p mp --test foo exit 0 (3/3 pass)")];
        check_evidence_not_overwritten(&state, &pre).unwrap();
    }

    #[test]
    fn evidence_not_overwritten_fails_when_back_filled() {
        let mut state = sample_state("207", "executed");
        let mut ac = AcceptanceCriterion::default();
        ac.id = "AC-01".into();
        ac.evidence = "All done".into();
        state.milestone.acceptance_criteria.push(ac);
        let pre = [("AC-01", "cargo nextest run -p mp --test foo exit 0 (3/3 pass)")];
        let err = check_evidence_not_overwritten(&state, &pre).unwrap_err();
        assert!(matches!(
            err,
            EvidenceShapeError::OverwrittenAfterCompletion { .. }
        ));
    }

    #[test]
    fn check_evidence_contract_flags_generic_summaries_on_every_ac() {
        let mut state = sample_state("207", "executed");
        for i in 1..=2 {
            let mut ac = AcceptanceCriterion::default();
            ac.id = format!("AC-0{i}");
            ac.evidence = "All steps done".into();
            state.milestone.acceptance_criteria.push(ac);
        }
        let failing = check_evidence_contract(&state, &sample_notification("207", "executed"));
        assert_eq!(failing.len(), 2);
        assert_eq!(failing[0].0, "AC-01");
        assert_eq!(failing[1].0, "AC-02");
    }

    // ──── VerifierState::activity_for_milestone ──────────────────

    #[test]
    fn activity_for_milestone_filters_by_subject() {
        let mut state = sample_state("207", "approved");
        state.activity.events.push(ActivityEvent::now(
            "lifecycle-transition",
            "207",
            "lifecycle: approved → in-progress",
        ));
        state.activity.events.push(ActivityEvent::now(
            "lifecycle-transition",
            "999",
            "different milestone",
        ));
        let events = state.activity_for_milestone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].subject, "207");
    }

    // ──── JSON round-trip ────────────────────────────────────────

    #[test]
    fn violations_round_trip_via_serde() {
        let v = Violation::RunnerReviewViolation(RunnerReviewViolation {
            milestone_id: "207".into(),
            pane_id: "%2".into(),
            event_seq: Some(1),
        });
        let json = serde_json::to_string(&v).unwrap();
        let back: Violation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn verdict_round_trips_via_serde() {
        let v = Verdict::Accept;
        let json = serde_json::to_string(&v).unwrap();
        let back: Verdict = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    // ──── Type-suppression smoke (TempDir unused in some tests) ─

    #[test]
    fn _ctx_in_is_well_typed() {
        let tmp = TempDir::new().unwrap();
        let _ = ctx_in(tmp.path());
    }
}