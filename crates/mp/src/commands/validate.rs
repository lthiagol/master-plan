use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

use crate::activity;
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit_value;
use crate::paths::PlanContext;
use crate::validate;

pub(crate) fn cmd_validate(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    summary: bool,
) -> Result<()> {
    let report = validate::validate_plan(ctx)?;
    // M142 AC-06: L5 violations are advisory and never gate the
    // exit code. The exit code is governed by `errors` (gate-driven
    // issues) only; advisory L5 violations show up in `l5_audit`
    // but do not flip `ok` to false.
    let code = if report.ok { 0 } else { 2 };

    // M180 S6: compare the current validation result against the
    // most recent `validation-state` event in the journal and emit
    // a new event only when the result changed. Same-state runs
    // touch the journal only on the read side, preserving the
    // pre-M180 "validate is a pure read" contract that golden
    // scenarios (fs_unchanged=true) rely on.
    let _ = activity::record_validation_state_change(ctx, report.ok, report.errors.len())?;

    let value: serde_json::Value = if summary {
        let mut warnings_by_code: HashMap<String, usize> = HashMap::new();
        let mut errors_by_code: HashMap<String, usize> = HashMap::new();
        for w in &report.warnings {
            *warnings_by_code.entry(w.code.clone()).or_insert(0) += 1;
        }
        for e in &report.errors {
            *errors_by_code.entry(e.code.clone()).or_insert(0) += 1;
        }
        // M142: surface the L5 audit advisory count alongside the
        // gate counts, but never as part of `error_count`.
        let mut summary_json = json!({
            "ok": report.ok,
            "error_count": report.errors.len(),
            "warning_count": report.warnings.len(),
            "warnings_by_code": warnings_by_code,
            "errors_by_code": errors_by_code,
        });
        if let Some(l5) = &report.l5_audit {
            summary_json["l5_audit"] = json!({
                "ok": l5.ok,
                "violation_count": l5.violation_count,
                "milestones_with_violations": l5
                    .milestones
                    .iter()
                    .filter(|m| !m.ok)
                    .map(|m| m.milestone_id.clone())
                    .collect::<Vec<_>>(),
            });
        }
        summary_json
    } else {
        serde_json::to_value(&report)?
    };

    emit_value(format, &value, fields)?;
    if code != 0 {
        return Err(anyhow::Error::new(crate::ExitCode(code)));
    }
    Ok(())
}
