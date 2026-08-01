use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, SpecsCmd};
use crate::commands::common::emit;
use crate::commands::delta as cmd_delta_mod;
use crate::paths::PlanContext;
use crate::specs;

pub(crate) fn cmd_specs(ctx: &PlanContext, cmd: SpecsCmd, format: Fmt) -> Result<()> {
    if matches!(&cmd, SpecsCmd::List | SpecsCmd::Show { .. }) {
        return cmd_specs_inner(ctx, cmd, format);
    }
    let recoverable = matches!(&cmd, SpecsCmd::Delta(_));
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|_| cmd_specs_inner(ctx, cmd, format))
    } else {
        txn.run(|_| cmd_specs_inner(ctx, cmd, format))
    }
}

fn cmd_specs_inner(ctx: &PlanContext, cmd: SpecsCmd, format: Fmt) -> Result<()> {
    match cmd {
        SpecsCmd::List => {
            let domains = specs::list_domains(ctx)?;
            emit(format, &domains)
        }
        SpecsCmd::Show { domain } => {
            let spec = specs::show_domain(ctx, &domain)?;
            emit(format, &spec)
        }
        SpecsCmd::Init { domain, title } => {
            let spec = specs::init_domain(ctx, &domain, title.as_deref())?;
            emit(format, &json!({ "ok": true, "domain": spec.domain }))
        }
        SpecsCmd::Delta(cmd) => cmd_delta_mod::cmd_delta(ctx, cmd, format),
    }
}
