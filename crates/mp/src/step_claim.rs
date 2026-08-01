use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};

use crate::milestone::{self, load_milestone_path};
use crate::model::Step;
use crate::paths::PlanContext;
use crate::store;

pub fn claim_step(
    ctx: &PlanContext,
    milestone_id: &str,
    step_id: &str,
    claimed_by: &str,
    lease: Option<&str>,
) -> Result<Step> {
    if claimed_by.trim().is_empty() {
        bail!("--by is required");
    }
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    let step = m
        .steps
        .iter_mut()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found"))?;

    let now = store::now_rfc3339();
    step.claimed_by = claimed_by.to_string();
    step.claimed_at = now.clone();
    step.lease_expires_at = match lease {
        Some(raw) => parse_lease_expiry(&now, raw)?,
        None => String::new(),
    };

    let out = step.clone();
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(out)
}

pub fn release_step(ctx: &PlanContext, milestone_id: &str, step_id: &str) -> Result<Step> {
    let path = load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    let step = m
        .steps
        .iter_mut()
        .find(|s| s.id == step_id)
        .with_context(|| format!("step {step_id} not found"))?;
    clear_claim(step);
    let out = step.clone();
    m.milestone.updated = store::today();
    milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(out)
}

pub fn clear_claim(step: &mut Step) {
    step.claimed_by.clear();
    step.claimed_at.clear();
    step.lease_expires_at.clear();
}

pub fn step_claim_active(step: &Step) -> bool {
    if step.claimed_by.is_empty() {
        return false;
    }
    if step.lease_expires_at.is_empty() {
        return true;
    }
    if let Ok(expires) = DateTime::parse_from_rfc3339(&step.lease_expires_at) {
        return expires.with_timezone(&Utc) > Utc::now();
    }
    true
}

pub fn claim_json(step: &Step) -> Option<serde_json::Value> {
    if !step_claim_active(step) {
        return None;
    }
    Some(serde_json::json!({
        "claimed_by": step.claimed_by,
        "claimed_at": step.claimed_at,
        "lease_expires_at": if step.lease_expires_at.is_empty() { serde_json::Value::Null } else { serde_json::json!(step.lease_expires_at) },
    }))
}

fn parse_lease_expiry(from: &str, lease: &str) -> Result<String> {
    let start = DateTime::parse_from_rfc3339(from)
        .with_context(|| format!("invalid claim timestamp: {from}"))?;
    let duration = parse_duration(lease)?;
    Ok((start.with_timezone(&Utc) + duration).to_rfc3339())
}

fn parse_duration(raw: &str) -> Result<Duration> {
    let s = raw.trim();
    if let Some(mins) = s.strip_suffix('m').or_else(|| s.strip_suffix("min")) {
        let n: i64 = mins.parse().context("invalid lease minutes")?;
        return Ok(Duration::minutes(n));
    }
    if let Some(hours) = s.strip_suffix('h') {
        let n: i64 = hours.parse().context("invalid lease hours")?;
        return Ok(Duration::hours(n));
    }
    if let Some(days) = s.strip_suffix('d') {
        let n: i64 = days.parse().context("invalid lease days")?;
        return Ok(Duration::days(n));
    }
    bail!("invalid lease duration: {raw} (use 30m, 2h, 1d)");
}
