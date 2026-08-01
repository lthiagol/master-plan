//! `mp activity` — bounded read of the project activity journal.
//!
//! M180: a machine-readable, projection-friendly read of the bounded
//! future-only event journal at `<plan_dir>/activity.json`. The read
//! shape is `{events: [...], total: N, cap: 500}` so consumers
//! (raul, scripts) can decide whether to consume `events` directly or
//! re-query with a different limit.
//!
//! The journal is lazy: a pre-M180 project with no `activity.json`
//! returns an empty `events` array (no backfill, no migration). See
//! [`crate::activity`] for the storage contract.

use anyhow::Result;
use serde_json::json;

use crate::activity;
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit_value;
use crate::paths::PlanContext;

pub(crate) fn cmd_activity(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    limit: Option<usize>,
) -> Result<()> {
    // Default cap matches the on-disk retention cap (500). Callers
    // can request fewer (raul's 5-row preview) but not more — the
    // hard cap is the journal itself, so any `limit > 500` is clamped
    // to 500.
    let requested = limit.unwrap_or(activity::ACTIVITY_RETENTION_CAP);
    let limit = requested.min(activity::ACTIVITY_RETENTION_CAP);
    let events = activity::read_recent_events(ctx, limit)?;
    let total = activity::load(ctx)?.len();
    let value = json!({
        "total": total,
        "cap": activity::ACTIVITY_RETENTION_CAP,
        "limit": limit,
        "events": events,
    });
    emit_value(format, &value, fields)
}
