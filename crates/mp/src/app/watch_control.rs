use anyhow::Result;

use crate::cli::{OutputFormat, WatchControlCmd};
use crate::commands::watch_control;
use crate::paths::PlanContext;

pub(super) fn run(
    ctx: &PlanContext,
    cmd: WatchControlCmd,
    format: OutputFormat,
    fields: &[String],
) -> Result<()> {
    ctx.ensure_plan_exists()?;
    match cmd {
        WatchControlCmd::Status { summary } => {
            watch_control::cmd_watch_control_status(ctx, summary, format, fields)
        }
        WatchControlCmd::Stop { pid, timeout_secs } => {
            watch_control::cmd_watch_control_stop(ctx, pid, timeout_secs, format, fields)
        }
        WatchControlCmd::Output {
            max_bytes,
            timeout_ms,
            role,
        } => watch_control::cmd_watch_control_output(
            ctx, max_bytes, timeout_ms, role, format, fields,
        ),
        WatchControlCmd::Result { force } => {
            watch_control::cmd_watch_control_result(ctx, force, format, fields)
        }
    }
}
