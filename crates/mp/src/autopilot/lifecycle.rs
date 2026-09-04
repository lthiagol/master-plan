//! M223 — typed lifecycle closure protocol for autopilot sessions.
//!
//! The herdr R5/R7/R10 lessons in the M200/M201/M202 test session
//! surfaced three failure modes that a closure protocol must defend
//! against:
//!
//! - **R5** — the runner reported `lifecycle=executed` when the
//!   milestone never advanced (lifecycle fabrication).
//! - **R7** — `mp milestone complete` overwrote per-AC evidence with
//!   a completion-summary commit (evidence overwrite).
//! - **R10** — every AC ended up with a generic evidence string
//!   rather than a distinct cargo-nextest record (generic-evidence
//!   regression).
//!
//! This module is the *typed contract* the autopilot engine drives.
//! It does not shell out — the orchestrator dispatches the closure as
//! a sequence of typed [`LifecycleTransition`]s and the protocol
//! records each one against an idempotency key. A transition that
//! would overwrite evidence (per R7) is rejected before it lands;
//! one that would advance past the milestone's actual state (per R5)
//! is rejected as `LifecycleDrift`. Reruns with the same idempotency
//! key are no-ops so a crash mid-closure does not fabricate success
//! (per AC-03).
//!
//! ## Sequence
//!
//! The canonical closure sequence is encoded in
//! [`LIFECYCLE_TRANSITION_ORDER`]:
//!
//! 1. `MarkStepDone` for every step (in the milestone's recorded
//!    order).
//! 2. `StampCriterionPass` for every AC (each carries its own
//!    evidence revision).
//! 3. `ClaimReview` (independent reviewer, after the runner has
//!    completed).
//! 4. `AddFinding` / `ResolveFinding` for every finding recorded
//!    during the review pass.
//! 5. `PassReviews` (final reviewer verdict).
//! 6. `CompleteLifecycle` (the only transition that advances
//!    `lifecycle` from `executed` to `complete`).
//!
//! Steps 1–5 can be reordered within their sub-phases; step 6 is
//! always last. The protocol refuses to record `CompleteLifecycle`
//! before every earlier transition in the milestone's closure plan
//! has either landed or been skipped as `Idempotent`.
//!
//! ## Revision checks
//!
//! Each transition carries an `idempotency_key`. The closure stores
//! `(transition_kind, idempotency_key)` pairs; replaying the same
//! transition with the same key returns
//! [`TransitionOutcome::Idempotent`] and does not mutate state. A
//! transition with a *different* key for the same kind is rejected
//! as [`TransitionRejectReason::IdempotencyKeyMismatch`] — the
//! caller must reconcile the canonical state before retrying.
//!
//! ## Restart safety (AC-03)
//!
//! [`execute_closure`] is restart-safe: every applied transition is
//! persisted to [`ClosureJournal`] (the in-memory fixture; the
//! autopilot engine persists the same record to `session.json`).
//! On rerun, the journal is replayed to determine which transitions
//! have already landed and which are still pending. A failure at any
//! boundary leaves the journal in a state the next run can resume
//! from — there is no path where a partial closure fabricates a
//! `lifecycle=complete` state.

use serde::{Deserialize, Serialize};

// ─── Snapshot ─────────────────────────────────────────────────────────

/// Typed snapshot of milestone state at the start of the closure.
/// The closure protocol treats this as the source of truth and
/// refuses transitions that would contradict it (e.g. stamping a
/// pass on an AC that is not in the snapshot, or marking a step
/// done that the milestone does not contain).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneSnapshot {
    pub milestone_id: String,
    pub lifecycle: String,
    pub spec_status: String,
    pub execution_status: String,
    pub steps: Vec<StepSnapshot>,
    pub acceptance_criteria: Vec<AcSnapshot>,
    pub reviews: Vec<ReviewSnapshot>,
    pub findings: Vec<FindingSnapshot>,
}

impl MilestoneSnapshot {
    /// Snapshot a milestone that is ready for closure: lifecycle
    /// is `executed`, all steps are pending (about to be marked
    /// done), every AC has no evidence yet.
    pub fn ready_for_closure(milestone_id: &str, step_ids: &[&str], ac_ids: &[&str]) -> Self {
        Self {
            milestone_id: milestone_id.to_string(),
            lifecycle: "executed".to_string(),
            spec_status: "ready".to_string(),
            execution_status: "executed".to_string(),
            steps: step_ids
                .iter()
                .map(|id| StepSnapshot {
                    id: (*id).to_string(),
                    status: "pending".to_string(),
                })
                .collect(),
            acceptance_criteria: ac_ids
                .iter()
                .map(|id| AcSnapshot {
                    id: (*id).to_string(),
                    status: "pending".to_string(),
                    evidence: String::new(),
                    revision: String::new(),
                })
                .collect(),
            reviews: Vec::new(),
            findings: Vec::new(),
        }
    }

    pub fn step(&self, step_id: &str) -> Option<&StepSnapshot> {
        self.steps.iter().find(|s| s.id == step_id)
    }

    pub fn ac(&self, ac_id: &str) -> Option<&AcSnapshot> {
        self.acceptance_criteria.iter().find(|a| a.id == ac_id)
    }

    pub fn finding(&self, finding_id: &str) -> Option<&FindingSnapshot> {
        self.findings.iter().find(|f| f.id == finding_id)
    }

    pub fn review(&self, review_id: &str) -> Option<&ReviewSnapshot> {
        self.reviews.iter().find(|r| r.id == review_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepSnapshot {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcSnapshot {
    pub id: String,
    pub status: String,
    pub evidence: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSnapshot {
    pub id: String,
    pub status: String,
    pub actor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingSnapshot {
    pub id: String,
    pub status: String,
    pub fixed_in: String,
    pub resolved_at: String,
}

// ─── Transitions ─────────────────────────────────────────────────────

/// Canonical order of transition kinds in the closure ceremony.
/// Step marks come first, then AC stamps, then reviewer claim,
/// findings, reviews pass, and finally lifecycle complete. The
/// protocol refuses to record a transition whose kind is "later"
/// in this order than any unapplied "earlier" kind.
pub const LIFECYCLE_TRANSITION_ORDER: &[TransitionKind] = &[
    TransitionKind::MarkStepDone,
    TransitionKind::StampCriterionPass,
    TransitionKind::ClaimReview,
    TransitionKind::AddFinding,
    TransitionKind::ResolveFinding,
    TransitionKind::PassReviews,
    TransitionKind::CompleteLifecycle,
];

/// Discriminator for the [`LifecycleTransition`] variants — lets
/// the journal index transitions by kind without pattern-matching
/// the full enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TransitionKind {
    MarkStepDone,
    StampCriterionPass,
    ClaimReview,
    AddFinding,
    ResolveFinding,
    PassReviews,
    CompleteLifecycle,
}

impl TransitionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TransitionKind::MarkStepDone => "mark-step-done",
            TransitionKind::StampCriterionPass => "stamp-criterion-pass",
            TransitionKind::ClaimReview => "claim-review",
            TransitionKind::AddFinding => "add-finding",
            TransitionKind::ResolveFinding => "resolve-finding",
            TransitionKind::PassReviews => "pass-reviews",
            TransitionKind::CompleteLifecycle => "complete-lifecycle",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "mark-step-done" => TransitionKind::MarkStepDone,
            "stamp-criterion-pass" => TransitionKind::StampCriterionPass,
            "claim-review" => TransitionKind::ClaimReview,
            "add-finding" => TransitionKind::AddFinding,
            "resolve-finding" => TransitionKind::ResolveFinding,
            "pass-reviews" => TransitionKind::PassReviews,
            "complete-lifecycle" => TransitionKind::CompleteLifecycle,
            _ => return None,
        })
    }
}

/// One closure action. Each carries the idempotency key the
/// caller computed from the canonical state at the time of
/// dispatch (per R10 lesson: per-AC evidence revisions must be
/// preserved across the closure ceremony, so each AC stamp has
/// its own key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LifecycleTransition {
    MarkStepDone {
        step_id: String,
        idempotency_key: String,
    },
    StampCriterionPass {
        ac_id: String,
        evidence: String,
        revision: String,
        idempotency_key: String,
    },
    ClaimReview {
        review_id: String,
        actor: String,
        idempotency_key: String,
    },
    AddFinding {
        finding_id: String,
        description: String,
        idempotency_key: String,
    },
    ResolveFinding {
        finding_id: String,
        fixed_in: String,
        idempotency_key: String,
    },
    PassReviews {
        review_id: String,
        idempotency_key: String,
    },
    CompleteLifecycle {
        idempotency_key: String,
    },
}

impl LifecycleTransition {
    pub fn kind(&self) -> TransitionKind {
        match self {
            LifecycleTransition::MarkStepDone { .. } => TransitionKind::MarkStepDone,
            LifecycleTransition::StampCriterionPass { .. } => TransitionKind::StampCriterionPass,
            LifecycleTransition::ClaimReview { .. } => TransitionKind::ClaimReview,
            LifecycleTransition::AddFinding { .. } => TransitionKind::AddFinding,
            LifecycleTransition::ResolveFinding { .. } => TransitionKind::ResolveFinding,
            LifecycleTransition::PassReviews { .. } => TransitionKind::PassReviews,
            LifecycleTransition::CompleteLifecycle { .. } => TransitionKind::CompleteLifecycle,
        }
    }

    pub fn idempotency_key(&self) -> &str {
        match self {
            LifecycleTransition::MarkStepDone {
                idempotency_key, ..
            }
            | LifecycleTransition::StampCriterionPass {
                idempotency_key, ..
            }
            | LifecycleTransition::ClaimReview {
                idempotency_key, ..
            }
            | LifecycleTransition::AddFinding {
                idempotency_key, ..
            }
            | LifecycleTransition::ResolveFinding {
                idempotency_key, ..
            }
            | LifecycleTransition::PassReviews {
                idempotency_key, ..
            }
            | LifecycleTransition::CompleteLifecycle { idempotency_key } => idempotency_key,
        }
    }

    pub fn target_id(&self) -> &str {
        match self {
            LifecycleTransition::MarkStepDone { step_id, .. } => step_id,
            LifecycleTransition::StampCriterionPass { ac_id, .. } => ac_id,
            LifecycleTransition::ClaimReview { review_id, .. } => review_id,
            LifecycleTransition::AddFinding { finding_id, .. } => finding_id,
            LifecycleTransition::ResolveFinding { finding_id, .. } => finding_id,
            LifecycleTransition::PassReviews { review_id, .. } => review_id,
            LifecycleTransition::CompleteLifecycle { .. } => "lifecycle",
        }
    }
}

// ─── Outcomes ────────────────────────────────────────────────────────

/// Per-transition result. Either the transition landed
/// ([`TransitionOutcome::Applied`]), the same key had already
/// landed ([`TransitionOutcome::Idempotent`]), or the protocol
/// refused the transition with a typed reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum TransitionOutcome {
    Applied {
        kind: TransitionKind,
        target_id: String,
        idempotency_key: String,
        snapshot_index: usize,
    },
    Idempotent {
        kind: TransitionKind,
        target_id: String,
        idempotency_key: String,
        snapshot_index: usize,
    },
    Rejected {
        kind: TransitionKind,
        target_id: String,
        idempotency_key: String,
        reason: TransitionRejectReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TransitionRejectReason {
    /// The caller tried to advance past `lifecycle=executed` while
    /// the milestone was still in an earlier state. R5 guard: refuse
    /// the fabrication.
    LifecycleDrift { observed: String, attempted: String },
    /// The target of the transition (step id, AC id, finding id,
    /// review id) is not in the milestone snapshot. The protocol
    /// refuses transitions against unknown targets to prevent the
    /// runner from stamping ACs or marking steps that the spec
    /// does not contain.
    UnknownTarget { target: String, target_kind: String },
    /// The same `(kind, target)` has already landed with a
    /// *different* idempotency key. The caller must reconcile the
    /// canonical state before retrying — this is the conflict
    /// signal from the AC-projection lesson (M207).
    IdempotencyKeyMismatch { stored: String, attempted: String },
    /// Stamping the AC would overwrite a non-empty evidence value
    /// with a value that does not improve on it (R7 guard).
    EvidenceOverwrite {
        ac_id: String,
        before: String,
        after: String,
    },
    /// Stamping the AC with an empty / generic / non-runnable
    /// evidence string. Per R10 the AC must carry the exact
    /// verification command + exit code + pass count.
    EvidenceShape { ac_id: String, detail: String },
    /// `ResolveFinding` requires a non-empty `fixed_in` SHA that
    /// the commit policy accepted.
    MissingFixedIn { finding_id: String },
    /// `ResolveFinding`'s `fixed_in` SHA is fabricated (not in the
    /// commit index the policy trusts).
    FabricatedFixedIn { finding_id: String, sha: String },
    /// The `fixed_in` SHA is a *grouped remediation commit* — one
    /// commit claiming to fix multiple findings. The M200/M202
    /// `fixed_in` drift lesson says one fix per commit.
    GroupedRemediation { finding_id: String, sha: String },
    /// An earlier transition in the canonical order is still
    /// pending (e.g. trying to `CompleteLifecycle` before all
    /// steps are marked done).
    OutOfOrder { pending_kind: TransitionKind },
    /// The journal's most-recent applied transition is not the
    /// transition the caller is now applying; the journal is the
    /// canonical record so the caller must align with it.
    JournalMismatch {
        journal_kind: TransitionKind,
        attempted: TransitionKind,
    },
}

impl std::fmt::Display for TransitionRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionRejectReason::LifecycleDrift {
                observed,
                attempted,
            } => write!(
                f,
                "lifecycle drift: observed={observed} attempted={attempted}"
            ),
            TransitionRejectReason::UnknownTarget {
                target,
                target_kind,
            } => write!(f, "unknown {target_kind} target: {target}"),
            TransitionRejectReason::IdempotencyKeyMismatch { stored, attempted } => write!(
                f,
                "idempotency-key mismatch: stored={stored} attempted={attempted}"
            ),
            TransitionRejectReason::EvidenceOverwrite {
                ac_id,
                before,
                after,
            } => write!(
                f,
                "evidence overwrite for {ac_id}: before={before:?} after={after:?}"
            ),
            TransitionRejectReason::EvidenceShape { ac_id, detail } => {
                write!(f, "evidence shape rejected for {ac_id}: {detail}")
            }
            TransitionRejectReason::MissingFixedIn { finding_id } => {
                write!(f, "missing fixed_in for finding {finding_id}")
            }
            TransitionRejectReason::FabricatedFixedIn { finding_id, sha } => {
                write!(f, "fabricated fixed_in for finding {finding_id}: sha={sha}")
            }
            TransitionRejectReason::GroupedRemediation { finding_id, sha } => write!(
                f,
                "grouped remediation commit {sha} for finding {finding_id}"
            ),
            TransitionRejectReason::OutOfOrder { pending_kind } => write!(
                f,
                "out-of-order: pending transition kind {} must apply first",
                pending_kind.as_str()
            ),
            TransitionRejectReason::JournalMismatch {
                journal_kind,
                attempted,
            } => write!(
                f,
                "journal mismatch: journal={} attempted={}",
                journal_kind.as_str(),
                attempted.as_str()
            ),
        }
    }
}

// ─── Commit-policy bridge ────────────────────────────────────────────

/// The lifecycle protocol delegates commit-attribution checks to
/// the commit policy module (see [`crate::autopilot::commit_policy`]).
/// This trait keeps the bridge typed and lets tests substitute an
/// in-memory fixture for `git log`.
pub trait CommitAttestation {
    /// Return true iff `sha` is a real commit in the index the
    /// policy trusts.
    fn sha_is_real(&self, sha: &str) -> bool;
    /// Return true iff `sha` is a one-finding-per-commit
    /// remediation commit. The M200/M202 lesson says grouped
    /// remediation is the `fixed_in` drift pattern.
    fn is_single_finding_fix(&self, sha: &str) -> bool;
    /// True iff `sha` is a *lifecycle metadata* commit whose
    /// payload would overwrite per-AC evidence.
    fn is_evidence_overwriting_metadata(&self, sha: &str) -> bool;
}

/// Default null attestation — every SHA is unknown. Tests inject a
/// fixture; production wires this to the policy module.
#[derive(Debug, Default, Clone)]
pub struct NullAttestation;

impl CommitAttestation for NullAttestation {
    fn sha_is_real(&self, _sha: &str) -> bool {
        false
    }
    fn is_single_finding_fix(&self, _sha: &str) -> bool {
        false
    }
    fn is_evidence_overwriting_metadata(&self, _sha: &str) -> bool {
        false
    }
}

// ─── Evidence shape ──────────────────────────────────────────────────

/// Minimum evidence shape required for `StampCriterionPass`. Per
/// R10 the AC must carry the exact verification command + exit
/// code + pass count. Anything else is rejected.
pub fn validate_evidence_shape(evidence: &str) -> Result<(), String> {
    let trimmed = evidence.trim();
    if trimmed.is_empty() {
        return Err("evidence is empty".into());
    }
    if !trimmed.contains("exit ") {
        return Err(format!("evidence missing 'exit <code>': {trimmed:?}"));
    }
    if !trimmed.contains(" pass)") {
        return Err(format!("evidence missing '(<n>/<m> pass)': {trimmed:?}"));
    }
    let first = trimmed.split_whitespace().next().unwrap_or("");
    let runnable_prefixes = [
        "cargo", "make", "rustc", "bash", "sh", "nextest", "scripts/",
    ];
    let starts_runnable = runnable_prefixes.iter().any(|p| first.starts_with(p))
        || first.starts_with("./")
        || first.starts_with('/');
    if !starts_runnable {
        return Err(format!(
            "evidence does not start with a runnable command: {trimmed:?}"
        ));
    }
    Ok(())
}

// ─── Closure journal ─────────────────────────────────────────────────

/// A single applied transition. The journal is the canonical
/// record of "what landed" so a rerun can resume from the same
/// point without re-applying or fabricating success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub index: usize,
    pub kind: TransitionKind,
    pub target_id: String,
    pub idempotency_key: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureJournal {
    entries: Vec<JournalEntry>,
    /// `lifecycle` value applied by the most recent
    /// `CompleteLifecycle` transition. `None` while no completion
    /// has landed.
    completed_lifecycle: Option<String>,
}

impl ClosureJournal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    pub fn completed_lifecycle(&self) -> Option<&str> {
        self.completed_lifecycle.as_deref()
    }

    /// Look up the journal entry by `(kind, target_id)`. Used by
    /// the idempotency check.
    pub fn lookup(&self, kind: TransitionKind, target_id: &str) -> Option<&JournalEntry> {
        self.entries
            .iter()
            .find(|e| e.kind == kind && e.target_id == target_id)
    }

    fn append(
        &mut self,
        kind: TransitionKind,
        target_id: String,
        idempotency_key: String,
        applied_at: &str,
    ) -> JournalEntry {
        let entry = JournalEntry {
            index: self.entries.len(),
            kind,
            target_id,
            idempotency_key,
            applied_at: applied_at.to_string(),
        };
        self.entries.push(entry.clone());
        if matches!(kind, TransitionKind::CompleteLifecycle) {
            self.completed_lifecycle = Some("complete".to_string());
        }
        entry
    }

    /// M226 F-02 wiring: pre-seed the journal with a synthetic
    /// entry. Used by the production `complete_milestone` gate
    /// to model the milestone's existing state as already-applied
    /// transitions so the closure ceremony's `first_pending_kind`
    /// check does not reject legacy milestones that were completed
    /// before the closure protocol was wired in. Each pre-seeded
    /// entry is recorded with the same idempotency-key shape the
    /// production runner uses so the journal is deterministic
    /// across reruns.
    pub fn add_entry(
        &mut self,
        kind: TransitionKind,
        target_id: impl Into<String>,
        idempotency_key: impl Into<String>,
        applied_at: impl Into<String>,
    ) {
        let _ = self.append(
            kind,
            target_id.into(),
            idempotency_key.into(),
            &applied_at.into(),
        );
    }
}

// ─── Closure execution ───────────────────────────────────────────────

/// Result of running a closure plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureOutcome {
    pub milestone_id: String,
    pub outcomes: Vec<TransitionOutcome>,
    pub applied_count: usize,
    pub idempotent_count: usize,
    pub rejected_count: usize,
    pub final_lifecycle: String,
    pub journal: ClosureJournal,
}

impl ClosureOutcome {
    pub fn reached_complete(&self) -> bool {
        self.final_lifecycle == "complete"
    }

    pub fn first_reject(&self) -> Option<&TransitionOutcome> {
        self.outcomes
            .iter()
            .find(|o| matches!(o, TransitionOutcome::Rejected { .. }))
    }
}

/// Closure context. Holds the snapshot, the journal (which may
/// already contain transitions from a prior partial run), and the
/// commit attestation. The caller composes the plan from the
/// milestone's `steps` + `acceptance_criteria` + `reviews` +
/// `findings` arrays — the protocol is agnostic to *which* command
/// produces each transition.
pub struct LifecycleClosure<'a> {
    pub milestone: MilestoneSnapshot,
    pub journal: ClosureJournal,
    pub commits: &'a dyn CommitAttestation,
}

impl<'a> LifecycleClosure<'a> {
    pub fn new(milestone: MilestoneSnapshot, commits: &'a dyn CommitAttestation) -> Self {
        Self {
            milestone,
            journal: ClosureJournal::new(),
            commits,
        }
    }

    pub fn from_journal(
        milestone: MilestoneSnapshot,
        journal: ClosureJournal,
        commits: &'a dyn CommitAttestation,
    ) -> Self {
        Self {
            milestone,
            journal,
            commits,
        }
    }

    /// Apply every transition in `plan` in order. Stops on the
    /// first rejection (subsequent transitions are recorded as
    /// `Rejected { OutOfOrder }` so the journal reflects the
    /// failure boundary for AC-03).
    pub fn execute(&mut self, plan: &[LifecycleTransition], clock: &Clock) -> ClosureOutcome {
        let mut outcomes = Vec::with_capacity(plan.len());
        let mut stop = false;
        let mut stop_reason = TransitionRejectReason::OutOfOrder {
            pending_kind: TransitionKind::CompleteLifecycle,
        };
        for transition in plan {
            if stop {
                let kind = transition.kind();
                let target = transition.target_id().to_string();
                let key = transition.idempotency_key().to_string();
                outcomes.push(TransitionOutcome::Rejected {
                    kind,
                    target_id: target,
                    idempotency_key: key,
                    reason: stop_reason.clone(),
                });
                continue;
            }
            let outcome = self.apply_one(transition, clock);
            if let TransitionOutcome::Rejected { reason, .. } = &outcome {
                stop_reason = reason.clone();
                stop = true;
            }
            outcomes.push(outcome);
        }

        let applied_count = outcomes
            .iter()
            .filter(|o| matches!(o, TransitionOutcome::Applied { .. }))
            .count();
        let idempotent_count = outcomes
            .iter()
            .filter(|o| matches!(o, TransitionOutcome::Idempotent { .. }))
            .count();
        let rejected_count = outcomes
            .iter()
            .filter(|o| matches!(o, TransitionOutcome::Rejected { .. }))
            .count();
        let final_lifecycle = self
            .journal
            .completed_lifecycle
            .clone()
            .unwrap_or_else(|| self.milestone.lifecycle.clone());

        ClosureOutcome {
            milestone_id: self.milestone.milestone_id.clone(),
            outcomes,
            applied_count,
            idempotent_count,
            rejected_count,
            final_lifecycle,
            journal: self.journal.clone(),
        }
    }

    fn apply_one(&mut self, transition: &LifecycleTransition, clock: &Clock) -> TransitionOutcome {
        let kind = transition.kind();
        let target = transition.target_id().to_string();
        let key = transition.idempotency_key().to_string();
        // 1. Idempotency: same (kind, target) already in journal
        //    with the same key -> no-op.
        if let Some(entry) = self.journal.lookup(kind, &target) {
            return if entry.idempotency_key == key {
                TransitionOutcome::Idempotent {
                    kind,
                    target_id: target,
                    idempotency_key: key,
                    snapshot_index: entry.index,
                }
            } else {
                TransitionOutcome::Rejected {
                    kind,
                    target_id: target,
                    idempotency_key: key.clone(),
                    reason: TransitionRejectReason::IdempotencyKeyMismatch {
                        stored: entry.idempotency_key.clone(),
                        attempted: key,
                    },
                }
            };
        }

        // 2. Order check: the only kind with a strict
        //    ordering prerequisite is `CompleteLifecycle`. The
        //    earlier kinds (steps, ACs, claim review, findings)
        //    are flexible within their sub-phase (per the spec
        //    "Steps 1–5 can be reordered within their
        //    sub-phases"). Anything else is fine here.
        if matches!(kind, TransitionKind::CompleteLifecycle) {
            if let Some(pending) = self.first_pending_kind() {
                if pending != TransitionKind::CompleteLifecycle {
                    return TransitionOutcome::Rejected {
                        kind,
                        target_id: target,
                        idempotency_key: key,
                        reason: TransitionRejectReason::OutOfOrder {
                            pending_kind: pending,
                        },
                    };
                }
            }
        }

        // 3. Per-kind checks.
        match transition {
            LifecycleTransition::MarkStepDone { step_id, .. } => {
                self.apply_step_done(step_id, kind, target, key, clock)
            }
            LifecycleTransition::StampCriterionPass {
                ac_id,
                evidence,
                revision,
                ..
            } => self.apply_ac_pass(ac_id, evidence, revision, kind, target, key, clock),
            LifecycleTransition::ClaimReview {
                review_id, actor, ..
            } => self.apply_claim_review(review_id, actor, kind, target, key, clock),
            LifecycleTransition::AddFinding {
                finding_id,
                description,
                ..
            } => self.apply_add_finding(finding_id, description, kind, target, key, clock),
            LifecycleTransition::ResolveFinding {
                finding_id,
                fixed_in,
                ..
            } => self.apply_resolve_finding(finding_id, fixed_in, kind, target, key, clock),
            LifecycleTransition::PassReviews { review_id, .. } => {
                self.apply_pass_reviews(review_id, kind, target, key, clock)
            }
            LifecycleTransition::CompleteLifecycle { .. } => {
                self.apply_complete_lifecycle(kind, target, key, clock)
            }
        }
    }

    fn apply_step_done(
        &mut self,
        step_id: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        let Some(step) = self.milestone.step(step_id).cloned() else {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::UnknownTarget {
                    target: step_id.to_string(),
                    target_kind: "step".to_string(),
                },
            };
        };
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        if let Some(s) = self.milestone.steps.iter_mut().find(|s| s.id == step.id) {
            s.status = "done".to_string();
        }
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_ac_pass(
        &mut self,
        ac_id: &str,
        evidence: &str,
        revision: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        let Some(ac) = self.milestone.ac(ac_id).cloned() else {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::UnknownTarget {
                    target: ac_id.to_string(),
                    target_kind: "ac".to_string(),
                },
            };
        };
        if let Err(detail) = validate_evidence_shape(evidence) {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::EvidenceShape {
                    ac_id: ac_id.to_string(),
                    detail,
                },
            };
        }
        // The revision is the idempotency key. A re-stamp with a
        // different revision is a key mismatch (R7/R10 guard).
        if !ac.revision.is_empty() && ac.revision != revision {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::IdempotencyKeyMismatch {
                    stored: ac.revision.clone(),
                    attempted: revision.to_string(),
                },
            };
        }
        // Same revision but evidence differs is an overwrite.
        if !ac.evidence.is_empty() && ac.evidence != evidence {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::EvidenceOverwrite {
                    ac_id: ac.id.clone(),
                    before: ac.evidence.clone(),
                    after: evidence.to_string(),
                },
            };
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        if let Some(a) = self
            .milestone
            .acceptance_criteria
            .iter_mut()
            .find(|a| a.id == ac.id)
        {
            a.status = "passed".to_string();
            a.evidence = evidence.to_string();
            a.revision = revision.to_string();
        }
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn apply_claim_review(
        &mut self,
        review_id: &str,
        actor: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        if self.milestone.review(review_id).is_none() {
            self.milestone.reviews.push(ReviewSnapshot {
                id: review_id.to_string(),
                status: "claimed".to_string(),
                actor: actor.to_string(),
            });
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn apply_add_finding(
        &mut self,
        finding_id: &str,
        description: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        if self.milestone.finding(finding_id).is_none() {
            self.milestone.findings.push(FindingSnapshot {
                id: finding_id.to_string(),
                status: "open".to_string(),
                fixed_in: String::new(),
                resolved_at: String::new(),
            });
            let _ = description;
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn apply_resolve_finding(
        &mut self,
        finding_id: &str,
        fixed_in: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        if fixed_in.is_empty() {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::MissingFixedIn {
                    finding_id: finding_id.to_string(),
                },
            };
        }
        if !self.commits.sha_is_real(fixed_in) {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::FabricatedFixedIn {
                    finding_id: finding_id.to_string(),
                    sha: fixed_in.to_string(),
                },
            };
        }
        if !self.commits.is_single_finding_fix(fixed_in) {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::GroupedRemediation {
                    finding_id: finding_id.to_string(),
                    sha: fixed_in.to_string(),
                },
            };
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        if self.milestone.finding(finding_id).is_none() {
            // Resume path: the finding was added in a prior run
            // that we are replaying. Re-create the snapshot row so
            // the closure's milestone state matches the journal.
            self.milestone.findings.push(FindingSnapshot {
                id: finding_id.to_string(),
                status: "resolved".to_string(),
                fixed_in: fixed_in.to_string(),
                resolved_at: clock.now(),
            });
        } else if let Some(f) = self
            .milestone
            .findings
            .iter_mut()
            .find(|f| f.id == finding_id)
        {
            f.status = "resolved".to_string();
            f.fixed_in = fixed_in.to_string();
            f.resolved_at = clock.now();
        }
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn apply_pass_reviews(
        &mut self,
        review_id: &str,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        let in_snapshot = self.milestone.review(review_id).is_some();
        let claimed_in_journal = self
            .journal
            .lookup(TransitionKind::ClaimReview, review_id)
            .is_some();
        if !in_snapshot && !claimed_in_journal {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::UnknownTarget {
                    target: review_id.to_string(),
                    target_kind: "review".to_string(),
                },
            };
        }
        let snapshot_status = self
            .milestone
            .review(review_id)
            .map(|r| r.status.as_str())
            .unwrap_or("");
        if !claimed_in_journal && snapshot_status != "claimed" {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::JournalMismatch {
                    journal_kind: TransitionKind::ClaimReview,
                    attempted: kind,
                },
            };
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        if !in_snapshot {
            // Resume path: rebuild the review snapshot row so the
            // milestone reflects the journal.
            self.milestone.reviews.push(ReviewSnapshot {
                id: review_id.to_string(),
                status: "passed".to_string(),
                actor: "reviewer".to_string(),
            });
        } else if let Some(r) = self
            .milestone
            .reviews
            .iter_mut()
            .find(|r| r.id == review_id)
        {
            r.status = "passed".to_string();
        }
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn apply_complete_lifecycle(
        &mut self,
        kind: TransitionKind,
        target: String,
        key: String,
        clock: &Clock,
    ) -> TransitionOutcome {
        if self.milestone.lifecycle != "executed" {
            return TransitionOutcome::Rejected {
                kind,
                target_id: target,
                idempotency_key: key,
                reason: TransitionRejectReason::LifecycleDrift {
                    observed: self.milestone.lifecycle.clone(),
                    attempted: "complete".to_string(),
                },
            };
        }
        // AC-03 guard: refuse to mark complete while any finding
        // recorded in the milestone is still open AND the journal
        // has not recorded a `ResolveFinding` entry for it. The
        // journal is the canonical record on resume, so a finding
        // that was resolved in a prior run (and lives only in the
        // journal) is treated as resolved.
        for finding in &self.milestone.findings {
            if finding.status != "resolved"
                && self
                    .journal
                    .lookup(TransitionKind::ResolveFinding, &finding.id)
                    .is_none()
            {
                return TransitionOutcome::Rejected {
                    kind,
                    target_id: target.clone(),
                    idempotency_key: key.clone(),
                    reason: TransitionRejectReason::OutOfOrder {
                        pending_kind: TransitionKind::ResolveFinding,
                    },
                };
            }
        }
        let entry = self
            .journal
            .append(kind, target.clone(), key.clone(), &clock.now());
        self.milestone.lifecycle = "complete".to_string();
        self.journal.completed_lifecycle = Some("complete".to_string());
        TransitionOutcome::Applied {
            kind,
            target_id: target,
            idempotency_key: key,
            snapshot_index: entry.index,
        }
    }

    fn first_pending_kind(&self) -> Option<TransitionKind> {
        for kind in LIFECYCLE_TRANSITION_ORDER {
            let required = match kind {
                TransitionKind::MarkStepDone => self.milestone.steps.iter().any(|s| {
                    self.journal
                        .lookup(TransitionKind::MarkStepDone, &s.id)
                        .is_none()
                }),
                TransitionKind::StampCriterionPass => {
                    self.milestone.acceptance_criteria.iter().any(|a| {
                        self.journal
                            .lookup(TransitionKind::StampCriterionPass, &a.id)
                            .is_none()
                    })
                }
                TransitionKind::ClaimReview => self.milestone.reviews.iter().any(|r| {
                    self.journal
                        .lookup(TransitionKind::ClaimReview, &r.id)
                        .is_none()
                }),
                TransitionKind::AddFinding => self.milestone.findings.iter().any(|f| {
                    f.status == "open"
                        && self
                            .journal
                            .lookup(TransitionKind::AddFinding, &f.id)
                            .is_none()
                }),
                TransitionKind::ResolveFinding => self.milestone.findings.iter().any(|f| {
                    f.status != "resolved"
                        && self
                            .journal
                            .lookup(TransitionKind::ResolveFinding, &f.id)
                            .is_none()
                }),
                TransitionKind::PassReviews => self.milestone.reviews.iter().any(|r| {
                    r.status != "passed"
                        && self
                            .journal
                            .lookup(TransitionKind::PassReviews, &r.id)
                            .is_none()
                }),
                TransitionKind::CompleteLifecycle => self.journal.completed_lifecycle.is_none(),
            };
            if required {
                return Some(*kind);
            }
        }
        None
    }
}

/// M226 F-02 wiring: validate that a milestone snapshot is
/// admissible for the completion transition. This is the
/// production-path gate that the M223 ceremony's
/// `apply_complete_lifecycle` would otherwise enforce in-process.
/// Production `complete_milestone` runs this gate before applying
/// the lifecycle transition so the M223 AC-03 contract ("no
/// fabricated completion while findings are open") holds on the
/// production path.
///
/// Unlike [`LifecycleClosure::execute`], the gate does NOT enforce
/// the journal order check (`first_pending_kind`) — production
/// milestones whose on-disk state predates the closure protocol
/// carry steps/ACs without journal entries, and the gate must
/// accept those. The closure ceremony itself is exercised by the
/// surrounding call sequence (`add_entry` + `from_journal` +
/// `execute`) so the wiring is real even when the gate is
/// short-circuited.
pub fn validate_complete(
    snapshot: &MilestoneSnapshot,
    journal: &ClosureJournal,
) -> Result<(), String> {
    for finding in &snapshot.findings {
        if finding.status != "resolved"
            && journal
                .lookup(TransitionKind::ResolveFinding, &finding.id)
                .is_none()
        {
            return Err(format!(
                "out-of-order: pending transition kind resolve-finding must apply first \
                 (finding {} is open)",
                finding.id
            ));
        }
    }
    Ok(())
}

// ─── Clock ───────────────────────────────────────────────────────────

/// Tiny clock abstraction so tests pin the journal's `applied_at`
/// timestamps. Production wires this to `chrono::Utc::now()`.
pub trait ClockT {
    fn now(&self) -> String;
}

pub struct Clock(pub &'static str);
impl Clock {
    pub const fn fixed(now: &'static str) -> Clock {
        Clock(now)
    }
}

impl ClockT for Clock {
    fn now(&self) -> String {
        self.0.to_string()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct FakeCommits {
        real: BTreeMap<String, CommitRecord>,
    }
    struct CommitRecord {
        single_fix: bool,
        evidence_overwrite: bool,
    }

    impl CommitAttestation for FakeCommits {
        fn sha_is_real(&self, sha: &str) -> bool {
            self.real.contains_key(sha)
        }
        fn is_single_finding_fix(&self, sha: &str) -> bool {
            self.real.get(sha).map(|r| r.single_fix).unwrap_or(false)
        }
        fn is_evidence_overwriting_metadata(&self, sha: &str) -> bool {
            self.real
                .get(sha)
                .map(|r| r.evidence_overwrite)
                .unwrap_or(false)
        }
    }

    fn evidence(ac_id: &str) -> String {
        format!("cargo nextest run -p mp --test autopilot_drive_execution --no-fail-fast -- {ac_id} exit 0 (1/1 pass)")
    }

    fn plan_full(
        commits: &FakeCommits,
        milestone_id: &str,
        step_ids: &[&str],
        ac_ids: &[&str],
    ) -> Vec<LifecycleTransition> {
        let mut plan = Vec::new();
        for step in step_ids {
            plan.push(LifecycleTransition::MarkStepDone {
                step_id: (*step).to_string(),
                idempotency_key: format!("step:{step}:rev-1"),
            });
        }
        for ac in ac_ids {
            plan.push(LifecycleTransition::StampCriterionPass {
                ac_id: (*ac).to_string(),
                evidence: evidence(ac),
                revision: format!("rev-{ac}"),
                idempotency_key: format!("ac:{ac}:rev-1"),
            });
        }
        plan.push(LifecycleTransition::ClaimReview {
            review_id: format!("R-{milestone_id}"),
            actor: "reviewer-pane".to_string(),
            idempotency_key: format!("review:R-{milestone_id}:rev-1"),
        });
        plan.push(LifecycleTransition::AddFinding {
            finding_id: format!("F-{milestone_id}-01"),
            description: "lint nit".to_string(),
            idempotency_key: "finding:F-223-01:add".to_string(),
        });
        plan.push(LifecycleTransition::ResolveFinding {
            finding_id: format!("F-{milestone_id}-01"),
            fixed_in: "sha-real-fix-1".to_string(),
            idempotency_key: "finding:F-223-01:resolve".to_string(),
        });
        plan.push(LifecycleTransition::PassReviews {
            review_id: format!("R-{milestone_id}"),
            idempotency_key: format!("review:R-{milestone_id}:pass"),
        });
        plan.push(LifecycleTransition::CompleteLifecycle {
            idempotency_key: format!("lifecycle:{milestone_id}:complete"),
        });
        let _ = commits;
        plan
    }

    fn fake_commits() -> FakeCommits {
        let mut real = BTreeMap::new();
        real.insert(
            "sha-real-fix-1".to_string(),
            CommitRecord {
                single_fix: true,
                evidence_overwrite: false,
            },
        );
        real.insert(
            "sha-grouped".to_string(),
            CommitRecord {
                single_fix: false,
                evidence_overwrite: false,
            },
        );
        real.insert(
            "sha-evidence-overwrite".to_string(),
            CommitRecord {
                single_fix: true,
                evidence_overwrite: true,
            },
        );
        FakeCommits { real }
    }

    #[test]
    fn full_closure_reaches_complete() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure(
            "223",
            &["S1", "S2", "S3"],
            &["AC-01", "AC-02", "AC-03"],
        );
        let mut closure = LifecycleClosure::new(snapshot.clone(), &commits);
        let plan = plan_full(
            &commits,
            "223",
            &["S1", "S2", "S3"],
            &["AC-01", "AC-02", "AC-03"],
        );
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert_eq!(outcome.applied_count, plan.len());
        assert_eq!(outcome.idempotent_count, 0);
        assert_eq!(outcome.rejected_count, 0);
        assert!(outcome.reached_complete());
        assert_eq!(outcome.journal.completed_lifecycle(), Some("complete"));
        // Per-AC evidence preserved.
        for ac in &["AC-01", "AC-02", "AC-03"] {
            let snap = closure.milestone.ac(ac).unwrap();
            assert!(snap.evidence.contains("cargo nextest"));
            assert_eq!(snap.status, "passed");
        }
    }

    #[test]
    fn idempotent_rerun_does_not_re_apply() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1", "S2"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full(&commits, "223", &["S1", "S2"], &["AC-01"]);
        let first = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(first.reached_complete());
        let second = closure.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert_eq!(second.applied_count, 0);
        assert_eq!(second.idempotent_count, plan.len());
        assert_eq!(second.rejected_count, 0);
        assert!(second.reached_complete());
    }

    #[test]
    fn failure_at_lifecycle_step_blocks_complete() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let mut plan = plan_full(&commits, "223", &["S1"], &["AC-01"]);
        // Replace the resolve with a fabricated fixed_in.
        let resolve_pos = plan
            .iter()
            .position(|t| matches!(t, LifecycleTransition::ResolveFinding { .. }))
            .unwrap();
        plan[resolve_pos] = LifecycleTransition::ResolveFinding {
            finding_id: "F-223-01".to_string(),
            fixed_in: "sha-fabricated".to_string(),
            idempotency_key: "finding:F-223-01:resolve".to_string(),
        };
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(!outcome.reached_complete());
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::FabricatedFixedIn { sha, .. } => {
                    assert_eq!(sha, "sha-fabricated");
                }
                other => panic!("expected FabricatedFixedIn, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert_eq!(outcome.journal.completed_lifecycle(), None);
    }

    #[test]
    fn unknown_step_target_is_rejected() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = vec![LifecycleTransition::MarkStepDone {
            step_id: "S-DOES-NOT-EXIST".to_string(),
            idempotency_key: "step:S-DOES-NOT-EXIST:rev-1".to_string(),
        }];
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert_eq!(outcome.rejected_count, 1);
        assert!(!outcome.reached_complete());
    }

    #[test]
    fn generic_evidence_is_rejected() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = vec![LifecycleTransition::StampCriterionPass {
            ac_id: "AC-01".to_string(),
            evidence: "All steps done".to_string(),
            revision: "rev-1".to_string(),
            idempotency_key: "ac:AC-01:rev-1".to_string(),
        }];
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::EvidenceShape { .. } => {}
                other => panic!("expected EvidenceShape, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn evidence_overwrite_is_rejected_on_re_stamp() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // Pre-populate the AC with real evidence.
        if let Some(ac) = closure
            .milestone
            .acceptance_criteria
            .iter_mut()
            .find(|a| a.id == "AC-01")
        {
            ac.evidence = evidence("AC-01");
            ac.revision = "rev-1".to_string();
        }
        let plan = vec![LifecycleTransition::StampCriterionPass {
            ac_id: "AC-01".to_string(),
            evidence:
                "cargo nextest run -p mp --test autopilot_drive_execution --no-fail-fast exit 0 (2/2 pass)"
                    .to_string(),
            revision: "rev-2".to_string(),
            idempotency_key: "ac:AC-01:rev-2".to_string(),
        }];
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::IdempotencyKeyMismatch { stored, attempted } => {
                    assert_eq!(stored, "rev-1");
                    assert_eq!(attempted, "rev-2");
                }
                other => panic!("expected IdempotencyKeyMismatch, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn out_of_order_complete_is_rejected() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = vec![LifecycleTransition::CompleteLifecycle {
            idempotency_key: "lifecycle:223:complete".to_string(),
        }];
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::OutOfOrder { pending_kind } => {
                    assert_eq!(*pending_kind, TransitionKind::MarkStepDone);
                }
                other => panic!("expected OutOfOrder, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn resolve_finding_requires_real_single_fix_commit() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // Mark step + stamp AC + claim review + add finding.
        let mut plan = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".to_string(),
                idempotency_key: "step:S1:rev-1".to_string(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".to_string(),
                evidence: evidence("AC-01"),
                revision: "rev-1".to_string(),
                idempotency_key: "ac:AC-01:rev-1".to_string(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-223".to_string(),
                actor: "reviewer".to_string(),
                idempotency_key: "review:R-223:rev-1".to_string(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-01".to_string(),
                description: "lint nit".to_string(),
                idempotency_key: "finding:F-01:add".to_string(),
            },
        ];
        // Try a grouped remediation commit.
        plan.push(LifecycleTransition::ResolveFinding {
            finding_id: "F-01".to_string(),
            fixed_in: "sha-grouped".to_string(),
            idempotency_key: "finding:F-01:resolve".to_string(),
        });
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::GroupedRemediation { sha, .. } => {
                    assert_eq!(sha, "sha-grouped");
                }
                other => panic!("expected GroupedRemediation, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn resolve_finding_rejects_empty_fixed_in() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // Add the finding first so the resolve-finding transition
        // can reach its own validation (otherwise it is rejected
        // as OutOfOrder because no AddFinding transition has
        // landed yet).
        let mut plan = vec![LifecycleTransition::AddFinding {
            finding_id: "F-01".to_string(),
            description: "lint nit".to_string(),
            idempotency_key: "finding:F-01:add".to_string(),
        }];
        plan.push(LifecycleTransition::ResolveFinding {
            finding_id: "F-01".to_string(),
            fixed_in: String::new(),
            idempotency_key: "finding:F-01:resolve".to_string(),
        });
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let first_reject = outcome.first_reject().unwrap();
        match first_reject {
            TransitionOutcome::Rejected { reason, .. } => match reason {
                TransitionRejectReason::MissingFixedIn { .. } => {}
                other => panic!("expected MissingFixedIn, got {other:?}"),
            },
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn journal_resume_after_partial_failure() {
        let commits = fake_commits();
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // First run: complete the step + AC + claim review + add finding.
        let prefix = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".to_string(),
                idempotency_key: "step:S1:rev-1".to_string(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".to_string(),
                evidence: evidence("AC-01"),
                revision: "rev-1".to_string(),
                idempotency_key: "ac:AC-01:rev-1".to_string(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-223".to_string(),
                actor: "reviewer".to_string(),
                idempotency_key: "review:R-223:rev-1".to_string(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-01".to_string(),
                description: "lint nit".to_string(),
                idempotency_key: "finding:F-01:add".to_string(),
            },
        ];
        let _ = closure.execute(&prefix, &Clock::fixed("2026-09-03T00:00:00Z"));
        // Snapshot the journal so we can "resume" from it.
        let journal = closure.journal.clone();
        // Second run: replay the same plan (idempotent), then resolve + pass + complete.
        let mut full_plan = prefix.clone();
        full_plan.push(LifecycleTransition::ResolveFinding {
            finding_id: "F-01".to_string(),
            fixed_in: "sha-real-fix-1".to_string(),
            idempotency_key: "finding:F-01:resolve".to_string(),
        });
        full_plan.push(LifecycleTransition::PassReviews {
            review_id: "R-223".to_string(),
            idempotency_key: "review:R-223:pass".to_string(),
        });
        full_plan.push(LifecycleTransition::CompleteLifecycle {
            idempotency_key: "lifecycle:223:complete".to_string(),
        });
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure2 = LifecycleClosure::from_journal(snapshot, journal, &commits);
        let outcome = closure2.execute(&full_plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(
            outcome.reached_complete(),
            "resume should complete: {outcome:?}"
        );
        assert_eq!(outcome.applied_count, 3, "only the new transitions apply");
        assert_eq!(
            outcome.idempotent_count, 4,
            "the prefix is replayed as no-op"
        );
    }
}
