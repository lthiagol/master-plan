use anyhow::Result;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit_and_exit_on_fail;
use crate::doctor;
use crate::paths::PlanContext;

/// M197 F-11 / AC-02: `mp doctor` exits non-zero when the report is
/// red (herdr CLI shape mismatch, missing harness config, etc.) so
/// shell pipelines and CI can detect the failure. Previously the
/// report was emitted and the binary returned Ok(()) unconditionally,
/// which silently swallowed the same surface `mp config validate`
/// and `mp config set` already gated via
/// [`crate::commands::common::emit_and_exit_on_fail`].
pub(crate) fn cmd_doctor(ctx: &PlanContext, project: bool, format: Fmt) -> Result<()> {
    let report = if project || ctx.plan_dir.is_dir() {
        doctor::doctor_project(ctx)
    } else {
        doctor::doctor_toolkit()
    };
    emit_and_exit_on_fail(format, &report, report.ok)
}
