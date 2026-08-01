use anyhow::Result;

use crate::brownfield;
use crate::cli::{BrownfieldCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_brownfield(ctx: &PlanContext, cmd: BrownfieldCmd, format: Fmt) -> Result<()> {
    match cmd {
        BrownfieldCmd::Scan { domain, query } => {
            let report = brownfield::scan(ctx, domain.as_deref(), query.as_deref())?;
            emit(format, &report)
        }
    }
}
