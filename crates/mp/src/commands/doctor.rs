use anyhow::Result;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::doctor;
use crate::paths::PlanContext;

pub(crate) fn cmd_doctor(ctx: &PlanContext, project: bool, format: Fmt) -> Result<()> {
    let report = if project || ctx.plan_dir.is_dir() {
        doctor::doctor_project(ctx)
    } else {
        doctor::doctor_toolkit()
    };
    emit(format, &report)
}
