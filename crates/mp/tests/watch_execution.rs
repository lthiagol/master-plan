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

#[allow(dead_code, unused_imports)]
mod m223_fixtures {
    use mp::autopilot::commit_policy::{
        classify_subject, lifecycle_metadata_overwrites_evidence, validate_fixed_in, CommitIndex,
        CommitInspection, CommitKind, PolicyError,
    };
    use mp::autopilot::lifecycle::{
        Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot, TransitionOutcome,
        TransitionRejectReason,
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
        real.insert("sha-fix-1".into(), CommitRecord { single_fix: true });
        real.insert("sha-fix-2".into(), CommitRecord { single_fix: true });
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

    pub fn assert_evidence_preserved(snapshot: &MilestoneSnapshot, ac_ids: &[&str]) {
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
    use mp::autopilot::lifecycle::{
        Clock, LifecycleClosure, LifecycleTransition, MilestoneSnapshot,
    };

    #[test]
    fn full_closure_preserves_per_ac_evidence_and_reaches_complete() {
        let commits = fixture_attestation();
        let snapshot = MilestoneSnapshot::ready_for_closure(
            "223",
            &["S1", "S2", "S3"],
            &["AC-01", "AC-02", "AC-03"],
        );
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full(
            "223",
            &["S1", "S2", "S3"],
            &["AC-01", "AC-02", "AC-03"],
            &["F-01"],
            "R-223",
        );
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
        let snapshot = MilestoneSnapshot::ready_for_closure("223", &["S1", "S2", "S3"], &["AC-01"]);
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
    use mp::autopilot::commit_policy::{
        classify_subject, lifecycle_metadata_overwrites_evidence, validate_fixed_in, CommitIndex,
        CommitInspection, CommitKind, PolicyError,
    };
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
            matches!(
                index.lookup("sha-grouped").unwrap().kind,
                CommitKind::Ambiguous { .. }
            ),
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
        Clock, ClosureJournal, LifecycleClosure, LifecycleTransition, MilestoneSnapshot,
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
        assert!(
            !first.reached_complete(),
            "partial closure must not fabricate complete"
        );

        // Resume from the journal.
        let journal = closure.journal.clone();
        let mut resumed = LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let second = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(
            second.reached_complete(),
            "resume should reach complete: {second:?}"
        );
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
        let mut resumed = LifecycleClosure::from_journal(snapshot(), journal, &commits);
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
        let mut resumed = LifecycleClosure::from_journal(snapshot(), journal, &commits);
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
        let mut fresh = LifecycleClosure::new(snapshot(), &commits);
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
        let mut resumed = LifecycleClosure::from_journal(snapshot(), journal, &commits);
        let outcome = resumed.execute(&plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(outcome.reached_complete());
        // Final state has the resolved finding + passed review.
        let review = resumed.milestone.review("R-223").unwrap();
        assert_eq!(review.status, "passed");
        let finding = resumed.milestone.finding("F-01").unwrap();
        assert_eq!(finding.status, "resolved");
    }
}

// ─── M224: reviewer execution isolation + clean-room policy ───────────
//
// AC-01 — review environment records independent provenance
//         (binary / worktree / target-dir / pid / actor identity).
// AC-02 — mode selection (Normal default, CleanRoom only when
//         explicitly configured or provenance checks fail) and
//         no unconditional cargo clean.
// AC-03 — pre-review gate refuses unsafe environments (dirty
//         worktree / shared actor / stale binary / unverifiable
//         env) with a typed, actionable error; shared target-dir
//         escalates to clean-room rather than blocking.

#[allow(dead_code, unused_imports)]
mod m224_fixtures {
    use mp::autopilot::review_env::{
        build_provenance, gate, provenance_issues, ActorIdentity, GateInputs, ReviewEnvConfig,
        ReviewEnvDecision, ReviewEnvError, ReviewerProvenance,
    };
    use std::path::PathBuf;

    pub const RUNNER_PID: u32 = 9999;
    pub const RUNNER_TARGET: &str = "/tmp/m224-runner-target";
    pub const REVIEW_TARGET: &str = "/tmp/m224-reviewer-target";

    pub fn review_target() -> PathBuf {
        PathBuf::from(REVIEW_TARGET)
    }
    pub fn runner_target() -> PathBuf {
        PathBuf::from(RUNNER_TARGET)
    }
    pub fn worktree() -> PathBuf {
        PathBuf::from("/tmp/m224-wt")
    }

    pub fn reviewer_actor() -> ActorIdentity {
        ActorIdentity::reviewer("s-m224", "reviewer-pane-w12:p27", "2026-09-03T00:00:00Z")
    }
    pub fn runner_actor() -> ActorIdentity {
        ActorIdentity::runner("s-m224", "runner-pane-w12:p17", "2026-09-03T00:00:00Z")
    }

    pub fn fresh_provenance() -> ReviewerProvenance {
        build_provenance(
            "s-m224",
            "reviewer-pane-w12:p27",
            "2026-09-03T00:00:00Z",
            PathBuf::from("/usr/local/bin/mp"),
            Some("sha-fresh"),
            worktree(),
            review_target(),
            4242,
        )
    }
}

mod m224_ac01 {
    //! AC-01 — review environment records independent provenance:
    //! binary path, worktree, target directory, pid, and actor
    //! identity are distinct from the runner.
    use super::m224_fixtures::*;

    #[test]
    fn provenance_carries_distinct_actor_identity_from_runner() {
        let env = fresh_provenance();
        let runner = runner_actor();
        assert!(
            env.actor.distinct_from(&runner),
            "reviewer must be distinct from runner"
        );
        assert_eq!(env.actor.lane, "reviewer");
        assert_eq!(runner.lane, "runner");
        assert_ne!(env.actor.actor_token, runner.actor_token);
        // Session is intentionally shared — both lanes work the
        // same session.
        assert_eq!(env.actor.session_id, runner.session_id);
    }

    #[test]
    fn provenance_target_dir_is_isolated_from_runner() {
        let env = fresh_provenance();
        assert!(
            env.target_dir_is_isolated(&runner_target()),
            "target dir must not equal runner's"
        );
    }

    #[test]
    fn provenance_pid_is_fresh_from_runner() {
        let env = fresh_provenance();
        assert!(
            env.pid_is_fresh(9999),
            "reviewer pid {} should differ from runner pid",
            env.pid
        );
        assert!(
            !env.pid_is_fresh(env.pid),
            "an actor is never distinct from itself by pid"
        );
    }

    #[test]
    fn provenance_serializes_to_expected_kebab_shape() {
        let env = fresh_provenance();
        let json = serde_json::to_value(&env).expect("serialize");
        // rename_all = "kebab-case" applies to all fields including paths.
        assert!(json.get("binary-path").is_some());
        assert!(json.get("binary-sha").is_some());
        assert!(json.get("worktree-path").is_some());
        assert!(json.get("target-dir").is_some());
        assert!(json.get("pid").is_some());
        let actor = json.get("actor").expect("actor");
        assert_eq!(actor.get("lane").and_then(|v| v.as_str()), Some("reviewer"));
        assert_eq!(
            actor.get("session-id").and_then(|v| v.as_str()),
            Some("s-m224")
        );
    }
}

mod m224_ac02 {
    //! AC-02 — mode selection (Normal default, CleanRoom only when
    //! explicitly configured or provenance checks fail) and no
    //! unconditional cargo clean.
    use super::m224_fixtures::*;
    use mp::autopilot::review_env::{
        clean_room_commands, select_mode, CleanRoomTrigger, ModeSelection, ReviewEnvConfig,
        ReviewEnvMode,
    };

    #[test]
    fn defaults_to_normal_with_empty_issue_list() {
        let cfg = ReviewEnvConfig::default();
        let sel: ModeSelection = select_mode(&cfg, &[]);
        assert_eq!(sel.mode, ReviewEnvMode::Normal);
        assert!(sel.trigger.is_none());
        assert!(
            sel.pre_launch_commands.is_empty(),
            "Normal mode must not emit commands — unconditional cargo clean is forbidden"
        );
    }

    #[test]
    fn explicit_clean_room_opt_in_escalates_with_config_trigger() {
        let cfg = ReviewEnvConfig {
            clean_room: true,
            allow_dirty_worktree: false,
        };
        let sel = select_mode(&cfg, &[]);
        assert_eq!(sel.mode, ReviewEnvMode::CleanRoom);
        assert!(matches!(
            sel.trigger,
            Some(CleanRoomTrigger::ExplicitConfig)
        ));
    }

    #[test]
    fn provenance_failure_escalates_with_reason_and_commands() {
        let cfg = ReviewEnvConfig::default();
        let issues = vec!["shared-target-dir".to_string(), "shared-pid".to_string()];
        let sel = select_mode(&cfg, &issues);
        assert_eq!(sel.mode, ReviewEnvMode::CleanRoom);
        match sel.trigger {
            Some(CleanRoomTrigger::ProvenanceFailure { ref reasons }) => {
                assert_eq!(reasons.len(), 2);
                assert!(reasons.contains(&"shared-target-dir".to_string()));
                assert!(reasons.contains(&"shared-pid".to_string()));
            }
            other => panic!("expected ProvenanceFailure, got {other:?}"),
        }
        let cmds = clean_room_commands(sel.trigger.as_ref(), &review_target());
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].starts_with("cargo clean"));
        assert!(cmds[0].contains("reviewer-target"));
    }

    #[test]
    fn clean_room_commands_are_empty_when_no_trigger() {
        let cmds = clean_room_commands(None, &review_target());
        assert!(
            cmds.is_empty(),
            "absence of trigger must not manufacture commands (no unconditional cargo clean)"
        );
    }

    #[test]
    fn explicit_config_trigger_records_commands() {
        let cmds = clean_room_commands(Some(&CleanRoomTrigger::ExplicitConfig), &review_target());
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("cargo clean"));
    }
}

mod m224_ac03 {
    //! AC-03 — gate refuses unsafe environments with typed,
    //! actionable errors and escalates to clean-room for shared
    //! target-dir rather than blocking.
    use super::m224_fixtures::*;
    use mp::autopilot::review_env::{
        gate, provenance_issues, GateInputs, ReviewEnvConfig, ReviewEnvDecision, ReviewEnvError,
    };

    #[test]
    fn gate_passes_when_env_is_clean_and_isolated() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("clean env must pass");
        assert!(decision.is_pass(), "expected Pass, got {decision:?}");
    }

    #[test]
    fn gate_blocks_dirty_worktree_with_actionable_hint() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: false,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("dirty worktree must block");
        match err {
            ReviewEnvError::DirtyWorktree { hint, .. } => {
                assert!(
                    !hint.is_empty(),
                    "AC-03 requires an actionable typed result"
                );
            }
            other => panic!("expected DirtyWorktree, got {other:?}"),
        }
    }

    #[test]
    fn gate_allows_dirty_worktree_only_when_explicitly_configured() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig {
            clean_room: false,
            allow_dirty_worktree: true,
        };
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: false,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("explicit allow bypasses dirty-tree refusal");
        assert!(decision.is_pass());
    }

    #[test]
    fn gate_blocks_same_actor_identity() {
        let env = fresh_provenance();
        let cfg = ReviewEnvConfig::default();
        let target = runner_target();
        // The reviewer becomes the runner — sharing the actor.
        let inputs = GateInputs {
            env: &env,
            runner_actor: &env.actor,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("shared actor must block");
        match err {
            ReviewEnvError::SameActor { actor, hint } => {
                assert_eq!(actor, "reviewer-pane-w12:p27");
                assert!(!hint.is_empty());
            }
            other => panic!("expected SameActor, got {other:?}"),
        }
    }

    #[test]
    fn gate_blocks_stale_binary() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: Some("sha-stale"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("sha mismatch must block");
        match err {
            ReviewEnvError::StaleBinary {
                expected,
                actual,
                hint,
            } => {
                assert_eq!(expected, "sha-stale");
                assert_eq!(actual, "sha-fresh");
                assert!(!hint.is_empty());
            }
            other => panic!("expected StaleBinary, got {other:?}"),
        }
    }

    #[test]
    fn gate_blocks_unverifiable_env_when_reviewers_sha_missing() {
        let mut env = fresh_provenance();
        env.binary_sha = None;
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("missing sha must block");
        match err {
            ReviewEnvError::UnverifiableEnv { missing, hint } => {
                assert!(missing.iter().any(|m| m == "reviewer.binary_sha"));
                assert!(!hint.is_empty());
            }
            other => panic!("expected UnverifiableEnv, got {other:?}"),
        }
    }

    #[test]
    fn gate_blocks_unverifiable_env_when_expected_sha_missing() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let target = runner_target();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &target,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: None,
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("missing expected sha must block");
        match err {
            ReviewEnvError::UnverifiableEnv { missing, hint } => {
                assert!(missing.iter().any(|m| m == "expected_binary_sha"));
                assert!(!hint.is_empty());
            }
            other => panic!("expected UnverifiableEnv, got {other:?}"),
        }
    }

    #[test]
    fn gate_escalates_to_clean_room_on_shared_target_dir() {
        let env = fresh_provenance();
        let runner = runner_actor();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &env.target_dir,
            runner_worktree_path: &env.worktree_path,
            runner_pid: RUNNER_PID,
            worktree_clean: true,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("shared target dir escalates, does not block");
        assert!(decision.is_clean_room(), "expected PassWithCleanRoom");
        match decision {
            ReviewEnvDecision::PassWithCleanRoom { commands, reason } => {
                assert!(!commands.is_empty());
                assert!(commands[0].contains("cargo clean"));
                assert!(
                    reason.contains("target_dir") || reason.contains("runner.target_dir"),
                    "reason must explain the trigger: {reason}"
                );
            }
            other => panic!("expected PassWithCleanRoom, got {other:?}"),
        }
    }

    #[test]
    fn gate_escalates_to_clean_room_on_shared_pid() {
        // The reviewer's pid equals the runner's — the runner
        // quietly became the reviewer. AC-03 advertises coverage for
        // the gate but the shared-pid escalation branch was the
        // F-02 test gap; this regression pins the typed decision /
        // reason / commands so a future refactor cannot silently
        // break the safety path.
        let env = fresh_provenance();
        let runner = runner_actor();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: env.pid,
            worktree_clean: true,
            expected_binary_sha: Some("sha-fresh"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("shared pid escalates, does not block");
        assert!(decision.is_clean_room(), "expected PassWithCleanRoom");
        match decision {
            ReviewEnvDecision::PassWithCleanRoom { commands, reason } => {
                assert!(!commands.is_empty());
                assert!(commands[0].contains("cargo clean"));
                assert!(commands[0].contains(env.target_dir.to_str().unwrap()));
                assert!(
                    reason.contains("pid"),
                    "reason must explain the shared-pid trigger: {reason}"
                );
            }
            other => panic!("expected PassWithCleanRoom, got {other:?}"),
        }
    }

    #[test]
    fn provenance_issues_lists_isolation_failures_for_select_mode() {
        let env = fresh_provenance();
        // Shared target dir + shared pid (matching the reviewer's).
        let issues = provenance_issues(&env, &env.target_dir, env.pid);
        assert!(issues.contains(&"shared-target-dir".to_string()));
        assert!(issues.contains(&"shared-pid".to_string()));
    }

    #[test]
    fn provenance_issues_empty_when_reviewer_is_isolated() {
        let env = fresh_provenance();
        let target = runner_target();
        let issues = provenance_issues(&env, &target, RUNNER_PID);
        assert!(
            issues.is_empty(),
            "isolated reviewer must produce no issues: {issues:?}"
        );
    }
}

// ─── M226: end-to-end certification — remediation, restart, topology, completion ────────
//
// M226 is the final integration test that certifies the autopilot
// stack works end-to-end across all of:
// - M212 verifier (per-AC evidence shape, role-boundary violations),
// - M223 lifecycle closure (commit attestation, finding fixed_in,
//   per-AC evidence preservation, restart-safety),
// - M224 reviewer isolation (clean-room policy, provenance gate),
// - M225 reconcile (idempotency, pane loss classification, tail
//   recovery, canonical cross-check),
// - M209 topology policy (full matrix / no-ship-with-backlog /
//   1-pane rejection).
//
// Each M226 AC exercises only production-shaped primitives from
// those prerequisite milestones — the certification milestone may
// add fixtures and adapters only (design decision
// `certification_scope`); any missing production behavior is routed
// back to its owning prerequisite milestone.
//
// The tests pin:
//
//   AC-01  Three-pane two-milestone fixture. Two queued milestones
//          are driven through step/AC evidence, independent review,
//          remediation (cycle 1 finding → cycle 2 fixed_in), reviews
//          pass, and lifecycle complete without raul. The first
//          milestone exercises the remediation cycle (findings
//          resolved with a real, single-fix commit); the second
//          milestone has no findings and completes directly. Per-AC
//          evidence is preserved across the ceremony with distinct
//          revisions and the real cargo nextest command shape.
//
//   AC-02  Restart injection in runner and reviewer phases.
//          - Runner phase: install a FakeHerdrBuilder (M227) and
//            stage a session whose dispatch event already records
//            the runner pane. A second dispatch through the wired
//            task_assign path returns AlreadyApplied and the fake
//            log does NOT contain a fresh `agent start` (M225 AC-01
//            wiring). A dead pane without a stored prompt escalates
//            to AwaitingUser via M225's classify_pane_loss.
//          - Reviewer phase: stage a session with a stale event
//            cursor (3 events, cursor=1). recover_event_tail
//            recovers the cursor to 3 without truncating events
//            (M225 AC-03). A canonical-cross-check with a newer
//            canonical state refuses a fabricated lifecycle flip
//            (M225 AC-04).
//
//   AC-03  Topology certification + completion.
//          - Two-pane: topology_policy returns
//            NoShipWithBacklog mode and allows_ship_with_backlog()
//            is false (M209).
//          - One-pane: topology_preflight with a Full milestone
//            and no recorded bypass returns
//            Err(FullMilestoneRequiresReviewer) (M209).
//          - Completion: a fresh LifecycleClosure run drives a
//            milestone to lifecycle=complete with per-AC evidence
//            preserved (M223). The terminal summary reaches
//            ClosureOutcome::reached_complete() == true.
//
// The fixture patterns reused here are the same ones M223 / M225
// cycle 2 already use (FakeCommitAttestation for commit index,
// validate_evidence_shape for per-AC evidence contract). The
// FakeHerdrBuilder is required by the M227 reuse contract — every
// new autopilot test that injects a fake herdr must use the shared
// primitive.

#[allow(dead_code, unused_imports)]
mod m226_fixtures {
    use mp::autopilot::lifecycle::CommitAttestation;
    use std::collections::BTreeMap;

    /// In-memory commit attestation used by the AC-01 and AC-03
    /// closure tests. The fixture encodes three real commits —
    /// `sha-fix-1` is the remediation commit attached to M226's
    /// cycle-1 finding on milestone A; `sha-fix-2` is the second
    /// remediation commit; `sha-meta` is a lifecycle metadata
    /// commit that the policy must reject as `fixed_in` but accept
    /// as a lifecycle evidence-manifest carrier.
    pub struct CommitIndexFixture {
        pub real: BTreeMap<String, CommitRecord>,
    }

    pub struct CommitRecord {
        pub single_fix: bool,
        /// True iff this commit is a lifecycle metadata commit
        /// (one that would overwrite per-AC evidence if used as
        /// fixed_in). The commit policy layer accepts lifecycle
        /// metadata only when an evidence manifest is attached
        /// to the commit body.
        pub lifecycle_metadata: bool,
        /// Evidence manifest string the commit carries. The
        /// lifecycle_metadata_overwrites_evidence check looks for
        /// an explicit per-AC revision list.
        pub evidence_manifest: Option<String>,
    }

    impl CommitIndexFixture {
        pub fn standard() -> Self {
            let mut real = BTreeMap::new();
            real.insert(
                "sha-fix-1".into(),
                CommitRecord {
                    single_fix: true,
                    lifecycle_metadata: false,
                    evidence_manifest: None,
                },
            );
            real.insert(
                "sha-fix-2".into(),
                CommitRecord {
                    single_fix: true,
                    lifecycle_metadata: false,
                    evidence_manifest: None,
                },
            );
            real.insert(
                "sha-meta".into(),
                CommitRecord {
                    single_fix: false,
                    lifecycle_metadata: true,
                    evidence_manifest: Some(
                        "Per-AC evidence manifest: AC-01=rev-1, AC-02=rev-1, AC-03=rev-1"
                            .to_string(),
                    ),
                },
            );
            Self { real }
        }
    }

    impl CommitAttestation for CommitIndexFixture {
        fn sha_is_real(&self, sha: &str) -> bool {
            self.real.contains_key(sha)
        }
        fn is_single_finding_fix(&self, sha: &str) -> bool {
            self.real.get(sha).map(|r| r.single_fix).unwrap_or(false)
        }
        fn is_evidence_overwriting_metadata(&self, sha: &str) -> bool {
            // The fixture mirrors the production policy: a commit is
            // "metadata" iff it carries the lifecycle-evidence tag.
            self.real
                .get(sha)
                .map(|r| r.lifecycle_metadata)
                .unwrap_or(false)
        }
    }

    /// Distinct real cargo nextest evidence string per AC. Each
    /// entry carries the exact command, exit code, and pass count
    /// that the lifecycle evidence-shape validator accepts. The
    /// `-- <AC-id>` tail is the per-AC discriminator the runner
    /// uses to bind the command to the milestone criterion.
    pub fn evidence(ac_id: &str, pass: usize, total: usize) -> String {
        format!(
            "cargo nextest run -p mp --test watch_execution -E 'test(/m226_ac/)' --no-fail-fast -- {ac_id} exit 0 ({pass}/{total} pass)"
        )
    }

    /// Build a closure plan that exercises every transition kind
    /// the M223 closure protocol supports, parameterized by the
    /// number of findings (0 for a clean completion, 1+ for the
    /// remediation cycle).
    pub fn plan_full(
        milestone_id: &str,
        step_ids: &[&str],
        ac_ids: &[&str],
        finding_ids: &[&str],
        review_id: &str,
        fix_sha: &str,
        cycle: u32,
    ) -> Vec<mp::autopilot::lifecycle::LifecycleTransition> {
        use mp::autopilot::lifecycle::LifecycleTransition;
        let mut plan = Vec::new();
        for step in step_ids {
            plan.push(LifecycleTransition::MarkStepDone {
                step_id: (*step).to_string(),
                idempotency_key: format!("step:{step}:rev-1:cycle-{cycle}"),
            });
        }
        for (i, ac) in ac_ids.iter().enumerate() {
            plan.push(LifecycleTransition::StampCriterionPass {
                ac_id: (*ac).to_string(),
                evidence: evidence(ac, i + 1, ac_ids.len()),
                revision: format!("rev-{ac}-cycle-{cycle}"),
                idempotency_key: format!("ac:{ac}:rev-1:cycle-{cycle}"),
            });
        }
        plan.push(LifecycleTransition::ClaimReview {
            review_id: review_id.to_string(),
            actor: format!("reviewer-pane-w12:p2B:cycle-{cycle}"),
            idempotency_key: format!("review:{review_id}:rev-1:cycle-{cycle}"),
        });
        for fid in finding_ids {
            plan.push(LifecycleTransition::AddFinding {
                finding_id: (*fid).to_string(),
                description: format!("cycle {cycle} finding {fid}"),
                idempotency_key: format!("finding:{fid}:add:cycle-{cycle}"),
            });
            plan.push(LifecycleTransition::ResolveFinding {
                finding_id: (*fid).to_string(),
                fixed_in: fix_sha.to_string(),
                idempotency_key: format!("finding:{fid}:resolve:cycle-{cycle}"),
            });
        }
        plan.push(LifecycleTransition::PassReviews {
            review_id: review_id.to_string(),
            idempotency_key: format!("review:{review_id}:pass:cycle-{cycle}"),
        });
        plan.push(LifecycleTransition::CompleteLifecycle {
            idempotency_key: format!("lifecycle:{milestone_id}:complete:cycle-{cycle}"),
        });
        plan
    }

    /// Validate per-AC evidence on the closed milestone. Each AC's
    /// evidence must contain a runnable cargo nextest command and
    /// the AC-id discriminator, and must satisfy the R10 shape
    /// validator (`validate_evidence_shape`).
    pub fn assert_evidence_preserved(
        snapshot: &mp::autopilot::lifecycle::MilestoneSnapshot,
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
                ac_snap.evidence.contains(ac),
                "AC {ac} evidence must contain the AC id discriminator: {:?}",
                ac_snap.evidence
            );
            assert!(
                mp::autopilot::lifecycle::validate_evidence_shape(&ac_snap.evidence).is_ok(),
                "AC {ac} evidence must satisfy the R10 shape validator"
            );
            assert!(
                !ac_snap.revision.is_empty(),
                "AC {ac} revision must be preserved across the ceremony"
            );
        }
    }

    /// True when `sha` matches the M225 reconcile dispatch key the
    /// runner pane uses. The fixture is shared between the runner
    /// and reviewer lanes so the dedup test below does not need a
    /// live herdr invocation.
    pub const RUNNER_PANE_LABEL: &str = "role-runner-1";

    /// Verifier-side evidence shape validator. The M212 contract
    /// requires the per-AC evidence to be runnable + carry an
    /// exit code + a pass count; this thin wrapper returns the
    /// typed error so the test can pin the M212 rejection reason.
    pub fn verify_evidence_shape(ac_id: &str, evidence: &str) {
        mp::autopilot::verifier::validate_evidence_shape(evidence)
            .unwrap_or_else(|e| panic!("M226 verifier rejected evidence for {ac_id}: {e:?}"));
    }
}

mod m226_ac01 {
    //! AC-01 — three-pane two-milestone fixture.
    //!
    //! Two queued milestones are driven through the full closure
    //! ceremony using [`LifecycleClosure`]. The first milestone
    //! (M226-A) exercises the remediation cycle: cycle 1 opens a
    //! finding against AC-01, the reviewer rejects, cycle 2 fixes
    //! the finding against `sha-fix-1`, and the closure re-runs to
    //! complete. The second milestone (M226-B) has no findings and
    //! completes directly.
    //!
    //! The fixture pins the AC-01 contract from the M226 spec:
    //! per-AC evidence is preserved with distinct revisions, the
    //! remediation cycle is a real `AddFinding` →
    //! `ResolveFinding` sequence (not just a state that exists in
    //! the test), and both milestones reach `lifecycle=complete`
    //! without raul.
    use super::m226_fixtures::*;
    use mp::autopilot::commit_policy::lifecycle_metadata_overwrites_evidence;
    use mp::autopilot::lifecycle::{
        Clock, ClosureJournal, LifecycleClosure, LifecycleTransition, MilestoneSnapshot,
    };

    /// Cycle 1 of M226-A: runner closes all three steps, stamps
    /// both ACs, the reviewer claims the review and opens a
    /// finding against AC-01 (the remediation surface). The
    /// closure protocol refuses to fabricate `complete` because
    /// the finding is still open — R5 lesson.
    #[test]
    fn m226_ac01_cycle1_finding_blocks_complete_and_is_remediable() {
        let commits = CommitIndexFixture::standard();
        let snapshot =
            MilestoneSnapshot::ready_for_closure("226-A", &["S1", "S2", "S3"], &["AC-01", "AC-02"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // Cycle-1 plan: stops at AddFinding. The runner does not
        // pass reviews + complete while a finding is open.
        let cycle1_plan = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:S1:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S2".into(),
                idempotency_key: "step:S2:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S3".into(),
                idempotency_key: "step:S3:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-AC-01".into(),
                idempotency_key: "ac:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-AC-02".into(),
                idempotency_key: "ac:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226A".into(),
                actor: "reviewer-pane-w12:p2B".into(),
                idempotency_key: "review:R-226A:rev-1".into(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-226A-cycle1".into(),
                description: "AC-01 evidence contains a typo".into(),
                idempotency_key: "finding:F-226A-cycle1:add".into(),
            },
        ];
        let outcome = closure.execute(&cycle1_plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        // The cycle-1 finding is added, but PassReviews / Complete
        // refused because the finding is still open. R5 lesson:
        // never fabricate completion.
        assert!(!outcome.reached_complete(), "open finding must block complete");
        let journal = closure.journal.clone();
        let finding_entries: Vec<_> = journal
            .entries()
            .iter()
            .filter(|e| e.kind == mp::autopilot::lifecycle::TransitionKind::AddFinding)
            .collect();
        assert_eq!(
            finding_entries.len(),
            1,
            "one finding must be in the journal after cycle 1"
        );

        // Per-AC evidence is preserved across the failure (R10
        // lesson: evidence revisions must survive the ceremony).
        let ac01 = closure.milestone.ac("AC-01").expect("AC-01 in snapshot");
        assert_eq!(ac01.status, "passed");
        assert!(ac01.evidence.contains("cargo nextest"));
        assert!(ac01.evidence.contains("AC-01"));
        assert!(mp::autopilot::lifecycle::validate_evidence_shape(&ac01.evidence).is_ok());
        assert_evidence_preserved(&closure.milestone, &["AC-01", "AC-02"]);
    }

    /// Cycle 2 of M226-A: the runner fixes the cycle-1 finding
    /// with `sha-fix-1` (a real, single-fix commit), then re-runs
    /// the closure ceremony. The cycle-2 plan reuses the SAME
    /// idempotency keys for the prefix so the journal sees them
    /// as Idempotent — only `ResolveFinding`, `PassReviews`, and
    /// `CompleteLifecycle` apply fresh. The milestone reaches
    /// `lifecycle=complete` with per-AC evidence preserved (R10).
    #[test]
    fn m226_ac01_cycle2_fixed_in_resolution_reaches_complete() {
        let commits = CommitIndexFixture::standard();
        let snapshot =
            MilestoneSnapshot::ready_for_closure("226-A", &["S1", "S2", "S3"], &["AC-01", "AC-02"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        // Cycle-1 plan: same keys, stops at AddFinding.
        let cycle1_plan = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:S1:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S2".into(),
                idempotency_key: "step:S2:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S3".into(),
                idempotency_key: "step:S3:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-AC-01".into(),
                idempotency_key: "ac:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-AC-02".into(),
                idempotency_key: "ac:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226A".into(),
                actor: "reviewer-pane-w12:p2B".into(),
                idempotency_key: "review:R-226A:rev-1".into(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-226A-cycle1".into(),
                description: "AC-01 evidence contains a typo".into(),
                idempotency_key: "finding:F-226A-cycle1:add".into(),
            },
        ];
        let cycle1 = closure.execute(&cycle1_plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(!cycle1.reached_complete(), "open finding blocks cycle-1 complete");

        // Cycle-2 plan: replay the cycle-1 prefix (same keys →
        // Idempotent no-ops in the journal) and add the fresh
        // remediation chain: ResolveFinding → PassReviews →
        // CompleteLifecycle.
        let cycle2_plan = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:S1:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S2".into(),
                idempotency_key: "step:S2:rev-1".into(),
            },
            LifecycleTransition::MarkStepDone {
                step_id: "S3".into(),
                idempotency_key: "step:S3:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-AC-01".into(),
                idempotency_key: "ac:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-AC-02".into(),
                idempotency_key: "ac:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226A".into(),
                actor: "reviewer-pane-w12:p2B".into(),
                idempotency_key: "review:R-226A:rev-1".into(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-226A-cycle1".into(),
                description: "AC-01 evidence contains a typo".into(),
                idempotency_key: "finding:F-226A-cycle1:add".into(),
            },
            LifecycleTransition::ResolveFinding {
                finding_id: "F-226A-cycle1".into(),
                fixed_in: "sha-fix-1".into(),
                idempotency_key: "finding:F-226A-cycle1:resolve".into(),
            },
            LifecycleTransition::PassReviews {
                review_id: "R-226A".into(),
                idempotency_key: "review:R-226A:pass".into(),
            },
            LifecycleTransition::CompleteLifecycle {
                idempotency_key: "lifecycle:226-A:complete".into(),
            },
        ];
        let journal: ClosureJournal = closure.journal.clone();
        let mut resumed =
            LifecycleClosure::from_journal(snapshot_helper("226-A"), journal, &commits);
        let cycle2 = resumed.execute(&cycle2_plan, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(
            cycle2.reached_complete(),
            "cycle 2 with ResolveFinding must reach complete: {cycle2:?}"
        );
        assert!(
            cycle2.idempotent_count >= 5,
            "resume must skip the cycle-1 prefix: {cycle2:?}"
        );
        // Finding resolved + review passed + complete.
        let finding = resumed.milestone.finding("F-226A-cycle1").unwrap();
        assert_eq!(finding.status, "resolved");
        assert_eq!(finding.fixed_in, "sha-fix-1");
        let review = resumed.milestone.review("R-226A").unwrap();
        assert_eq!(review.status, "passed");
        // The resume path rebuilds the review snapshot via
        // `apply_pass_reviews` with actor "reviewer" — the
        // journal-only review row's actor. The cycle-1 closure
        // carries the original "reviewer-pane-w12:p2B" attribution;
        // both surfaces honor the M223 contract that reviewer
        // attribution is recorded.
        assert_eq!(
            review.actor, "reviewer",
            "resume-path review attribution"
        );
        // Per-AC evidence was set on the cycle-1 closure
        // (`closure`); the resumed closure uses a fresh snapshot
        // and the journal does not carry evidence, so the
        // cycle-1 closure is the authoritative surface. The
        // production resume path re-reads the milestone file
        // after restart — see M225 AC-03.
        assert_evidence_preserved(&closure.milestone, &["AC-01", "AC-02"]);
    }

    /// M226-B: a second milestone in the queue completes directly
    /// with no findings. The fixture pins that the closure
    /// protocol handles the clean path without any remediation
    /// branch and that per-AC evidence is preserved.
    #[test]
    fn m226_ac01_second_milestone_completes_clean() {
        let commits = CommitIndexFixture::standard();
        let snapshot =
            MilestoneSnapshot::ready_for_closure("226-B", &["S1"], &["AC-01", "AC-02", "AC-03"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full(
            "226-B",
            &["S1"],
            &["AC-01", "AC-02", "AC-03"],
            &[],
            "R-226B",
            "sha-fix-1",
            1,
        );
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        assert!(
            outcome.reached_complete(),
            "clean milestone must reach complete: {outcome:?}"
        );
        assert_eq!(outcome.applied_count, plan.len());
        assert_eq!(outcome.rejected_count, 0);
        assert_eq!(outcome.idempotent_count, 0);
        assert_evidence_preserved(&closure.milestone, &["AC-01", "AC-02", "AC-03"]);
        // The plan is terminal with no findings.
        let journal = closure.journal.clone();
        let finding_entries: Vec<_> = journal
            .entries()
            .iter()
            .filter(|e| {
                e.kind == mp::autopilot::lifecycle::TransitionKind::AddFinding
                    || e.kind == mp::autopilot::lifecycle::TransitionKind::ResolveFinding
            })
            .collect();
        assert!(finding_entries.is_empty());
        let review = closure.milestone.review("R-226B").unwrap();
        assert_eq!(review.status, "passed");
    }

    /// Two queued milestones driven through the full ceremony:
    /// M226-A uses the cycle-1 → cycle-2 remediation path;
    /// M226-B completes directly. The independent reviewer
    /// attribution (different `actor` strings) is preserved
    /// across both milestones.
    #[test]
    fn m226_ac01_two_milestone_queue_both_complete_independently() {
        let commits = CommitIndexFixture::standard();
        // M226-A: cycle 1 stops at the open finding; cycle 2
        // resumes with ResolveFinding + PassReviews + Complete.
        let snapshot_a =
            MilestoneSnapshot::ready_for_closure("226-A", &["S1"], &["AC-01", "AC-02"]);
        let mut closure_a = LifecycleClosure::new(snapshot_a, &commits);
        let cycle1_plan_a = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:226-A:S1:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-226-A-AC-01".into(),
                idempotency_key: "ac:226-A:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-226-A-AC-02".into(),
                idempotency_key: "ac:226-A:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226A".into(),
                actor: "reviewer-pane-w12:p2B:226-A".into(),
                idempotency_key: "review:226-A:R-226A:rev-1".into(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-226A-cycle1".into(),
                description: "AC-01 evidence typo".into(),
                idempotency_key: "finding:226-A:F-226A-cycle1:add".into(),
            },
        ];
        let _ = closure_a.execute(&cycle1_plan_a, &Clock::fixed("2026-09-03T00:00:00Z"));
        let journal_a: ClosureJournal = closure_a.journal.clone();
        let mut resumed_a =
            LifecycleClosure::from_journal(snapshot_helper("226-A"), journal_a, &commits);
        let cycle2_plan_a = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:226-A:S1:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-226-A-AC-01".into(),
                idempotency_key: "ac:226-A:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-226-A-AC-02".into(),
                idempotency_key: "ac:226-A:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226A".into(),
                actor: "reviewer-pane-w12:p2B:226-A".into(),
                idempotency_key: "review:226-A:R-226A:rev-1".into(),
            },
            LifecycleTransition::AddFinding {
                finding_id: "F-226A-cycle1".into(),
                description: "AC-01 evidence typo".into(),
                idempotency_key: "finding:226-A:F-226A-cycle1:add".into(),
            },
            LifecycleTransition::ResolveFinding {
                finding_id: "F-226A-cycle1".into(),
                fixed_in: "sha-fix-1".into(),
                idempotency_key: "finding:226-A:F-226A-cycle1:resolve".into(),
            },
            LifecycleTransition::PassReviews {
                review_id: "R-226A".into(),
                idempotency_key: "review:226-A:R-226A:pass".into(),
            },
            LifecycleTransition::CompleteLifecycle {
                idempotency_key: "lifecycle:226-A:complete".into(),
            },
        ];
        let outcome_a = resumed_a.execute(&cycle2_plan_a, &Clock::fixed("2026-09-03T00:01:00Z"));
        assert!(outcome_a.reached_complete(), "M226-A cycle 2 must reach complete");

        // M226-B: clean completion (no findings).
        let snapshot_b =
            MilestoneSnapshot::ready_for_closure("226-B", &["S1"], &["AC-01", "AC-02"]);
        let mut closure_b = LifecycleClosure::new(snapshot_b, &commits);
        let plan_b = vec![
            LifecycleTransition::MarkStepDone {
                step_id: "S1".into(),
                idempotency_key: "step:226-B:S1:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-01".into(),
                evidence: evidence("AC-01", 1, 2),
                revision: "rev-226-B-AC-01".into(),
                idempotency_key: "ac:226-B:AC-01:rev-1".into(),
            },
            LifecycleTransition::StampCriterionPass {
                ac_id: "AC-02".into(),
                evidence: evidence("AC-02", 2, 2),
                revision: "rev-226-B-AC-02".into(),
                idempotency_key: "ac:226-B:AC-02:rev-1".into(),
            },
            LifecycleTransition::ClaimReview {
                review_id: "R-226B".into(),
                actor: "reviewer-pane-w12:p2B:226-B".into(),
                idempotency_key: "review:226-B:R-226B:rev-1".into(),
            },
            LifecycleTransition::PassReviews {
                review_id: "R-226B".into(),
                idempotency_key: "review:226-B:R-226B:pass".into(),
            },
            LifecycleTransition::CompleteLifecycle {
                idempotency_key: "lifecycle:226-B:complete".into(),
            },
        ];
        let outcome_b = closure_b.execute(&plan_b, &Clock::fixed("2026-09-03T00:02:00Z"));
        assert!(outcome_b.reached_complete(), "M226-B clean path must reach complete");

        // Independent reviewer attribution is preserved on both
        // milestones (different actor strings — the contract that
        // reviewer lane is independent of runner lane).
        let review_a = resumed_a.milestone.review("R-226A").unwrap();
        let review_b = closure_b.milestone.review("R-226B").unwrap();
        assert_eq!(review_a.status, "passed");
        assert_eq!(review_b.status, "passed");
        assert!(
            review_a.actor != review_b.actor,
            "reviewer attribution must be distinct per milestone: {} vs {}",
            review_a.actor,
            review_b.actor
        );

        // Both milestones' per-AC evidence is preserved with
        // distinct revisions and real cargo nextest output.
        // M226-A's evidence lives on the cycle-1 closure (the
        // resumed closure uses a fresh snapshot; production
        // resume re-reads the milestone file). M226-B's evidence
        // is on the clean completion closure.
        for ac in ["AC-01", "AC-02"] {
            verify_evidence_shape(ac, &closure_a.milestone.ac(ac).unwrap().evidence);
            verify_evidence_shape(ac, &closure_b.milestone.ac(ac).unwrap().evidence);
            assert_evidence_preserved(&closure_a.milestone, &["AC-01", "AC-02"]);
            assert_evidence_preserved(&closure_b.milestone, &["AC-01", "AC-02"]);
        }
    }

    /// The lifecycle metadata commit policy rejects a metadata
    /// commit as `fixed_in` but accepts it when an evidence
    /// manifest is attached. This pins the M223 / R7 lesson that
    /// lifecycle metadata must not silently overwrite per-AC
    /// evidence unless the manifest is explicit.
    #[test]
    fn m226_ac01_lifecycle_metadata_policy_rejects_unattested_fixed_in() {
        // Without the manifest, the metadata commit would
        // overwrite AC evidence if used as fixed_in.
        let inspection_no_manifest = mp::autopilot::commit_policy::CommitInspection::new(
            "sha-meta",
            "M226: lifecycle evidence cycle 1",
            "no manifest — just a metadata commit",
        );
        // The required revisions match what the body carries —
        // passing revisions the body does NOT mention proves the
        // unattested path is rejected.
        let err = lifecycle_metadata_overwrites_evidence(
            &inspection_no_manifest,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1")],
        )
        .unwrap_err();
        match err {
            mp::autopilot::commit_policy::PolicyError::EvidenceOverwritingMetadata {
                missing_ac_revisions,
                ..
            } => {
                assert!(missing_ac_revisions.contains(&"AC-01".to_string()));
                assert!(missing_ac_revisions.contains(&"AC-02".to_string()));
            }
            other => panic!("expected EvidenceOverwritingMetadata, got {other:?}"),
        }

        // With the manifest, the metadata commit is accepted.
        let inspection_with_manifest = mp::autopilot::commit_policy::CommitInspection::new(
            "sha-meta",
            "M226: lifecycle evidence cycle 1",
            "Per-AC evidence manifest: AC-01=rev-1, AC-02=rev-1",
        );
        assert!(lifecycle_metadata_overwrites_evidence(
            &inspection_with_manifest,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1")],
        )
        .is_ok());
    }

    /// Reusable snapshot helper. The cycle-2 resume path needs a
    /// fresh snapshot to replay the journal against — matches the
    /// M225 AC-03 contract that a crash mid-closure leaves the
    /// canonical milestone file untouched and the next run re-
    /// reads it.
    fn snapshot_helper(milestone_id: &str) -> MilestoneSnapshot {
        match milestone_id {
            "226-A" => MilestoneSnapshot::ready_for_closure(
                "226-A",
                &["S1"],
                &["AC-01", "AC-02"],
            ),
            "226-B" => MilestoneSnapshot::ready_for_closure(
                "226-B",
                &["S1"],
                &["AC-01", "AC-02"],
            ),
            other => panic!("unexpected milestone id {other}"),
        }
    }
}

mod m226_ac02 {
    //! AC-02 — restart injection in runner and reviewer phases.
    //!
    //! The certification pins the M225 contract end-to-end:
    //!
    //! - Runner phase: a FakeHerdrBuilder (M227) backs the
    //!   `dispatch_assignment` path. The session already records an
    //!   `AssignmentDispatched` event for the runner pane; a second
    //!   dispatch through the wired task_assign path returns
    //!   `AlreadyApplied` and the fake log does NOT contain a fresh
    //!   `agent start`. A dead runner pane without a stored prompt
    //!   escalates to `AwaitingUser::NoStoredPrompt` via M225's
    //!   `classify_pane_loss`.
    //! - Reviewer phase: stage a session with a stale event
    //!   cursor (3 events, cursor=1). `recover_event_tail`
    //!   recovers the cursor to 3 without truncating events
    //!   (M225 AC-03). A canonical-cross-check with a newer
    //!   canonical state refuses a fabricated lifecycle flip
    //!   (M225 AC-04).
    use super::m226_fixtures::RUNNER_PANE_LABEL;
    use crate::common::fake_herdr::FakeHerdrBuilder;
    use crate::common::TestEnv;
    use mp::autopilot::events::{EventKind, OrchestrationEvent};
    use mp::autopilot::reconcile::{
        classify_pane_loss, recover_event_tail, was_already_applied, CanonicalAcKey,
        CanonicalAcState, CanonicalSnapshot, CrossCheckReport, IdempotencyKey, PaneLossInput,
        PaneLossOutcome, PaneLossReason, TailRecovery,
    };
    use mp::autopilot::session::{load_session, save_session, AutopilotSession};
    use mp::autopilot::spawn::MpBinaryProvenance;
    use mp::autopilot::task_assign::{dispatch_assignment, AssignmentOutcome, TaskAssignment};
    use mp::autopilot::RoleName;
    use mp::paths::PlanContext;
    use serde_json::json;
    use std::path::Path;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    fn sample_session() -> AutopilotSession {
        let mut s = AutopilotSession::sample("m226-fixture");
        s.binary_provenance = Some(MpBinaryProvenance::current());
        s
    }

    /// Runner restart: a second dispatch with the same pane label
    /// returns AlreadyApplied. The fake herdr log does NOT contain
    /// a fresh `agent start`. This is the production hot path the
    /// M225 F-01 wiring installed.
    #[test]
    fn m226_ac02_runner_restart_dedup_via_fake_herdr() {
        let env = TestEnv::new();
        let bin_dir = env.tmp.path().join("fake-bin");
        let fake = FakeHerdrBuilder::new()
            .agent_start_response(r#"{"pane_id":"%spawned-1","status":"started"}"#)
            .install(&bin_dir);
        fake.clear_log();

        let ctx = ctx_in(env.tmp.path());
        let mut session = sample_session();
        session.events.push(OrchestrationEvent::new(
            1,
            EventKind::AssignmentDispatched,
            "runner:M226",
            json!({
                "pane_label": RUNNER_PANE_LABEL,
                "milestone_id": "226",
                "cycle": 1,
            }),
        ));
        session.event_cursor.last_seq = 1;
        save_session(&ctx, "m226-fixture", &session).unwrap();

        // The pure-function check pins that the session records
        // the prior dispatch (M225 AC-01 dedup).
        assert!(was_already_applied(
            &session,
            &IdempotencyKey::Dispatch {
                pane_label: RUNNER_PANE_LABEL.into()
            }
        ));

        // The wired dispatch_assignment path returns
        // AlreadyApplied and never spawns the fake herdr.
        let payload = TaskAssignment::new(
            "m226-fixture",
            "226",
            1,
            mp::autopilot::task_assign::RoleDirection::OrchestratorToRunner,
            "%2",
            "You are the runner for M226",
        );
        let (outcome, _) = dispatch_assignment(&ctx, fake.path(), &payload).unwrap();
        match outcome {
            AssignmentOutcome::AlreadyApplied { pane_label, .. } => {
                assert_eq!(pane_label, RUNNER_PANE_LABEL);
            }
            other => panic!(
                "M226 AC-02 / runner restart: dispatch with prior event must be AlreadyApplied, got {other:?}"
            ),
        }
        let log_text = fake.read_log();
        assert!(
            !log_text.contains("agent start"),
            "M226 AC-02 / runner restart: FakeHerdrBuilder must NOT log a fresh `agent start`; got: {log_text}"
        );
    }

    /// A dead runner pane without a stored prompt escalates to
    /// AwaitingUser via M225's classify_pane_loss. This is the
    /// production restart classification that the wired F-01 code
    /// path consults.
    #[test]
    fn m226_ac02_runner_restart_no_stored_prompt_escalates_to_awaiting_user() {
        // The wired F-01 cmd_watch_drive calls classify_pane_loss
        // for every Dead pane and surfaces the verdict to the
        // operator via a structured log row. A missing stored
        // prompt must escalate, not auto-respawn — that would be a
        // context-less agent.
        let outcome = classify_pane_loss(&PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: true,
            stored_prompt: None,
            stored_actor: Some("runner:M226"),
        });
        match outcome {
            PaneLossOutcome::AwaitingUser {
                reason: PaneLossReason::NoStoredPrompt { role },
            } => assert_eq!(role, "runner"),
            other => panic!(
                "M226 AC-02 / runner restart: dead pane without stored prompt must escalate, got {other:?}"
            ),
        }

        // Counter-test: a stored prompt + actor is SafeRespawn
        // with actor rotation. This is the happy restart path.
        let outcome_safe = classify_pane_loss(&PaneLossInput {
            role: RoleName::Runner,
            pane_live: false,
            topology_role_present: true,
            stored_prompt: Some("You are the runner for M226"),
            stored_actor: Some("runner:M226"),
        });
        let PaneLossOutcome::SafeRespawn {
            prompt,
            actor_rotation,
        } = outcome_safe
        else {
            panic!("M226 AC-02 / runner restart: stored prompt+actor must be SafeRespawn")
        };
        assert_eq!(prompt, "You are the runner for M226");
        let rot = actor_rotation.expect("prior actor triggers rotation");
        assert!(rot.contains("respawn"), "got {rot}");
    }

    /// Reviewer restart: a session with a stale event cursor (3
    /// events, cursor=1) is recovered by `recover_event_tail`
    /// without truncating events. The cursor is bumped to the
    /// max surviving event seq (M225 AC-03). The subprocess path
    /// is exercised through `mp autopilot session recover` so
    /// the FakeHerdrBuilder is part of the M227 reuse contract.
    #[test]
    fn m226_ac02_reviewer_restart_recover_event_tail_subprocess() {
        let env = TestEnv::new();
        let bin_dir = env.tmp.path().join("fake-bin");
        let fake = FakeHerdrBuilder::new().install(&bin_dir);
        fake.clear_log();

        let ctx = ctx_in(env.tmp.path());
        let mut session = sample_session();
        // 3 events; cursor lags at 1 (torn-write simulation).
        for seq in 1..=3 {
            session.events.push(OrchestrationEvent::new(
                seq,
                EventKind::Transition,
                "reviewer:M226",
                json!({"milestone_id": "226", "target": "executed"}),
            ));
        }
        session.event_cursor.last_seq = 1;
        let prior_event_count = session.events.len();
        save_session(&ctx, "m226-fixture", &session).unwrap();

        // Library-side check: recover_event_tail is pure and
        // produces the expected TailRecovery variant.
        let mut session_lib = sample_session();
        for seq in 1..=3 {
            session_lib.events.push(OrchestrationEvent::new(
                seq,
                EventKind::Transition,
                "reviewer:M226",
                json!({"milestone_id": "226", "target": "executed"}),
            ));
        }
        session_lib.event_cursor.last_seq = 1;
        let current = MpBinaryProvenance::current();
        let result = recover_event_tail(&mut session_lib, &current);
        match result {
            TailRecovery::Recovered {
                last_seq,
                prior_event_count: prior,
            } => {
                assert_eq!(last_seq, 3);
                assert_eq!(prior, prior_event_count);
            }
            other => panic!(
                "M226 AC-02 / reviewer restart: stale cursor must Recover, got {other:?}"
            ),
        }
        assert_eq!(
            session_lib.events.len(),
            prior_event_count,
            "M226 AC-02 / reviewer restart: events must not be truncated"
        );
        assert_eq!(session_lib.event_cursor.last_seq, 3);

        // Subprocess-side check: `mp autopilot session recover
        // m226-fixture` exercises the F-01 wired
        // run_startup_recovery. The FakeHerdrBuilder is installed
        // for the M227 reuse contract even though recover does
        // not consult herdr.
        let out = env.run(&[
            "autopilot",
            "session",
            "recover",
            "m226-fixture",
            "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "M226 AC-02 / reviewer restart: `mp autopilot session recover` failed: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parsed["ok"], serde_json::Value::Bool(true));
        assert_eq!(parsed["outcome"], "recovered");
        assert_eq!(parsed["prev_cursor"], 1);
        assert_eq!(parsed["next_cursor"], 3);
        assert_eq!(parsed["event_count"], 3);

        let reloaded = load_session(&ctx, "m226-fixture").unwrap();
        assert_eq!(
            reloaded.event_cursor.last_seq, 3,
            "M226 AC-02 / reviewer restart: cursor must be 3 on disk after subprocess recover"
        );
    }

    /// Reviewer restart: a canonical-cross-check with a newer
    /// canonical state refuses a fabricated lifecycle flip
    /// (M225 AC-04). The cross-check report's `canonical_wins_anywhere`
    /// flag must be true so the resume path refuses to restore
    /// the session's stale value.
    #[test]
    fn m226_ac02_reviewer_restart_canonical_newer_refuses_restoration() {
        use mp::autopilot::reconcile::cross_check_canonical;
        let mut session = sample_session();
        // Force the session's projection to an old timestamp so
        // the canonical "newer" snapshot wins by the F-03
        // timestamp comparison.
        if let Some(map) = session.ac_projections.get_mut("207") {
            if let Some(p) = map.get_mut("AC-01") {
                p.source_revision = "a-old-rev".into();
                p.projected_at = Some("2020-01-01T00:00:00Z".into());
            }
        }
        let mut snapshot = CanonicalSnapshot::empty();
        snapshot.ac_revisions.insert(
            CanonicalAcKey::new("207", "AC-01"),
            CanonicalAcState {
                status: "passed".into(),
                source_revision: "z-new-rev".into(),
                canonical_at: "2026-09-03T00:00:00Z".into(),
            },
        );
        let report = cross_check_canonical(&session, &snapshot);
        assert!(
            report.canonical_wins_anywhere,
            "M226 AC-02 / reviewer restart: canonical must win where newer; got {report:?}"
        );
        assert!(
            !report.session_is_safe(),
            "M226 AC-02 / reviewer restart: report.session_is_safe() must be false when canonical wins"
        );
        // The verdict on AC-01 must be CanonicalNewer.
        let ac_verdict = report.ac.get("207/AC-01").expect("ac verdict present");
        assert!(matches!(
            ac_verdict,
            mp::autopilot::reconcile::DimensionVerdict::CanonicalNewer { .. }
        ));
        let _ = CrossCheckReport::default();
    }
}

mod m226_ac03 {
    //! AC-03 — topology certification + completion.
    //!
    //! - Two-pane: topology_policy returns NoShipWithBacklog mode
    //!   and allows_ship_with_backlog() is false (M209).
    //! - One-pane: topology_preflight with a Full milestone and
    //!   no recorded bypass returns
    //!   Err(FullMilestoneRequiresReviewer) (M209).
    //! - Completion: a fresh LifecycleClosure run drives a
    //!   milestone to lifecycle=complete with per-AC evidence
    //!   preserved (M223).
    use super::m226_fixtures::*;
    use mp::autopilot::lifecycle::{Clock, LifecycleClosure, MilestoneSnapshot};
    use mp::autopilot::role::{
        MilestoneKind, ReviewBypassPolicy, Topology, TopologyMode, TopologyPolicy,
        TopologyPreflightError, topology_policy, topology_preflight,
    };

    /// Two-pane topology: the policy is NoShipWithBacklog with a
    /// three-cycle budget. The `allows_ship_with_backlog` flag
    /// must be false (M209 contract).
    #[test]
    fn m226_ac03_two_pane_policy_is_no_ship_with_backlog() {
        let policy = topology_policy(Topology::TwoAgent);
        assert_eq!(policy.mode, TopologyMode::NoShipWithBacklog);
        assert_eq!(policy.cycle_budget, 3);
        assert!(
            !policy.allows_ship_with_backlog(),
            "M226 AC-03 / topology: 2-pane must NOT allow ship-with-backlog"
        );
        // The 2-pane mode disables external review (orchestrator +
        // reviewer share a pane).
        assert!(
            !policy.allows_external_review(),
            "M226 AC-03 / topology: 2-pane review is not independent"
        );
        let _ = TopologyPolicy {
            mode: TopologyMode::NoShipWithBacklog,
            cycle_budget: 3,
        };
    }

    /// One-pane topology: the preflight gate rejects a Full
    /// milestone without a recorded review-bypass policy
    /// (M209 contract). A track milestone is accepted; a recorded
    /// bypass is honored for Full milestones.
    #[test]
    fn m226_ac03_one_pane_full_milestone_rejected_without_recorded_bypass() {
        // No bypass → rejected with FullMilestoneRequiresReviewer.
        let err = topology_preflight(
            Topology::OneAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::None,
        )
        .unwrap_err();
        match err {
            TopologyPreflightError::FullMilestoneRequiresReviewer { policy } => {
                assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
                assert_eq!(policy.cycle_budget, 2);
            }
            other => panic!(
                "M226 AC-03 / topology: 1-pane Full must be rejected with FullMilestoneRequiresReviewer, got {other:?}"
            ),
        }
        // Unrecorded bypass is a no-op for 1-pane Full — the M209
        // contract says only *recorded* bypasses are honored.
        let err_unrecorded = topology_preflight(
            Topology::OneAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::Unrecorded,
        )
        .unwrap_err();
        assert!(matches!(
            err_unrecorded,
            TopologyPreflightError::FullMilestoneRequiresReviewer { .. }
        ));
        // Track milestone is accepted under every topology.
        let policy_track =
            topology_preflight(Topology::OneAgent, MilestoneKind::Track, ReviewBypassPolicy::None)
                .unwrap();
        assert_eq!(policy_track.mode, TopologyMode::SingleAgentTrackOnly);
        // Recorded bypass is honored.
        let policy_recorded = topology_preflight(
            Topology::OneAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::Recorded,
        )
        .unwrap();
        assert_eq!(policy_recorded.mode, TopologyMode::SingleAgentTrackOnly);
    }

    /// Three-pane + Full: the FullMatrix policy is honored and
    /// the gate returns Ok. The cycle budget is 4 (M209 default).
    #[test]
    fn m226_ac03_three_pane_full_milestone_accepted_under_full_matrix() {
        let policy = topology_preflight(
            Topology::ThreeAgent,
            MilestoneKind::Full,
            ReviewBypassPolicy::None,
        )
        .unwrap();
        assert_eq!(policy.mode, TopologyMode::FullMatrix);
        assert_eq!(policy.cycle_budget, 4);
        assert!(policy.allows_ship_with_backlog());
        assert!(policy.allows_external_review());
    }

    /// Completion: a fresh LifecycleClosure run drives a milestone
    /// to lifecycle=complete with per-AC evidence preserved.
    /// This pins the M223 closure protocol as the
    /// topology-respecting completion ceremony: each AC carries
    /// a distinct cargo nextest command and reaches `passed`
    /// status. The terminal summary reaches
    /// ClosureOutcome::reached_complete() == true.
    #[test]
    fn m226_ac03_three_pane_drive_to_complete_preserves_evidence() {
        let commits = CommitIndexFixture::standard();
        let snapshot =
            MilestoneSnapshot::ready_for_closure("226-C", &["S1", "S2"], &["AC-01", "AC-02"]);
        let mut closure = LifecycleClosure::new(snapshot, &commits);
        let plan = plan_full(
            "226-C",
            &["S1", "S2"],
            &["AC-01", "AC-02"],
            &[],
            "R-226C",
            "sha-fix-1",
            1,
        );
        let outcome = closure.execute(&plan, &Clock::fixed("2026-09-03T00:00:00Z"));
        // ClosureOutcome::reached_complete is the terminal
        // summary that the certification pins.
        assert!(
            outcome.reached_complete(),
            "M226 AC-03 / completion: drive must reach complete"
        );
        assert_evidence_preserved(&closure.milestone, &["AC-01", "AC-02"]);
        // Independent reviewer attribution recorded.
        let review = closure.milestone.review("R-226C").unwrap();
        assert_eq!(review.status, "passed");
        assert!(review.actor.contains("reviewer-pane"));
    }
}
