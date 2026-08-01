use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::backlog;
use crate::cli::{BacklogCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_backlog(ctx: &PlanContext, cmd: BacklogCmd, format: Fmt) -> Result<()> {
    if matches!(&cmd, BacklogCmd::List { .. } | BacklogCmd::Show { .. }) {
        return cmd_backlog_inner(ctx, cmd, format);
    }
    let recoverable = matches!(&cmd, BacklogCmd::Promote { .. });
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|_| cmd_backlog_inner(ctx, cmd, format))
    } else {
        txn.run(|_| cmd_backlog_inner(ctx, cmd, format))
    }
}

fn cmd_backlog_inner(ctx: &PlanContext, cmd: BacklogCmd, format: Fmt) -> Result<()> {
    match cmd {
        BacklogCmd::List {
            source,
            status,
            priority,
            limit,
        } => {
            let items = backlog::backlog_list_filtered(
                ctx,
                source.as_deref(),
                status.as_deref(),
                priority.as_deref(),
                limit,
            )?;
            emit(format, &json!({ "items": items }))
        }
        BacklogCmd::Add {
            desc,
            source,
            suggested_when,
            priority,
        } => {
            let item = backlog::backlog_add(
                ctx,
                &desc,
                source.as_deref(),
                suggested_when.as_deref(),
                priority.as_deref(),
            )?;
            // TW-03 / M170: surface the assigned id on its own first line so
            // agents and humans can grab it without parsing the JSON body.
            println!("Assigned: {}", item.id);
            emit(format, &json!({ "ok": true, "item": item }))
        }
        BacklogCmd::Show { id } => {
            let item = backlog::backlog_show(ctx, &id)?;
            emit(format, &json!({ "ok": true, "item": item }))
        }
        BacklogCmd::Resolve {
            id,
            into_milestone,
            wont_fix,
            reason,
        } => {
            let item = backlog::backlog_resolve(
                ctx,
                &id,
                into_milestone.as_deref(),
                wont_fix,
                reason.as_deref(),
            )?;
            emit(format, &json!({ "ok": true, "item": item }))
        }
        BacklogCmd::Promote {
            id,
            to_milestone,
            to_track,
        } => {
            let target_count = to_milestone as u8 + to_track.is_some() as u8;
            if target_count != 1 {
                bail!("specify exactly one of --to-milestone or --to-track");
            }
            let target = if to_milestone {
                backlog::BacklogPromoteTarget::Milestone
            } else {
                backlog::BacklogPromoteTarget::Track(
                    to_track.as_deref().context("--to-track kind required")?,
                )
            };
            let payload = backlog::backlog_promote(ctx, &id, target)?;
            emit(format, &payload)
        }
    }
}
