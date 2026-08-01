use anyhow::Result;

use crate::cli::{ExecutionCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::execution;
use crate::execution_report;
use crate::paths::PlanContext;

pub(crate) fn cmd_execution(ctx: &PlanContext, cmd: ExecutionCmd, format: Fmt) -> Result<()> {
    if !matches!(
        &cmd,
        ExecutionCmd::Handoff { .. } | ExecutionCmd::Pause { .. }
    ) {
        return cmd_execution_inner(ctx, cmd, format, None);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|txn| cmd_execution_inner(ctx, cmd, format, Some(txn)))
}

fn cmd_execution_inner(
    ctx: &PlanContext,
    cmd: ExecutionCmd,
    format: Fmt,
    txn: Option<&crate::plan_io::PlanWriteTxn>,
) -> Result<()> {
    match cmd {
        ExecutionCmd::Check => {
            let report = execution::execution_check(ctx)?;
            emit(format, &report)
        }
        ExecutionCmd::Handoff {
            allow_tracks_only,
            by,
        } => {
            let payload = execution::execution_handoff_in_txn(
                ctx,
                allow_tracks_only,
                by.as_deref(),
                txn.expect("write command must own PlanWriteTxn"),
            )?;
            emit(format, &payload)
        }
        ExecutionCmd::HandoffShow => {
            let payload = crate::plan_diff::handoff_show(ctx)?;
            emit(format, &payload)
        }
        ExecutionCmd::Pause { reason } => {
            let payload = execution::execution_pause(ctx, reason.as_deref())?;
            emit(format, &payload)
        }
        ExecutionCmd::Status => {
            let payload = execution::execution_status(ctx)?;
            emit(format, &payload)
        }
        ExecutionCmd::Report { milestone } => {
            let report = execution_report::build_execution_report(ctx, &milestone)?;
            emit(format, &report)
        }
    }
}
