use anyhow::{bail, Result};
use serde_json::json;

use crate::brief;
use crate::cli::{BriefCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_brief(ctx: &PlanContext, cmd: BriefCmd, format: Fmt) -> Result<()> {
    if matches!(
        &cmd,
        BriefCmd::Todo | BriefCmd::List | BriefCmd::Show { .. }
    ) {
        return cmd_brief_inner(ctx, cmd, format);
    }
    let recoverable = matches!(
        &cmd,
        BriefCmd::Done | BriefCmd::Reopen | BriefCmd::Promote { .. }
    );
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|_| cmd_brief_inner(ctx, cmd, format))
    } else {
        txn.run(|_| cmd_brief_inner(ctx, cmd, format))
    }
}

fn cmd_brief_inner(ctx: &PlanContext, cmd: BriefCmd, format: Fmt) -> Result<()> {
    match cmd {
        BriefCmd::Todo => {
            let report = brief::brief_todo(ctx)?;
            emit(format, &report)
        }
        BriefCmd::List => {
            let topics = brief::brief_list(ctx)?;
            emit(format, &json!({ "ok": true, "topics": topics }))
        }
        BriefCmd::Show { id } => {
            let value = brief::brief_show(ctx, id.as_deref())?;
            emit(format, &value)
        }
        BriefCmd::Edit { id, body, status } => {
            let topic = brief::brief_edit(ctx, &id, body.as_deref(), status.as_deref())?;
            emit(format, &json!({ "ok": true, "topic": topic }))
        }
        BriefCmd::Add {
            title,
            prompt,
            required,
        } => {
            let topic = brief::brief_add(ctx, &title, prompt.as_deref(), required)?;
            emit(format, &json!({ "ok": true, "topic": topic }))
        }
        BriefCmd::Rm { id } => {
            brief::brief_rm(ctx, &id)?;
            emit(format, &json!({ "ok": true, "removed": id }))
        }
        BriefCmd::Skip { id } => {
            let topic = brief::brief_skip(ctx, &id)?;
            emit(format, &json!({ "ok": true, "topic": topic }))
        }
        BriefCmd::Done => {
            let report = brief::brief_done(ctx)?;
            emit(format, &report)?;
            if !report.ok {
                return Err(anyhow::Error::new(crate::ExitCode(2)));
            }
            Ok(())
        }
        BriefCmd::Reopen => {
            let report = brief::brief_reopen(ctx)?;
            emit(format, &report)
        }
        BriefCmd::Promote {
            id,
            to_idea,
            to_backlog,
        } => {
            let target_count = to_idea as u8 + to_backlog as u8;
            if target_count != 1 {
                bail!("specify exactly one of --to-idea or --to-backlog");
            }
            let target = if to_idea {
                brief::BriefPromoteTarget::Idea
            } else {
                brief::BriefPromoteTarget::Backlog
            };
            let payload = brief::brief_promote(ctx, &id, target)?;
            emit(format, &payload)
        }
        BriefCmd::Import { from_file } => {
            let report = brief::brief_import(ctx, &from_file)?;
            emit(format, &report)
        }
    }
}
