use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::cli::{IdeaCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::idea;
use crate::paths::PlanContext;
use crate::step;

pub(crate) fn cmd_idea(ctx: &PlanContext, cmd: IdeaCmd, format: Fmt) -> Result<()> {
    if matches!(&cmd, IdeaCmd::List { .. } | IdeaCmd::Show { .. }) {
        return cmd_idea_inner(ctx, cmd, format);
    }
    let recoverable = matches!(&cmd, IdeaCmd::Promote { .. });
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|_| cmd_idea_inner(ctx, cmd, format))
    } else {
        txn.run(|_| cmd_idea_inner(ctx, cmd, format))
    }
}

fn cmd_idea_inner(ctx: &PlanContext, cmd: IdeaCmd, format: Fmt) -> Result<()> {
    match cmd {
        IdeaCmd::Create {
            title,
            body,
            tags,
            source,
        } => {
            let tags = step::parse_csv_list(tags.as_deref());
            let idea = idea::idea_create(ctx, &title, body.as_deref(), tags, source.as_deref())?;
            emit(format, &json!({ "ok": true, "idea": idea }))
        }
        IdeaCmd::List { status } => {
            let ideas = idea::idea_list(ctx, status.as_deref())?;
            emit(format, &json!({ "ok": true, "ideas": ideas }))
        }
        IdeaCmd::Show { id } => {
            let idea = idea::idea_show(ctx, &id)?;
            emit(format, &json!({ "ok": true, "idea": idea }))
        }
        IdeaCmd::Update {
            id,
            title,
            body,
            status,
            tags,
        } => {
            let tags = tags.map(|t| step::parse_csv_list(Some(&t)));
            let idea = idea::idea_update(
                ctx,
                &id,
                title.as_deref(),
                body.as_deref(),
                status.as_deref(),
                tags,
            )?;
            emit(format, &json!({ "ok": true, "idea": idea }))
        }
        IdeaCmd::Dismiss { id } => {
            let idea = idea::idea_dismiss(ctx, &id)?;
            emit(format, &json!({ "ok": true, "idea": idea }))
        }
        IdeaCmd::Archive { id } => {
            let idea = idea::idea_archive(ctx, &id)?;
            emit(format, &json!({ "ok": true, "idea": idea }))
        }
        IdeaCmd::Remove { id } => {
            idea::idea_remove(ctx, &id)?;
            emit(format, &json!({ "ok": true, "removed": id }))
        }
        IdeaCmd::Promote {
            id,
            to_milestone,
            to_backlog,
            to_track,
        } => {
            let target_count = to_milestone as u8 + to_backlog as u8 + to_track.is_some() as u8;
            if target_count != 1 {
                bail!("specify exactly one of --to-milestone, --to-backlog, or --to-track");
            }
            let target = if to_milestone {
                idea::IdeaPromoteTarget::Milestone
            } else if to_backlog {
                idea::IdeaPromoteTarget::Backlog
            } else {
                idea::IdeaPromoteTarget::Track(
                    to_track.as_deref().context("--to-track kind required")?,
                )
            };
            let payload = idea::idea_promote(ctx, &id, target)?;
            emit(format, &payload)
        }
    }
}
