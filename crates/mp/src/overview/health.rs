//! `mp overview` health / totals / lifecycle / steps aggregators.

use anyhow::Result;
use serde::Serialize;

use crate::autopilot::drive;
use crate::autopilot::drive::classification::{classify_state, RunState};
use crate::model::{effective_lifecycle, MilestoneFile};
use crate::paths::PlanContext;
use crate::store;
use crate::validate;

#[derive(Debug, Clone, Serialize)]
pub struct OverviewHealth {
    /// One of `ok`, `errors`. Mirrors `mp validate --summary`.
    pub validation_state: String,
    pub validation_error_count: usize,
    /// Number of milestones with `execution_status == "blocked"`.
    pub blocker_count: usize,
    /// Current execution mode (`planning` / `autonomous`).
    pub execution_mode: String,
    /// Plan-level planning status (`planning`, `in-execution`, …).
    pub planning_state: String,
    /// M180 / AC-05 watch state (idle / running / stopped / failed /
    /// complete). Derived from M178's classification + terminal
    /// outcomes; see M180 design decision `watch-summary-derivation`.
    pub watch_state: &'static str,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OverviewTotals {
    pub milestones: usize,
}

impl OverviewTotals {
    pub fn compute(milestones: usize) -> Self {
        Self { milestones }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LifecycleRollup {
    pub draft: usize,
    pub groomed: usize,
    pub approved: usize,
    pub in_progress: usize,
    pub done: usize,
    pub self_reviewed: usize,
    pub reviewed: usize,
    pub complete: usize,
    pub remediation: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StepRollup {
    pub pending: usize,
    pub in_progress: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Build health / totals / lifecycle / step rollups from the
/// milestone snapshot. Reuses `validate::validate_plan_with_milestones`
/// to share the existing gate-aware validate pass; reuses
/// `drive::WatchRunState::load_from` for the watch-state derivation
/// (M178 S1 v2 control-plane shape).
pub fn build_health(ctx: &PlanContext) -> Result<OverviewHealthBundle> {
    let plan = store::load_plan(ctx).unwrap_or_default();
    let milestones = store::load_all_milestones(ctx)?;
    let milestones_total = milestones.len();

    let validate_report =
        validate::validate_plan_with_milestones(ctx, &milestones).unwrap_or_else(|_| {
            validate::ValidationReport {
                ok: false,
                errors: Vec::new(),
                warnings: Vec::new(),
                l5_audit: None,
            }
        });
    let validation_state = if validate_report.ok {
        "ok".to_string()
    } else {
        "errors".to_string()
    };
    let validation_error_count = validate_report.errors.len();

    let mut blocker_count = 0usize;
    for (_, m) in &milestones {
        if m.milestone.execution_status == "blocked" || m.milestone.blocked {
            blocker_count += 1;
        }
    }

    let watch_state = derive_watch_state(ctx);
    let execution_mode = plan.execution.mode.clone();
    let planning_state = plan.project.planning_status.clone();

    let health = OverviewHealth {
        validation_state,
        validation_error_count,
        blocker_count,
        execution_mode,
        planning_state,
        watch_state,
    };

    let (lifecycle, steps) = rollup_milestones(&milestones);
    let totals = OverviewTotals::compute(milestones_total);

    Ok(OverviewHealthBundle {
        health,
        totals,
        lifecycle,
        steps,
    })
}

/// Convenience bundle returned from [`build_health`]; the step and
/// lifecycle rollups are produced in the same milestone walk so we
/// return them together rather than recomputing.
#[derive(Debug, Clone)]
pub struct OverviewHealthBundle {
    pub health: OverviewHealth,
    pub totals: OverviewTotals,
    pub lifecycle: LifecycleRollup,
    pub steps: StepRollup,
}

fn rollup_milestones(
    milestones: &[(std::path::PathBuf, MilestoneFile)],
) -> (LifecycleRollup, StepRollup) {
    let mut lifecycle = LifecycleRollup::default();
    for (_, m) in milestones {
        let lc = effective_lifecycle(&m.milestone);
        match lc.as_str() {
            "draft" => lifecycle.draft += 1,
            "groomed" => lifecycle.groomed += 1,
            "approved" => lifecycle.approved += 1,
            "in-progress" => lifecycle.in_progress += 1,
            "done" => lifecycle.done += 1,
            "self-reviewed" => lifecycle.self_reviewed += 1,
            "reviewed" => lifecycle.reviewed += 1,
            "complete" => lifecycle.complete += 1,
            "remediation" => lifecycle.remediation += 1,
            _ => {}
        }
    }

    let mut steps = StepRollup::default();
    for (_, m) in milestones {
        for s in &m.steps {
            match s.status.as_str() {
                "pending" => steps.pending += 1,
                "in-progress" => steps.in_progress += 1,
                "done" => steps.done += 1,
                "failed" => steps.failed += 1,
                "skipped" => steps.skipped += 1,
                _ => {}
            }
        }
    }
    (lifecycle, steps)
}

/// M180 / AC-05 watch summary state, derived from M178's v2 control
/// plane + terminal outcomes. The mapping is fixed by the
/// `watch-summary-derivation` design decision:
///
/// - `idle` — no recorded run (no state file or no terminal outcome
///   and the live classification is also empty). A stale/interrupted
///   run with no terminal outcome also lands here: the latest
///   recorded run has not produced a terminal result yet, so there
///   is nothing to summarize.
/// - `running` — M178 classifies the run live
///   ([`drive::RunState::Live`]).
/// - `complete` — terminal outcome [`drive::RunOutcome::Completed`].
/// - `failed` — terminal outcome [`drive::RunOutcome::PartialFailure`],
///   [`drive::RunOutcome::Skipped`], [`drive::RunOutcome::Exhausted`].
/// - `stopped` — terminal outcome
///   [`drive::RunOutcome::GracefullyStopped`].
fn derive_watch_state(ctx: &PlanContext) -> &'static str {
    let path = drive::default_run_state_path(&ctx.plan_dir);
    let Some(state) = drive::WatchRunState::load_from(&path).ok().flatten() else {
        return "idle";
    };
    if let Some(outcome) = state.run_outcome.as_ref() {
        use drive::RunOutcome::*;
        return match outcome {
            Completed => "complete",
            PartialFailure | Skipped { .. } | Exhausted { .. } => "failed",
            // M197 WP3 / AC-04: a spawn failure is a distinct
            // terminal kind — different from a lifecycle failure
            // (no agent work happened) and from a skip (no skip
            // decision was made). Raul's summary badge surfaces
            // "spawn failed" as its own row.
            SpawnFailed { .. } => "spawn-failed",
            GracefullyStopped => "stopped",
        };
    }
    // No terminal outcome yet — classify as live/stale/idle using the
    // PID + herdr list probe. classification::classify_state without a
    // herdr list falls back to the PID probe alone, which is the
    // right call here: Raul doesn't read herdr, and an idle project
    // (no panes, no live PID) must report idle.
    let herdr_list = None;
    match classify_state(Some(&state), herdr_list) {
        RunState::Live => "running",
        RunState::Stale { .. } | RunState::Terminal => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_watch_state_returns_idle_when_state_file_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = PlanContext {
            project_root: tmp.path().to_path_buf(),
            plan_dir: tmp.path().to_path_buf(),
        };
        assert_eq!(derive_watch_state(&ctx), "idle");
    }
}
