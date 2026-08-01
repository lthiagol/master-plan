use anyhow::Result;
use serde_json::json;

use crate::charter;
use crate::cli::{MetricsCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_metrics(ctx: &PlanContext, cmd: MetricsCmd, format: Fmt) -> Result<()> {
    match cmd {
        MetricsCmd::Show => {
            let metrics = charter::metrics_show(ctx)?;
            emit(format, &json!({ "ok": true, "metrics": metrics }))
        }
        MetricsCmd::Set {
            lines_of_code,
            unit_tests,
            integration_tests,
            coverage_percent,
        } => {
            let metrics = charter::metrics_set(
                ctx,
                charter::MetricsSetInput {
                    lines_of_code,
                    unit_tests,
                    integration_tests,
                    coverage_percent,
                },
            )?;
            emit(format, &json!({ "ok": true, "metrics": metrics }))
        }
    }
}
