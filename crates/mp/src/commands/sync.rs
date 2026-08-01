use anyhow::Result;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::sync;

pub(crate) fn cmd_sync(ctx: &PlanContext, format: Fmt) -> Result<()> {
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run_recoverable(|_| {
        let report = sync::sync_plan(ctx)?;
        emit(format, &report)
    })
}
