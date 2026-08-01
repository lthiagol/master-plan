use anyhow::Result;
use serde::Serialize;
use serde_json::json;

use crate::cli::{ConfigCmd, OutputFormat};
use crate::commands::common::emit;
use crate::config_cmd;
use crate::paths::PlanContext;
use crate::ExitCode;

/// Emit `report` on stdout, then exit with code 1 when `report.ok` is false.
/// Single home for the "report first, then maybe fail" pattern shared by
/// `config set`, `config set --dry-run`, and `config validate` — adding
/// a fourth consumer means changing this one helper.
fn emit_and_exit_on_fail<T: Serialize>(format: OutputFormat, report: &T, ok: bool) -> Result<()> {
    emit(format, report)?;
    if !ok {
        return Err(ExitCode(1).into());
    }
    Ok(())
}

pub(crate) fn cmd_config(ctx: &PlanContext, cmd: ConfigCmd, format: OutputFormat) -> Result<()> {
    let writes = matches!(&cmd, ConfigCmd::Set { dry_run: false, .. });
    if !writes {
        return cmd_config_inner(ctx, cmd, format);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|_| cmd_config_inner(ctx, cmd, format))
}

fn cmd_config_inner(ctx: &PlanContext, cmd: ConfigCmd, format: OutputFormat) -> Result<()> {
    match cmd {
        ConfigCmd::Show => {
            let report = config_cmd::config_show(ctx);
            emit(format, &report)
        }
        ConfigCmd::Get { key } => {
            let value = config_cmd::config_get(ctx, &key)?;
            emit(format, &json!({ "key": key, "value": value }))
        }
        ConfigCmd::Set {
            key,
            value,
            dry_run,
        } => {
            let report = config_cmd::config_set(ctx, &key, &value, dry_run)?;
            emit_and_exit_on_fail(format, &report, report.ok)
        }
        ConfigCmd::Validate { file } => {
            let report = config_cmd::config_validate(ctx, file.as_deref());
            emit_and_exit_on_fail(format, &report, report.ok)
        }
    }
}
