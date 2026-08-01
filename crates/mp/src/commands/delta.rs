use anyhow::Result;

use crate::cli::{DeltaCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::delta;
use crate::paths::PlanContext;

pub(crate) fn cmd_delta(ctx: &PlanContext, cmd: DeltaCmd, format: Fmt) -> Result<()> {
    match cmd {
        DeltaCmd::Rebase { milestone } => {
            let payload = delta::delta_rebase_report(ctx, &milestone)?;
            emit(format, &payload)
        }
    }
}
