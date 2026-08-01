use anyhow::Result;
use serde_json::json;

use crate::charter;
use crate::cli::{GoalsCmd, NongoalsCmd, OutputFormat as Fmt, PlanCmd, PrinciplesCmd};
use crate::commands::common::emit;
use crate::commands::metrics as cmd_metrics_mod;
use crate::commands::plan_verify_ac;
use crate::commands::plan_verify_lint;
use crate::groom;
use crate::paths::PlanContext;
use crate::plan_gaps;
use crate::step;

pub(crate) fn cmd_plan(ctx: &PlanContext, cmd: PlanCmd, format: Fmt) -> Result<()> {
    if matches!(
        &cmd,
        PlanCmd::Show
            | PlanCmd::Gaps { .. }
            | PlanCmd::Coverage { .. }
            | PlanCmd::InferDeps { .. }
            | PlanCmd::Diff { .. }
            | PlanCmd::VerifyLint
            | PlanCmd::VerifyAc { .. }
    ) {
        return cmd_plan_inner(ctx, cmd, format);
    }
    if matches!(&cmd, PlanCmd::Relocate { .. }) {
        let txn = crate::plan_io::PlanWriteTxn::acquire_project_root(&ctx.project_root)?;
        return txn.run(|_| cmd_plan_inner(ctx, cmd, format));
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|_| cmd_plan_inner(ctx, cmd, format))
}

fn cmd_plan_inner(ctx: &PlanContext, cmd: PlanCmd, format: Fmt) -> Result<()> {
    match cmd {
        PlanCmd::Show => {
            let report = charter::plan_show(ctx)?;
            emit(format, &report)
        }
        PlanCmd::Set {
            planning_status,
            planning_phase,
            target_version,
            stack,
            description,
            name,
        } => {
            let plan = charter::plan_set(
                ctx,
                charter::PlanSetInput {
                    planning_status,
                    planning_phase,
                    target_version,
                    stack: charter::parse_stack_csv(stack.as_deref()),
                    description,
                    name,
                },
            )?;
            emit(format, &json!({ "ok": true, "plan": plan }))
        }
        PlanCmd::Goals { cmd } => match cmd {
            GoalsCmd::Add { text } => {
                let plan = charter::plan_goals_add(ctx, &text)?;
                emit(format, &json!({ "ok": true, "goals": plan.charter.goals }))
            }
            GoalsCmd::Remove { text } => {
                let plan = charter::plan_goals_remove(ctx, &text)?;
                emit(format, &json!({ "ok": true, "goals": plan.charter.goals }))
            }
            GoalsCmd::Set { json } => {
                let plan = charter::plan_goals_set(ctx, &json)?;
                emit(format, &json!({ "ok": true, "goals": plan.charter.goals }))
            }
        },
        PlanCmd::Nongoals { cmd } => match cmd {
            NongoalsCmd::Add { text } => {
                let plan = charter::plan_nongoals_add(ctx, &text)?;
                emit(
                    format,
                    &json!({ "ok": true, "non_goals": plan.charter.non_goals }),
                )
            }
            NongoalsCmd::Remove { text } => {
                let plan = charter::plan_nongoals_remove(ctx, &text)?;
                emit(
                    format,
                    &json!({ "ok": true, "non_goals": plan.charter.non_goals }),
                )
            }
            NongoalsCmd::Set { json } => {
                let plan = charter::plan_nongoals_set(ctx, &json)?;
                emit(
                    format,
                    &json!({ "ok": true, "non_goals": plan.charter.non_goals }),
                )
            }
        },
        PlanCmd::Principles { cmd } => match cmd {
            PrinciplesCmd::Add { text } => {
                let plan = charter::plan_principles_add(ctx, &text)?;
                emit(
                    format,
                    &json!({ "ok": true, "principles": plan.charter.principles }),
                )
            }
            PrinciplesCmd::Remove { text } => {
                let plan = charter::plan_principles_remove(ctx, &text)?;
                emit(
                    format,
                    &json!({ "ok": true, "principles": plan.charter.principles }),
                )
            }
            PrinciplesCmd::Set { json } => {
                let plan = charter::plan_principles_set(ctx, &json)?;
                emit(
                    format,
                    &json!({ "ok": true, "principles": plan.charter.principles }),
                )
            }
        },
        PlanCmd::Gaps { id } => {
            let report = plan_gaps::plan_gaps(ctx, &id)?;
            emit(format, &report)
        }
        PlanCmd::Coverage { id } => {
            let report = groom::plan_coverage(ctx, &id)?;
            emit(format, &report)
        }
        PlanCmd::InferDeps { id } => {
            let report = step::infer_depends_on_steps(ctx, &id)?;
            emit(format, &report)
        }
        PlanCmd::Relocate { old, new } => {
            let old_path = ctx.project_root.join(&old);
            let new_path = ctx.project_root.join(&new);

            if !old_path.is_dir() {
                anyhow::bail!("plan directory not found: {}", old.display());
            }
            if new_path.exists() {
                anyhow::bail!("target directory already exists: {}", new.display());
            }

            // Snapshot config before any mutation so a post-persist config
            // failpoint can restore location with the directory.
            let prior_cfg = crate::store::load_config(ctx);

            // rename_plan_path rolls back the rename if a post-rename failpoint
            // fires, so a FAIL_AFTER_WRITE=1 never leaves path/config torn.
            crate::store::rename_plan_path(&old_path, &new_path)?;

            // Update location in the plan's config.json
            let new_ctx = PlanContext {
                project_root: ctx.project_root.clone(),
                plan_dir: new_path.clone(),
            };
            let mut cfg = prior_cfg.clone();
            cfg.workflow.plan.location = Some(new.to_string_lossy().to_string());
            if let Err(error) = crate::store::write_config(&new_ctx, &cfg) {
                // Path + config are one unit: roll the directory back, then
                // restore prior config (write_config may already have persisted
                // the new location before the failpoint returned Err).
                if new_path.exists() && !old_path.exists() {
                    crate::store::rename_plan_path(&new_path, &old_path).map_err(|rollback| {
                        anyhow::anyhow!(
                            "plan relocation config update failed: {error:#}; path rollback failed: {rollback}"
                        )
                    })?;
                }
                let restored_ctx = PlanContext {
                    project_root: ctx.project_root.clone(),
                    plan_dir: old_path,
                };
                if let Err(restore) = crate::store::write_config(&restored_ctx, &prior_cfg) {
                    return Err(anyhow::anyhow!(
                        "plan relocation config update failed: {error:#}; config restore failed: {restore}"
                    ));
                }
                return Err(error);
            }

            emit(format, &json!({ "ok": true, "old": old, "new": new }))
        }
        PlanCmd::Diff {
            since_handoff,
            since,
            git,
            markdown,
        } => {
            let report = crate::plan_diff::plan_diff(
                ctx,
                crate::plan_diff::PlanDiffOptions {
                    since_handoff,
                    since,
                    git_ref: git,
                    markdown,
                },
            )?;
            emit(format, &report)
        }
        PlanCmd::Metrics(cmd) => cmd_metrics_mod::cmd_metrics(ctx, cmd, format),
        PlanCmd::VerifyLint => {
            let report = plan_verify_lint::verify_lint(ctx)?;
            plan_verify_lint::print_human_warnings(&report);
            emit(format, &report)
        }
        PlanCmd::VerifyAc { id } => {
            let report = plan_verify_ac::verify_ac(ctx, &id)?;
            emit(format, &report)
        }
    }
}
