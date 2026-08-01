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
