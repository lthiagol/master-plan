use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, SessionCmd};
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::session;

pub(crate) fn cmd_session(ctx: &PlanContext, cmd: SessionCmd, format: Fmt) -> Result<()> {
    if matches!(
        &cmd,
        SessionCmd::Show { .. } | SessionCmd::List | SessionCmd::Export { .. }
    ) {
        return cmd_session_inner(ctx, cmd, format, None);
    }
    let recoverable = matches!(
        &cmd,
        SessionCmd::Start { .. } | SessionCmd::Archive { .. } | SessionCmd::Promote { .. }
    );
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|txn| cmd_session_inner(ctx, cmd, format, Some(txn)))
    } else {
        txn.run(|txn| cmd_session_inner(ctx, cmd, format, Some(txn)))
    }
}

fn cmd_session_inner(
    ctx: &PlanContext,
    cmd: SessionCmd,
    format: Fmt,
    txn: Option<&crate::plan_io::PlanWriteTxn>,
) -> Result<()> {
    match cmd {
        SessionCmd::Start { branch, title } => {
            let payload = session::session_start_in_txn(
                ctx,
                branch.as_deref(),
                title.as_deref(),
                txn.expect("write command must own PlanWriteTxn"),
            )?;
            emit(format, &payload)
        }
        SessionCmd::Show { id } => {
            let report = session::session_show(ctx, id.as_deref())?;
            emit(format, &report)
        }
        SessionCmd::List => {
            let sessions = session::session_list(ctx)?;
            emit(format, &json!({ "ok": true, "sessions": sessions }))
        }
        SessionCmd::Focus { id } => {
            let payload = session::session_focus(ctx, &id)?;
            emit(format, &payload)
        }
        SessionCmd::Unfocus => {
            let payload = session::session_unfocus(ctx)?;
            emit(format, &payload)
        }
        SessionCmd::Archive { id, force } => {
            session::session_archive(ctx, &id, force)?;
            emit(format, &json!({ "ok": true, "archived": id }))
        }
        SessionCmd::Export { id } => {
            let export = session::session_export(ctx, &id)?;
            emit(format, &export)
        }
        SessionCmd::Promote { id, milestone } => {
            let payload = session::session_promote(ctx, &id, milestone.as_deref())?;
            emit(format, &payload)
        }
    }
}
