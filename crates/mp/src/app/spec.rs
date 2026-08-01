use anyhow::Result;

use crate::cli::{OutputFormat, SpecCmd};
use crate::commands::common::emit_value;
use crate::paths::PlanContext;

pub(super) fn run(
    ctx: &PlanContext,
    cmd: SpecCmd,
    format: OutputFormat,
    fields: &[String],
) -> Result<()> {
    ctx.ensure_plan_exists()?;
    let value = match cmd {
        SpecCmd::Review { milestone } => crate::spec_review::spec_review(ctx, &milestone)?,
        SpecCmd::Diff { milestone } => crate::spec_review::spec_diff(ctx, &milestone)?,
    };
    emit_value(format, &value, fields)
}
