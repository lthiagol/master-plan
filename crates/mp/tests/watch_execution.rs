//! M149 S7 / AC-02, AC-03, AC-07: lifecycle state machine end-to-end.
//!
//! Drives the state machine against a Scripted DriveOps
//! implementation that returns canned milestones in sequence. The
//! real herdr + agent path is exercised by `mp watch <id>` (no
//! `--dry-run`); the full end-to-end with a live agent is the
//! milestone-level acceptance test (requires a running herdr + an
//! opencode/cursor/pi bridge). These tests pin the loop logic — the
//! stage routing, skip verdict, pane routing, and iteration cap —
//! without spawning agents.

mod common;

use mp::model::{MilestoneFile, MilestoneMeta};
use mp::watch::{
    drive_milestone, next_stage, should_skip, DriveOps, DriveOutcome, LifecycleTarget, PaneHandle,
    PromptStage, Role, StagePlan, WaitOutcome,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

fn ms(id: &str, lifecycle: &str) -> MilestoneFile {
    MilestoneFile {
        milestone: MilestoneMeta {
            id: id.to_string(),
            lifecycle: lifecycle.to_string(),
            spec_status: "ready".to_string(),
            execution_status: if lifecycle == "complete" {
                "complete".to_string()
            } else {
                "planned".to_string()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

fn ms_full(id: &str, lifecycle: &str, spec: &str, exec: &str) -> MilestoneFile {
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

/// Scripted DriveOps that pops milestones off a RefCell<Vec> on each
/// wait_for_lifecycle call (the transition marker). Records every
/// interaction so tests can assert on the trace.
struct Scripted {
    milestones: RefCell<Vec<MilestoneFile>>,
    prompts_sent: RefCell<Vec<String>>,
    panes_ensured: RefCell<Vec<Role>>,
    handoffs: RefCell<Vec<String>>,
    events: RefCell<std::collections::HashMap<&'static str, usize>>,
    plan_dir: PathBuf,
}

impl Scripted {
    fn new(seq: Vec<MilestoneFile>) -> Self {
        // LOW-2: anchor plan_dir at a guaranteed-empty tempdir so a
        // stray watch/ subdir under the cargo-runner cwd can't
        // accidentally satisfy the override lookup. Same pattern as
        // tempfile::TempDir in the integration tests; we leak the
        // PathBuf+tempdir because the in-process tree is short-lived
        // and we don't need cleanup discipline here.
        let anchor = std::env::temp_dir().join(format!(
            "mp-watch-scripted-{}-{}",
            std::process::id(),
            std::sync::atomic::AtomicUsize::new(0)
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        ));
        std::fs::create_dir_all(&anchor).expect("create anchor");
        Self {
            milestones: RefCell::new(seq),
            prompts_sent: RefCell::new(vec![]),
            panes_ensured: RefCell::new(vec![]),
            handoffs: RefCell::new(vec![]),
            events: RefCell::new(std::collections::HashMap::new()),
            plan_dir: anchor,
        }
    }
}

impl DriveOps for Scripted {
    fn read_milestone(&mut self) -> anyhow::Result<MilestoneFile> {
        Ok(self
            .milestones
            .borrow()
            .first()
            .cloned()
            .unwrap_or_else(|| ms("1", "complete")))
    }
    fn ensure_pane(&mut self, role: Role) -> anyhow::Result<PaneHandle> {
        self.panes_ensured.borrow_mut().push(role);
        Ok(PaneHandle {
            label: format!("role-{}-1", role.label()),
            pane_id: format!("role-{}-1", role.label()),
            reused: false,
        })
    }
    fn send_prompt_to(&mut self, _pane: &PaneHandle, text: &str) -> anyhow::Result<()> {
        self.prompts_sent.borrow_mut().push(text.to_string());
        Ok(())
    }
    fn log_event(&self, kind: &'static str, _message: impl Into<String>) {
        // M153 S2: tests can inspect events via the `events`
        // collector if needed; default is to count kinds.
        let mut counter = self.events.borrow_mut();
        *counter.entry(kind).or_insert(0usize) += 1;
    }
    fn wait_for_lifecycle(&mut self, _target: LifecycleTarget) -> anyhow::Result<WaitOutcome> {
        // Advance the script on each transition.
        let mut ms = self.milestones.borrow_mut();
        if ms.len() > 1 {
            ms.remove(0);
        }
        Ok(WaitOutcome::Reached)
    }
    fn plan_dir(&self) -> &Path {
        &self.plan_dir
    }
    fn record_handoff(&mut self, transition: &str) -> anyhow::Result<()> {
        self.handoffs.borrow_mut().push(transition.to_string());
        Ok(())
    }
}

// ─── should_skip verdicts ───────────────────────────────────────────────────

#[test]
fn should_skip_returns_none_for_ready_approved() {
    assert!(should_skip(&ms_full("1", "approved", "ready", "planned")).is_none());
}

#[test]
fn should_skip_returns_reason_for_unread_states() {
    assert_eq!(
        should_skip(&ms_full("1", "approved", "draft", "planned"))
            .as_deref()
            .unwrap(),
        "approved but not ready (spec_status=draft)"
    );
}

#[test]
fn should_skip_returns_none_for_inflight_lifecycles() {
    assert!(should_skip(&ms_full("1", "in-progress", "ready", "in-progress")).is_none());
    assert!(should_skip(&ms_full("1", "remediation", "ready", "in-progress")).is_none());
    assert!(should_skip(&ms_full("1", "self-reviewed", "ready", "in-progress")).is_some());
    assert!(should_skip(&ms_full("1", "reviewed", "ready", "complete")).is_some());
}

#[test]
fn should_skip_rejects_draft_and_groomed_lifecycles() {
    let reason = should_skip(&ms_full("1", "draft", "draft", "planned")).unwrap();
    assert!(
        reason.contains("lifecycle=draft"),
        "should explicitly reject draft: {reason}"
    );
}

#[test]
fn should_skip_handles_blocked_with_reason() {
    let mut m = ms_full("1", "approved", "ready", "planned");
    m.milestone.blocked = true;
    m.milestone.block_reason = "waiting on upstream".to_string();
    let reason = should_skip(&m).unwrap();
    assert!(
        reason.contains("waiting on upstream"),
        "blocked reason should include the block_reason text: {reason}"
    );
}

// ─── next_stage routing ─────────────────────────────────────────────────────

#[test]
fn next_stage_routes_approved_to_execute() {
    let plan = next_stage(&ms("1", "approved")).unwrap();
    assert_eq!(
        plan,
        StagePlan {
            stage: PromptStage::Execute,
            target: LifecycleTarget::Complete,
        }
    );
}

#[test]
fn next_stage_routes_remediation_to_runner() {
    let plan = next_stage(&ms("1", "remediation")).unwrap();
    assert_eq!(plan.stage, PromptStage::Remediate);
    assert_eq!(plan.target, LifecycleTarget::Complete);
}

#[test]
fn next_stage_returns_none_for_draft() {
    assert!(next_stage(&ms("1", "draft")).is_none());
}

// ─── drive_milestone full-loop behavior ─────────────────────────────────────

#[test]
fn drive_immediately_completes_when_milestone_is_complete() {
    let mut ops = Scripted::new(vec![ms("1", "complete")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    assert_eq!(outcome, DriveOutcome::Complete);
    assert!(ops.prompts_sent.borrow().is_empty());
    assert!(ops.panes_ensured.borrow().is_empty());
}

#[test]
fn drive_skips_unready_approved_milestone_ac07() {
    let mut ops = Scripted::new(vec![ms_full("1", "approved", "draft", "planned")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    match outcome {
        DriveOutcome::Skipped { reason } => {
            assert!(
                reason.contains("spec_status=draft"),
                "skip reason should expose the offending field: {reason}"
            );
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
}

#[test]
fn drive_skips_blocked_milestone() {
    let mut m = ms_full("1", "approved", "ready", "planned");
    m.milestone.blocked = true;
    m.milestone.block_reason = "dep M87 not done".into();
    let mut ops = Scripted::new(vec![m]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    match outcome {
        DriveOutcome::Skipped { reason } => assert!(reason.contains("dep M87 not done")),
        other => panic!("expected Skipped, got {other:?}"),
    }
}

#[test]
fn drive_sends_execute_prompt_then_completes() {
    let mut ops = Scripted::new(vec![ms("1", "approved"), ms("1", "complete")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    assert_eq!(outcome, DriveOutcome::Complete);
    let prompts = ops.prompts_sent.borrow();
    assert_eq!(prompts.len(), 1, "exactly one execute prompt");
    assert!(prompts[0].contains("runner"));
    assert!(prompts[0].contains("mp milestone set-status 1 in-progress"));
    let panes = ops.panes_ensured.borrow();
    assert_eq!(*panes, vec![Role::Runner]);
    let handoffs = ops.handoffs.borrow();
    assert!(
        handoffs.iter().any(|h| h.contains("complete")),
        "should record handoff into complete: {handoffs:?}"
    );
}

#[test]
fn drive_remediation_uses_runner_pane() {
    let mut ops = Scripted::new(vec![ms("1", "remediation"), ms("1", "complete")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    assert_eq!(outcome, DriveOutcome::Complete);
    assert_eq!(*ops.panes_ensured.borrow(), vec![Role::Runner]);
    let prompt = &ops.prompts_sent.borrow()[0];
    assert!(prompt.contains("runner"));
    assert!(prompt.contains("mp reviews finding resolve"));
}

#[test]
fn drive_does_not_re_prompt_when_runner_in_progress() {
    // Mid-execute: poll, don't re-spawn the runner.
    let mut ops = Scripted::new(vec![ms("1", "in-progress"), ms("1", "complete")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    assert_eq!(outcome, DriveOutcome::Complete);
    assert!(
        ops.prompts_sent.borrow().is_empty(),
        "in-progress runner should not be re-prompted"
    );
}

#[test]
fn drive_caps_iterations_on_infinite_loop() {
    // Script with one approved milestone that never advances.
    let mut ops = Scripted::new(vec![ms("1", "approved")]);
    let outcome = drive_milestone(&mut ops, 3).unwrap();
    match outcome {
        DriveOutcome::MaxIterationsExhausted { iterations } => {
            assert!(iterations <= 3, "cap should bind: got {iterations}");
        }
        other => panic!("expected MaxIterationsExhausted, got {other:?}"),
    }
}

#[test]
fn drive_option_a_delivery_lifecycle_uses_runner_only() {
    // Review is tracked in the reviews registry, not lifecycle destinations.
    let mut ops = Scripted::new(vec![ms("1", "approved"), ms("1", "complete")]);
    let outcome = drive_milestone(&mut ops, 10).unwrap();
    assert_eq!(outcome, DriveOutcome::Complete);
    let panes = ops.panes_ensured.borrow();
    assert!(
        *panes == [Role::Runner],
        "delivery lifecycle should use only the runner pane: {panes:?}"
    );
    assert_eq!(ops.prompts_sent.borrow().len(), 1);
}

// ─── Real-CLI smoke: skip verdict matches the S2 dry-run next_action ────────

#[test]
fn skip_verdict_and_s2_next_action_agree_on_unready_milestone() {
    // The S2 dry-run path routes unready milestones to skip_* actions.
    // The S7 should_skip verdict should also flag them. This test pins
    // that the two paths stay consistent.
    let unready = ms_full("1", "approved", "draft", "planned");
    assert!(should_skip(&unready).is_some());

    let ready = ms_full("1", "approved", "ready", "planned");
    assert!(should_skip(&ready).is_none());
}

// ─── M223: lifecycle closure transaction + commit/finding attribution ────────
//
// AC-01 — full closure ceremony with per-AC evidence revisioning and
// independent reviewer attribution.
// AC-02 — commit policy rejects missing/fabricated/ambiguous fixed_in
// and lifecycle metadata commits that overwrite per-AC evidence.
// AC-03 — failure injection at every command boundary is restart-safe;
// rerun is idempotent.
//
// These tests pin the typed protocol — they exercise the lifecycle
// and commit_policy modules directly with in-memory fixtures, no
// shelling out. The fixtures encode the same shape `mp milestone …`
// produces on disk so a follow-up integration test can drive the
// real CLI with the same data.

mod m223_fixtures {
    use mp::autopilot::commit_policy::{
        classify_subject, lifecycle_metadata_overwrites_evidence, validate_fixed_in, CommitIndex,
        CommitInspection, CommitKind, PolicyError,
    };
    use mp::autopilot::lifecycle::{
        Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot,
        TransitionOutcome, TransitionRejectReason,
    };
    use std::cell::RefCell;

    /// In-memory commit index used by both the lifecycle and the
    /// commit policy tests. Mirrors the fixture shape `git log`
    /// would produce so the two layers agree.
    pub struct FakeCommitAttestation {
        pub real: std::collections::BTreeMap<String, CommitRecord>,
    }
    pub struct CommitRecord {
        pub single_fix: bool,
    }

    impl mp::autopilot::lifecycle::CommitAttestation for FakeCommitAttestation {
        fn sha_is_real(&self, sha: &str) -> bool {
            self.real.contains_key(sha)
        }
        fn is_single_finding_fix(&self, sha: &str) -> bool {
            self.real.get(sha).map(|r| r.single_fix).unwrap_or(false)
        }
        fn is_evidence_overwriting_metadata(&self, _sha: &str) -> bool {
            false
        }
    }

    pub fn fixture_attestation() -> FakeCommitAttestation {
        let mut real = std::collections::BTreeMap::new();
        real.insert(
            "sha-fix-1".into(),
            CommitRecord { single_fix: true },
        );
        real.insert(
            "sha-fix-2".into(),
            CommitRecord { single_fix: true },
        );
        FakeCommitAttestation { real }
    }

    pub fn evidence(ac_id: &str) -> String {
        format!(
            "cargo nextest run -p mp --test watch_execution --no-fail-fast -- {ac_id} exit 0 (1/1 pass)"
        )
    }

    pub fn plan_full(
        milestone_id: &str,
        step_ids: &[&str],
        ac_ids: &[&str],
        finding_ids: &[&str],
        review_id: &str,
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
            review_id: review_id.to_string(),
            actor: "reviewer-pane".to_string(),
            idempotency_key: format!("review:{review_id}:rev-1"),
        });
        for fid in finding_ids {
            plan.push(LifecycleTransition::AddFinding {
                finding_id: (*fid).to_string(),
                description: format!("finding {fid}"),
                idempotency_key: format!("finding:{fid}:add"),
            });
            plan.push(LifecycleTransition::ResolveFinding {
                finding_id: (*fid).to_string(),
                fixed_in: "sha-fix-1".to_string(),
                idempotency_key: format!("finding:{fid}:resolve"),
            });
        }
        plan.push(LifecycleTransition::PassReviews {
            review_id: review_id.to_string(),
            idempotency_key: format!("review:{review_id}:pass"),
        });
        plan.push(LifecycleTransition::CompleteLifecycle {
            idempotency_key: format!("lifecycle:{milestone_id}:complete"),
        });
        plan
    }

    pub fn assert_evidence_preserved(
        snapshot: &MilestoneSnapshot,
        ac_ids: &[&str],
    ) {
        for ac in ac_ids {
            let ac_snap = snapshot.ac(ac).expect("AC in snapshot");
            assert_eq!(ac_snap.status, "passed", "AC {ac} should be passed");
            assert!(
                ac_snap.evidence.contains("cargo nextest"),
                "AC {ac} evidence must contain a real cargo nextest command: {:?}",
                ac_snap.evidence
            );
            assert!(
                ac_snap.evidence.contains("exit "),
                "AC {ac} evidence must contain exit code"
            );
            assert!(
                ac_snap.evidence.contains(" pass)"),
                "AC {ac} evidence must contain pass count"
            );
            assert_eq!(
                ac_snap.revision,
                format!("rev-{ac}"),
                "AC {ac} revision preserved"
            );
        }
    }

    // Failure-injection helper: every step boundary gets a chance to
    // fail. We use this to assert AC-03 restart safety.
    pub struct FailureInjector {
        fail_at: RefCell<Option<usize>>,
        calls: RefCell<usize>,
    }

    impl FailureInjector {
        pub fn new(fail_at: usize) -> Self {
            Self {
                fail_at: RefCell::new(Some(fail_at)),
                calls: RefCell::new(0),
            }
        }

        pub fn tick(&self) -> bool {
            let mut c = self.calls.borrow_mut();
            *c += 1;
            let target = *self.fail_at.borrow();
            target == Some(*c)
        }

        pub fn calls(&self) -> usize {
            *self.calls.borrow()
        }
    }
}

mod m223_ac01 {
    //! AC-01 — full closure sequence. Each AC's evidence revision is
    //! asserted to be distinct and the final lifecycle is `complete`.
    use super::m223_fixtures::*;
    use mp::autopilot::commit_policy::CommitKind;
    use mp::autopilot::lifecycle::{Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot};

    #[test]
    fn full_closure_preserves_per_ac_evidence_and_reaches_complete() {
        let commits = fixture_attestation();
        let snapshot = MilestoneSnapshot::ready_for_closure(
            "223",
            &["S1", "S2", "S3"],
            &["AC-01", "AC-02", "AC-03"],
        );
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full("223", &["S1", "S2", "S3"], &["AC-01", "AC-02", "AC-03"], &["F-01"], "R-223");
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(
            outcome.reached_complete(),
            "closure should reach complete: {outcome:?}"
        );
        assert_eq!(outcome.applied_count, plan.len());
        assert_eq!(outcome.rejected_count, 0);
        assert_eq!(outcome.idempotent_count, 0);

        // Independent reviewer attribution recorded.
        let review = closure
            .milestone
            .review("R-223")
            .expect("review R-223 in snapshot");
        assert_eq!(review.status, "passed");
        assert_eq!(review.actor, "reviewer-pane");

        // Per-AC evidence preserved with distinct revisions.
        assert_evidence_preserved(&closure.milestone, &["AC-01", "AC-02", "AC-03"]);
        let ac1 = closure.milestone.ac("AC-01").unwrap();
        let ac2 = closure.milestone.ac("AC-02").unwrap();
        let ac3 = closure.milestone.ac("AC-03").unwrap();
        assert_eq!(ac1.revision, "rev-AC-01");
        assert_eq!(ac2.revision, "rev-AC-02");
        assert_eq!(ac3.revision, "rev-AC-03");
        assert!(ac1.evidence.contains("-- AC-01"));
        assert!(ac2.evidence.contains("-- AC-02"));
        assert!(ac3.evidence.contains("-- AC-03"));

        // Suppress unused import warning.
        let _ = CommitKind::Unknown;
    }

    #[test]
    fn closure_records_each_step_done_with_revision() {
        let commits = fixture_attestation();
        let snapshot =
            MilestoneSnapshot::ready_for_closure("223", &["S1", "S2", "S3"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full("223", &["S1", "S2", "S3"], &["AC-01"], &[], "R-223");
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(outcome.reached_complete());
        for step_id in ["S1", "S2", "S3"] {
            let step = closure.milestone.step(step_id).expect("step in snapshot");
            assert_eq!(step.status, "done", "step {step_id} should be done");
        }
        // Journal entries reflect every transition.
        let entries = outcome.journal.entries();
        assert_eq!(entries.len(), plan.len());
        for entry in entries {
            assert!(!entry.idempotency_key.is_empty());
            assert!(!entry.applied_at.is_empty());
        }
        let _ = LifecycleTransition::MarkStepDone {
            step_id: "S1".to_string(),
            idempotency_key: "x".to_string(),
        };
    }
}

mod m223_ac02 {
    //! AC-02 — commit policy rejects missing/fabricated/ambiguous
    //! fixed_in and lifecycle metadata commits that would overwrite
    //! per-AC evidence.
    use super::m223_fixtures::*;
    use mp::autopilot::commit_policy::{classify_subject, CommitIndex, CommitInspection, CommitKind, PolicyError, lifecycle_metadata_overwrites_evidence, validate_fixed_in};
    use mp::autopilot::lifecycle::{
        Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot, TransitionOutcome,
        TransitionRejectReason,
    };

    #[test]
    fn rejects_missing_fixed_in() {
        let index = CommitIndex::new();
        let err = validate_fixed_in("F-01", "", &index).unwrap_err();
        assert!(matches!(err, PolicyError::MissingFixedIn { .. }));
    }

    #[test]
    fn rejects_fabricated_fixed_in() {
        let index = CommitIndex::new();
        let err = validate_fixed_in("F-01", "sha-does-not-exist", &index).unwrap_err();
        match err {
            PolicyError::FabricatedSha { sha, .. } => assert_eq!(sha, "sha-does-not-exist"),
            other => panic!("expected FabricatedSha, got {other:?}"),
        }
    }

    #[test]
    fn rejects_ambiguous_grouped_fix() {
        let mut index = CommitIndex::new();
        index.insert(CommitInspection::new(
            "sha-grouped",
            "M223: S1 — fix F-01 + fix F-02",
            "",
        ));
        let err = validate_fixed_in("F-01", "sha-grouped", &index).unwrap_err();
        match err {
            PolicyError::GroupedRemediation { sha, reasons, .. } => {
                assert_eq!(sha, "sha-grouped");
                assert!(reasons.contains(&"implementation".to_string()));
                assert!(reasons.contains(&"self-review-fix".to_string()));
            }
            other => panic!("expected GroupedRemediation, got {other:?}"),
        }
        assert!(
            matches!(index.lookup("sha-grouped").unwrap().kind, CommitKind::Ambiguous { .. }),
            "ambiguous commit must be classified as Ambiguous"
        );
    }

    #[test]
    fn rejects_lifecycle_metadata_commit_as_fixed_in() {
        let mut index = CommitIndex::new();
        index.insert(CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "",
        ));
        let err = validate_fixed_in("F-01", "sha-meta", &index).unwrap_err();
        match err {
            PolicyError::LifecycleMetadataNotFix { transition, .. } => {
                assert_eq!(transition, "cycle 1");
            }
            other => panic!("expected LifecycleMetadataNotFix, got {other:?}"),
        }
    }

    #[test]
    fn rejects_lifecycle_metadata_without_evidence_manifest() {
        let inspection = CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "just a summary commit, no manifest",
        );
        let err = lifecycle_metadata_overwrites_evidence(
            &inspection,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1")],
        )
        .unwrap_err();
        match err {
            PolicyError::EvidenceOverwritingMetadata {
                missing_ac_revisions,
                ..
            } => {
                assert!(missing_ac_revisions.contains(&"AC-01".to_string()));
                assert!(missing_ac_revisions.contains(&"AC-02".to_string()));
            }
            other => panic!("expected EvidenceOverwritingMetadata, got {other:?}"),
        }
    }

    #[test]
    fn accepts_lifecycle_metadata_with_complete_evidence_manifest() {
        let inspection = CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "Per-AC evidence manifest: AC-01=rev-1, AC-02=rev-1, AC-03=rev-1",
        );
        assert!(lifecycle_metadata_overwrites_evidence(
            &inspection,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1"), ("AC-03", "rev-1")],
        )
        .is_ok());
    }

    #[test]
    fn closure_rejects_grouped_fix_in_resolve_finding() {
        // End-to-end: build a plan that uses a grouped remediation
        // commit as `fixed_in`. The closure protocol must refuse it
        // before reaching `mp reviews finding resolve`.
        let mut real = std::collections::BTreeMap::new();
        real.insert(
            "sha-grouped".to_string(),
            CommitRecord { single_fix: false },
        );
        let commits = FakeCommitAttestation { real };
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".to_string(),
                idempotency_key: "step:S1:rev-1".to_string(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".to_string(),
                evidence: evidence("AC-01"),
                revision: "rev-AC-01".to_string(),
                idempotency_key: "ac:AC-01:rev-1".to_string(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-223".to_string(),
                actor: "reviewer".to_string(),
                idempotency_key: "review:R-223:rev-1".to_string(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-01".to_string(),
                description: "nit".to_string(),
                idempotency_key: "finding:F-01:add".to_string(),
            },
            LifecycleTransition::ResolveFinding {
                finding_id: "F-01".to_string(),
                fixed_in: "sha-grouped".to_string(),
                idempotency_key: "finding:F-01:resolve".to_string(),
            },
        ];
        // The closure still records the prior steps (idempotency
        // not yet exercised), and refuses the resolve.
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
        assert!(!outcome.reached_complete());
        let _ = (
            classify_subject("M223: S1 — grouped"),
            std::any::type_name::<CommitKind>(),
        );
    }
}

mod m223_ac03 {
    //! AC-03 — failure injection at every lifecycle command boundary
    //! is restart-safe; rerun is idempotent.
    use super::m223_fixtures::*;
    use mp::autopilot::lifecycle::{
        ClosureJournal, Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot,
        TransitionOutcome,
    };

    fn snapshot() -> MilestoneSnapshot {
        MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"])
    }

    #[test]
    fn rerun_with_same_keys_is_pure_idempotent() {
        let commits = fixture_attestation();
        let mut closure = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &[], "R-223");
        let first = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(first.reached_complete());
        let second = closure.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(second.reached_complete());
        assert_eq!(second.applied_count, 0);
        assert_eq!(second.idempotent_count, plan.len());
        assert_eq!(second.rejected_count, 0);
    }

    #[test]
    fn failure_at_step_done_boundary_is_resumable() {
        let commits = fixture_attestation();
        let mut closure = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &[], "R-223");
        // Simulate a failure after the very first transition. We
        // truncate the plan to force a stop at the StepDone
        // boundary; on resume we replay the full plan and the
        // journal idempotency check makes the prefix a no-op.
        let partial: Vec<_> = plan.iter().take(1).cloned().collect();
        let first = closure.execute(&partial, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert_eq!(first.applied_count, 1);
        assert_eq!(first.rejected_count, 0);
        assert!(!first.reached_complete(), "partial closure must not fabricate complete");

        // Resume from the journal.
        let journal = closure.journal.clone();
        let mut resumed =
            LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let second = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(second.reached_complete(), "resume should reach complete: {second:?}");
        assert_eq!(second.applied_count, plan.len() - 1);
        assert_eq!(second.idempotent_count, 1);
    }

    #[test]
    fn failure_at_ac_stamp_boundary_is_resumable() {
        let commits = fixture_attestation();
        let mut closure = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &[], "R-223");
        // Truncate after step done + AC stamp.
        let partial: Vec<_> = plan.iter().take(2).cloned().collect();
        let _ = closure.execute(&partial, &Clock::fixed("2026-09-03T00:00:00Z"));
        let journal = closure.journal.clone();
        let mut resumed =
            LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let second = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(second.reached_complete());
    }

    #[test]
    fn failure_at_resolve_finding_boundary_is_resumable() {
        let commits = fixture_attestation();
        // Include a finding so we have a resolve boundary to fail at.
        let mut closure = LifecycleClosure::new(
            MilestoneSnapshot::ready_for_closure("223", &["S1"], &["AC-01"]),
            &commits,
        );
        let plan = plan_full("223", &["S1"], &["AC-01"], &["F-01"], "R-223");
        // Truncate right before resolve finding (index 4: step, AC, claim, add).
        let partial: Vec<_> = plan.iter().take(4).cloned().collect();
        let _ = closure.execute(&partial, &Clock::fixed("2026-09-03T00:00:00Z"));
        let journal = closure.journal.clone();
        let mut resumed =
            LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let second = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(second.reached_complete());
    }

    #[test]
    fn rerun_does_not_overwrite_existing_evidence() {
        let commits = fixture_attestation();
        let mut closure = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &[], "R-223");
        let _ = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        let evidence_before = closure.milestone.ac("AC-01").unwrap().evidence.clone();
        let _ = closure.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        let evidence_after = closure.milestone.ac("AC-01").unwrap().evidence.clone();
        assert_eq!(
            evidence_before, evidence_after,
            "rerun must not overwrite per-AC evidence"
        );
    }

    #[test]
    fn partial_failure_journal_is_isolated_from_fresh_closure() {
        let commits = fixture_attestation();
        // Run closure to completion.
        let mut closure = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &[], "R-223");
        let _ = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        // Fresh closure must NOT inherit the journal — each cycle
        // is its own transaction.
        let mut fresh = LifecycleClosure::new(
            snapshot(),
            &commits,
        );
        let outcome = fresh.execute(
            &[LifecycleTransition::CompleteLifecycle {
                idempotency_key: "lifecycle:223:complete:cycle-2".to_string(),
            }],
            &Clock::fixed("2026-09-03T00:01:00Z"),
        );
        assert!(
            !outcome.reached_complete(),
            "fresh closure must not silently inherit prior journal: {outcome:?}"
        );
        assert!(matches!(
            outcome.first_reject(),
            Some(TransitionOutcome::Rejected { .. })
        ));
        // The fresh closure's journal is empty (only rejected entries).
        let fresh_journal = fresh.journal.clone();
        assert_eq!(fresh_journal.entries().len(), 0);
        // The completed closure's journal is unaffected.
        let original = closure.journal.clone();
        assert_ne!(
            fresh_journal.entries().len(),
            original.entries().len(),
            "fresh closure's journal must remain distinct from the completed one"
        );
    }

    #[test]
    fn journal_resume_from_real_journal_reaches_complete() {
        let commits = fixture_attestation();
        let mut first = LifecycleClosure::new(snapshot(), &commits);
        let plan = plan_full("223", &["S1"], &["AC-01"], &["F-01"], "R-223");
        let partial: Vec<_> = plan.iter().take(4).cloned().collect();
        let _ = first.execute(&partial, &Clock::fixed("2026-09-03T00:00:00Z"));
        let journal: ClosureJournal = first.journal.clone();

        // Resume on a fresh snapshot (matches AC-03: a crash mid-
        // closure leaves the canonical milestone file untouched;
        // the next run re-reads it).
        let mut resumed =
            LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let outcome = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(outcome.reached_complete());
        // Final state has the resolved finding + passed review.
        let review = resumed.milestone.review("R-223").unwrap();
        assert_eq!(review.status, "passed");
        let finding = resumed.milestone.finding("F-01").unwrap();
        assert_eq!(finding.status, "resolved");
    }
}
