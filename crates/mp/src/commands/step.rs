use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, StepCmd};
use crate::commands::common::{emit, prose_verification_warn, shell_parse_preflight};
use crate::paths::PlanContext;
use crate::step;

// M113 S1: the outer `cmd_milestone` dispatcher already acquires the
// plan-write advisory lock around the whole milestone command subtree;
// step subcommands reach us through that path so no second lock is
// needed here. Re-locking inside would deadlock.

pub(crate) fn cmd_step(ctx: &PlanContext, cmd: StepCmd, format: Fmt) -> Result<()> {
    match cmd {
        StepCmd::Add {
            milestone,
            wp,
            id,
            after,
            action,
            files,
            tests,
            done_when,
            covers_ac,
        } => {
            let input = step::AddStepInput {
                wp,
                id,
                after,
                action: action.unwrap_or_default(),
                files: step::parse_csv_list(files.as_deref()),
                tests: tests.unwrap_or_default(),
                done_when: done_when.unwrap_or_default(),
                covers_ac: step::parse_csv_list(covers_ac.as_deref()),
            };
            let prose_target = input.tests.clone();
            let s = step::add_step(ctx, &milestone, input)?;
            let mut payload = json!({ "ok": true, "step": s });
            if let Some(warning) = prose_verification_warn(&prose_target) {
                payload["prose_warning"] = warning;
            }
            emit(format, &payload)
        }
        StepCmd::SetStatus {
            milestone,
            step: step_id,
            status,
        } => {
            let s = step::set_step_status(ctx, &milestone, &step_id, &status)?;
            emit(format, &json!({ "ok": true, "step": s }))
        }
        StepCmd::Show { milestone, step } => {
            let value = step::show_step(ctx, &milestone, &step)?;
            emit(format, &value)
        }
        StepCmd::Done { milestone, step } => {
            let s = step::set_step_status(ctx, &milestone, &step, "done")?;
            emit(format, &json!({ "ok": true, "step": s }))
        }
        StepCmd::Update {
            milestone,
            step: step_id,
            action,
            files,
            tests,
            done_when,
            covers_ac,
            wp,
            depends_on_steps,
            evidence,
        } => {
            // M111 S6: capture for the pre-flight warning before it moves
            // into UpdateStepInput.
            let preflight_target = tests.clone();
            let input = step::UpdateStepInput {
                action,
                files: files.map(|f| step::parse_csv_list(Some(&f))),
                tests,
                done_when,
                covers_ac: covers_ac.map(|f| step::parse_csv_list(Some(&f))),
                work_package: wp,
                depends_on_steps: depends_on_steps.map(|d| step::parse_csv_list(Some(&d))),
                evidence,
            };
            let s = step::update_step(ctx, &milestone, &step_id, input)?;
            let mut payload = json!({ "ok": true, "step": s });
            if let Some(t) = preflight_target.as_deref() {
                if let Some(warning) = shell_parse_preflight(t) {
                    payload["preflight_warning"] = warning;
                }
                if let Some(warning) = prose_verification_warn(t) {
                    payload["prose_warning"] = warning;
                }
            }
            emit(format, &payload)
        }
        StepCmd::Split { milestone, step } => {
            let steps = step::split_step(ctx, &milestone, &step)?;
            emit(format, &json!({ "ok": true, "steps": steps }))
        }
        StepCmd::Remove { milestone, step } => {
            let result = step::remove_step(ctx, &milestone, &step)?;
            emit(format, &result)
        }
        StepCmd::Fail { milestone, step } => {
            let s = step::fail_step(ctx, &milestone, &step)?;
            emit(format, &json!({ "ok": true, "step": s }))
        }
        StepCmd::Claim {
            milestone,
            step,
            by,
            lease,
        } => {
            let s = crate::step_claim::claim_step(ctx, &milestone, &step, &by, lease.as_deref())?;
            emit(format, &json!({ "ok": true, "step": s }))
        }
        StepCmd::Release { milestone, step } => {
            let s = crate::step_claim::release_step(ctx, &milestone, &step)?;
            emit(format, &json!({ "ok": true, "step": s }))
        }
    }
}
