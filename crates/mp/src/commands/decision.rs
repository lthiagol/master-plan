use anyhow::Result;
use serde_json::json;

use crate::cli::{DecisionCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::decisions;
use crate::paths::PlanContext;

pub(crate) fn cmd_decision(ctx: &PlanContext, cmd: DecisionCmd, format: Fmt) -> Result<()> {
    if matches!(&cmd, DecisionCmd::List) {
        return cmd_decision_inner(ctx, cmd, format);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|_| cmd_decision_inner(ctx, cmd, format))
}

fn cmd_decision_inner(ctx: &PlanContext, cmd: DecisionCmd, format: Fmt) -> Result<()> {
    match cmd {
        DecisionCmd::Add {
            summary,
            context,
            milestone,
        } => {
            let entry =
                decisions::decision_add(ctx, &summary, context.as_deref(), milestone.as_deref())?;
            emit(format, &json!({ "ok": true, "decision": entry }))
        }
        DecisionCmd::List => {
            let items = decisions::decision_list(ctx)?;
            emit(format, &json!({ "ok": true, "decisions": items }))
        }
        DecisionCmd::Remove { id } => {
            decisions::decision_remove(ctx, &id)?;
            emit(format, &json!({ "ok": true, "removed": id }))
        }
    }
}
