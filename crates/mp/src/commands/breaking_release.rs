//! M229 / AC-01: `mp breaking-release preflight` and `mp breaking-release apply`.
//!
//! The preflight is a read-only check that returns
//! `{ok: bool, blockers: [...], target_version, evidence_releases}` based
//! on the recorded target version + migration-window evidence. `apply`
//! is the soft gate that records the breaking-release marker in
//! `<plan_dir>/.mp/breaking_release.json` so audit reads can confirm
//! the preflight was satisfied before removal work shipped.

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;

use crate::breaking_release::preflight;
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub struct BreakingReleaseApplyReport {
    pub ok: bool,
    pub marker_path: PathBuf,
    pub target_version: String,
    pub evidence_releases: Vec<String>,
    pub blockers: Vec<String>,
}

/// Top-level dispatch.
pub(crate) fn cmd_breaking_release(
    ctx: &PlanContext,
    cmd: crate::cli::BreakingReleaseCmd,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    match cmd {
        crate::cli::BreakingReleaseCmd::Preflight => cmd_breaking_release_preflight(ctx, format),
        crate::cli::BreakingReleaseCmd::Apply => cmd_breaking_release_apply(ctx, format),
    }
}

/// `mp breaking-release preflight` — read-only gate check.
pub(crate) fn cmd_breaking_release_preflight(
    ctx: &PlanContext,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let report = preflight(ctx)?;
    let payload = json!({
        "ok": report.ok,
        "target_version": report.target_version,
        "evidence_releases": report.evidence_releases,
        "blockers": report.blockers,
    });
    emit(format, &payload)
}

/// `mp breaking-release apply` — record the marker file so subsequent
/// removal work has audit evidence the preflight was satisfied.
pub(crate) fn cmd_breaking_release_apply(
    ctx: &PlanContext,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let report = preflight(ctx)?;
    if !report.ok {
        anyhow::bail!(
            "breaking-release preflight refuses to apply: {} \
             (record an explicit next-major target version on milestone \
              M229 and ensure at least one shipped release covers both M208 \
              and M219 before re-running)",
            report.blockers_joined()
        );
    }
    let marker_path = ctx.plan_dir.join(".mp").join("breaking_release.json");
    let payload = json!({
        "ok": true,
        "applied_at": store::now_rfc3339(),
        "target_version": report.target_version,
        "evidence_releases": report.evidence_releases,
    });
    std::fs::create_dir_all(marker_path.parent().unwrap())
        .map_err(|e| anyhow::anyhow!("create marker parent: {e}"))?;
    let serialized = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&marker_path, format!("{serialized}\n"))
        .map_err(|e| anyhow::anyhow!("write marker {}: {e}", marker_path.display()))?;
    let out = BreakingReleaseApplyReport {
        ok: true,
        marker_path,
        target_version: report.target_version,
        evidence_releases: report.evidence_releases,
        blockers: Vec::new(),
    };
    emit(format, &out)
}
