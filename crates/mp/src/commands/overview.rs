//! `mp overview` — consolidated project-health snapshot.
//!
//! M180: one structured read that aggregates health, lifecycle / step
//! statistics, work-queue counts, watch state, suggested-path preview,
//! Inbox preview, and recent activity. Raul consumes this single
//! payload instead of fanning out subprocesses.
//!
//! The shape is built by [`crate::overview::build_overview`]; this
//! command module just owns the CLI dispatch, `--fields` projection,
//! and `--summary` mode (a smaller payload for tab headers / status
//! strips).
//!
//! ## `mp overview` vs `mp status`
//!
//! `mp status` continues to expose the legacy lane / milestone
//! rollup; `mp overview` is the new mp-owned read model designed for
//! Raul's Overview tab and any other modern client. Both shapes
//! coexist — `mp status` is wire-stable for backcompat.

use anyhow::Result;
use serde_json::Value;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit_value;
use crate::overview;
use crate::paths::PlanContext;

pub(crate) fn cmd_overview(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    summary: bool,
) -> Result<()> {
    let snapshot = overview::build_overview(ctx)?;
    let value = if summary {
        overview::to_summary(&snapshot)
    } else {
        serde_json::to_value(&snapshot)?
    };
    emit_value(format, &value, fields)
}

/// `mp overview --format raw` returns the in-memory snapshot (typed)
/// pretty-printed; useful for debugging.
#[allow(dead_code)]
pub(crate) fn cmd_overview_raw(ctx: &PlanContext) -> Result<Value> {
    let snapshot = overview::build_overview(ctx)?;
    Ok(serde_json::to_value(&snapshot)?)
}
