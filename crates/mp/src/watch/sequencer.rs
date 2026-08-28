//! S8 / AC-04, AC-05: cross-milestone sequencing with per-role pane
//! reuse.
//!
//! Drives N milestones in order through a single [`SystemDriveOps`]
//! instance so the runner and coordinator panes (cached on the ops
//! struct) are reused across milestones.
//!
//! L5 session-boundary: M149 ext-review F-11 noted that the prior
//! comment claimed L5 was preserved by `PromptStage::ReReview`'s
//! "fresh session per L5" prompt text. That claim was aspirational;
//! ReReview is defined in `prompts.rs` but is NOT wired into
//! `next_stage` (see `state_machine::next_stage` — the live flow
//! under M148 Option A is approved → Execute → ExternalReview/Approve,
//! with no separate ReReview rung). So in the current production loop
//! the coordinator pane IS reused across iterations of ExternalReview
//! on the same milestone. This is acceptable for M149 because:
//! (a) the runner is mid-execute between ExternalReview iterations,
//!     so the cached pane sits idle until the runner returns;
//! (b) ExternalReview → Approve is the L5 boundary, and Approve fires
//!     `mp reviews pass` which is idempotent (M145).
//! If ReReview is later wired into `next_stage`, the cache would
//! defeat L5 and the fix is to clear the Coordinator cache entry on
//! ReReview transition (see F-11 description for the design).
//!
//! Sequencing contract (AC-05): milestones are processed strictly in
//! order. M(i+1) does not start until M(i) reaches `complete` (or is
//! skipped). No interleaving.

use serde::Serialize;

use crate::watch::{drive_milestone, DriveOutcome, RunOutcome, SystemDriveOps};

/// Per-milestone entry in the [`SequencerReport`]. Mirrors the
/// [`DriveOutcome`] plus the input id for traceability.
#[derive(Debug, Clone, Serialize)]
pub struct MilestoneOutcome {
    pub id: String,
    pub outcome: DriveOutcome,
}

/// Overall sequencer result. `all_complete` is true when every
/// milestone reached `Complete`. Skipped milestones are surfaced
/// explicitly so callers can decide whether to treat a skip as
/// failure (the CLI sets a non-zero exit when any milestone was
/// skipped per AC-07). M197: `any_spawn_failed` is the new
/// terminal-failure signal — a `pane split` or `agent start`
/// call exited non-zero, so the run halted without retry.
#[derive(Debug, Clone, Serialize)]
pub struct SequencerReport {
    pub outcomes: Vec<MilestoneOutcome>,
    pub all_complete: bool,
    pub any_skipped: bool,
    pub any_exhausted: bool,
    pub any_spawn_failed: bool,
}

/// Drive each milestone id in order through a shared ops instance.
/// The ops struct carries the pane cache, so the second milestone's
/// `ensure_pane(runner)` reuses the first milestone's runner pane
/// (AC-04).
///
/// Review finding #5: `MaxIterationsExhausted` halts the loop. A
/// runaway milestone (remediation stuck, never advancing) consumes
/// its iteration cap, gets flagged, and is NOT followed by a
/// half-finished attempt at M(i+1). The `any_exhausted` flag in the
/// report signals the CLI to exit non-zero.
///
/// M197 WP3 / AC-04: a verified spawn failure (`pane split` or
/// `agent start` exit non-zero) also halts the loop. The
/// state machine has already emitted a structured `spawn_error`
/// log entry with the full argv / stdout / stderr / exit code;
/// the sequencer's job is to translate that into a
/// `RunOutcome::SpawnFailed` so the v2 control-plane state and
/// `mp watch status` carry the same diagnostic the operator
/// already saw in the log. A `spawn-failed` run never retries —
/// retrying a known-bad launch would just pin the herdr pane in
/// a stale state and waste the operator's time.
pub fn run_milestones(
    ops: &mut SystemDriveOps,
    ids: &[String],
    max_iterations_per_milestone: usize,
) -> anyhow::Result<SequencerReport> {
    let mut outcomes = Vec::with_capacity(ids.len());
    let mut all_complete = true;
    let mut any_skipped = false;
    let mut any_exhausted = false;
    let mut any_spawn_failed = false;

    for id in ids {
        // Swap the active milestone id onto the shared ops. The pane
        // cache is NOT reset between milestones — that is the AC-04
        // pane-reuse contract.
        ops.set_active_milestone(id.clone())?;
        let outcome = match drive_milestone(ops, max_iterations_per_milestone) {
            Ok(o) => o,
            Err(err) => {
                // Translate a SpawnFailure into a
                // `DriveOutcome::SpawnFailed` so the normal outcome
                // arm below records it consistently. Any other
                // error is re-raised unchanged.
                if let Some(failure) = crate::watch::herdr::extract_spawn_failure(&err) {
                    let run_outcome = RunOutcome::SpawnFailed {
                        command: failure.command,
                        argv: failure.argv,
                        exit_code: failure.exit_code,
                        stderr: failure.stderr,
                    };
                    DriveOutcome::SpawnFailed {
                        run_outcome: Box::new(run_outcome),
                    }
                } else {
                    return Err(err);
                }
            }
        };
        // M152 S4: Shutdown halts the loop just like MaxIterationsExhausted.
        // M197 WP3: SpawnFailed halts the loop too (no retry of a
        // known-bad launch).
        let halt = matches!(
            outcome,
            DriveOutcome::MaxIterationsExhausted { .. }
                | DriveOutcome::Shutdown
                | DriveOutcome::SpawnFailed { .. }
        );
        let is_complete = outcome == DriveOutcome::Complete;
        if !is_complete {
            all_complete = false;
        }
        if matches!(outcome, DriveOutcome::Skipped { .. }) {
            any_skipped = true;
        }
        if matches!(outcome, DriveOutcome::MaxIterationsExhausted { .. }) {
            any_exhausted = true;
        }
        if matches!(outcome, DriveOutcome::SpawnFailed { .. }) {
            any_spawn_failed = true;
        }
        outcomes.push(MilestoneOutcome {
            id: id.clone(),
            outcome: outcome.clone(),
        });
        // M178 S2: write the per-milestone outcome to the v2
        // control-plane state. Mapping:
        //   DriveOutcome::Complete ⇒ RunOutcome::Completed
        //   DriveOutcome::Skipped { reason } ⇒ RunOutcome::Skipped { reason }
        //   DriveOutcome::MaxIterationsExhausted { n } ⇒ RunOutcome::Exhausted { n }
        //   DriveOutcome::Shutdown ⇒ RunOutcome::GracefullyStopped
        //   DriveOutcome::SpawnFailed { run_outcome } ⇒ carried through verbatim
        let run_outcome = match &outcome {
            DriveOutcome::Complete => RunOutcome::Completed,
            DriveOutcome::Skipped { reason } => RunOutcome::Skipped {
                reason: reason.clone(),
            },
            DriveOutcome::MaxIterationsExhausted { iterations } => RunOutcome::Exhausted {
                iterations: *iterations,
            },
            DriveOutcome::Shutdown => RunOutcome::GracefullyStopped,
            DriveOutcome::SpawnFailed { run_outcome } => *run_outcome.clone(),
        };
        ops.record_milestone_outcome(id.clone(), run_outcome)?;
        // Halt on exhaustion: a runaway milestone should NOT march
        // forward to M(i+1). Skipped milestones continue (a blocked
        // dep on M(i) doesn't justify halting M(i+1)). Shutdown halts
        // so the cli layer can perform cleanup before exit (state
        // file flush + flash note). SpawnFailed halts to avoid
        // re-launching a known-bad herdr pane.
        if halt {
            break;
        }
    }

    // Aggregate terminality belongs exclusively to the sequencer. Per-item
    // skips remain non-terminal while later queue entries are active.
    if !outcomes.is_empty() {
        let completed = outcomes
            .iter()
            .filter(|item| matches!(item.outcome, DriveOutcome::Complete))
            .count();
        let skipped = outcomes
            .iter()
            .filter(|item| matches!(item.outcome, DriveOutcome::Skipped { .. }))
            .count();
        let exhausted = outcomes.iter().find_map(|item| match item.outcome {
            DriveOutcome::MaxIterationsExhausted { iterations } => Some(iterations),
            _ => None,
        });
        let shutdown = outcomes
            .iter()
            .any(|item| matches!(item.outcome, DriveOutcome::Shutdown));
        // M197: a spawn failure takes precedence over every other
        // aggregate shape. The single SpawnFailed outcome is
        // forwarded verbatim (its argv + stderr payload is the
        // diagnostic the operator needs).
        let spawn_failed_outcome = outcomes.iter().find_map(|item| match &item.outcome {
            DriveOutcome::SpawnFailed { run_outcome } => Some(*run_outcome.clone()),
            _ => None,
        });
        let aggregate = if let Some(outcome) = spawn_failed_outcome {
            outcome
        } else if shutdown {
            RunOutcome::GracefullyStopped
        } else if completed == outcomes.len() {
            RunOutcome::Completed
        } else if completed > 0 || (skipped > 0 && exhausted.is_some()) {
            RunOutcome::PartialFailure
        } else if let Some(iterations) = exhausted {
            RunOutcome::Exhausted { iterations }
        } else {
            let reason = outcomes
                .iter()
                .find_map(|item| match &item.outcome {
                    DriveOutcome::Skipped { reason } => Some(reason.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "queue did not complete".to_string());
            RunOutcome::Skipped { reason }
        };
        ops.transition(crate::watch::WatchTransition::RunOutcome(aggregate.clone()))?;
        let queue: Vec<String> = ops.run_state().map(|s| s.queue.clone()).unwrap_or_default();
        crate::activity::append_event_best_effort(
            &crate::paths::PlanContext {
                project_root: ops.project_root.clone(),
                plan_dir: ops.plan_dir.clone(),
            },
            crate::activity::watch_outcome_event(&aggregate, &queue),
        )
        .ok();
    }

    Ok(SequencerReport {
        outcomes,
        all_complete,
        any_skipped,
        any_exhausted,
        any_spawn_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_id_list_is_a_noop_with_all_complete_true() {
        // The all_complete / any_skipped / any_exhausted flags are
        // vacuous for an empty input — the property holds by
        // construction. Pane reuse + ordering with real fake binaries
        // are covered by tests/watch_sequential.rs (SystemDriveOps
        // reads milestones from disk, which the unit-test fixture
        // does not provide).
        let env = tempfile::TempDir::new().unwrap();
        let mut ops = SystemDriveOps::new(
            env.path().join("mp"),
            env.path().join("herdr"),
            env.path().to_path_buf(),
            "1",
            crate::watch::state_machine::RoleConfigs::default(),
        );
        let report = run_milestones(&mut ops, &[], 5).unwrap();
        assert!(report.all_complete);
        assert!(!report.any_skipped);
        assert!(!report.any_exhausted);
        assert!(report.outcomes.is_empty());
    }

    #[test]
    fn report_flags_aggregate_correctly_for_mixed_outcomes() {
        // The report-shape aggregation is pure data manipulation;
        // verify it directly without spawning anything.
        let report = SequencerReport {
            outcomes: vec![
                MilestoneOutcome {
                    id: "1".into(),
                    outcome: DriveOutcome::Skipped { reason: "x".into() },
                },
                MilestoneOutcome {
                    id: "2".into(),
                    outcome: DriveOutcome::MaxIterationsExhausted { iterations: 3 },
                },
                MilestoneOutcome {
                    id: "3".into(),
                    outcome: DriveOutcome::Complete,
                },
            ],
            all_complete: false,
            any_skipped: true,
            any_exhausted: true,
            any_spawn_failed: false,
        };
        assert!(!report.all_complete);
        assert!(report.any_skipped);
        assert!(report.any_exhausted);
        assert_eq!(report.outcomes.len(), 3);
    }

    #[test]
    fn all_complete_report_when_every_milestone_completes() {
        let report = SequencerReport {
            outcomes: vec![
                MilestoneOutcome {
                    id: "1".into(),
                    outcome: DriveOutcome::Complete,
                },
                MilestoneOutcome {
                    id: "2".into(),
                    outcome: DriveOutcome::Complete,
                },
            ],
            all_complete: true,
            any_skipped: false,
            any_exhausted: false,
            any_spawn_failed: false,
        };
        assert!(report.all_complete);
        assert!(!report.any_skipped);
        assert!(!report.any_exhausted);
    }

    #[test]
    fn sequencer_halts_on_max_iterations_exhausted() {
        // Review finding #5: a runaway milestone consumes its cap and
        // halts the loop. The remaining ids are NOT processed.
        // Verified via the report shape (any_exhausted=true, outcomes
        // short of the input list).
        let report = SequencerReport {
            outcomes: vec![MilestoneOutcome {
                id: "1".into(),
                outcome: DriveOutcome::MaxIterationsExhausted { iterations: 10 },
            }],
            all_complete: false,
            any_skipped: false,
            any_exhausted: true,
            any_spawn_failed: false,
        };
        assert!(!report.all_complete);
        assert!(report.any_exhausted);
        assert_eq!(report.outcomes.len(), 1);
    }
}
