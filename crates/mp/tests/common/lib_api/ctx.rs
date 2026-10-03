//! PlanContext builders + read-only `validate`.
//!
//! Pure helpers that don't perform plan mutations — they project a
//! `TestEnv` tempdir or a project root onto the [`PlanContext`] shape
//! the domain helpers expect, and run the plan validator in-process.

use std::path::Path;

use anyhow::{Context, Result};
use mp::paths::PlanContext;
use mp::validate::validate_plan;
use serde_json::Value;

/// Build a `PlanContext` from a path. The path is treated as the
/// `project_root`; the `plan_dir` is `project_root.join("master-plan")`.
///
/// Use this when the test owns a `TempDir` and has copied a fixture into
/// it. For tests that only need the workspace's own plan, prefer
/// [`ctx_for_workspace`].
pub fn ctx_for(project_root: &Path) -> Result<PlanContext> {
    Ok(PlanContext {
        project_root: project_root.to_path_buf(),
        plan_dir: project_root.join("master-plan"),
    })
}

/// Convenience: build a `PlanContext` from a `TestEnv`'s temp dir.
/// Equivalent to `ctx_for(env.tmp.path())` but reads more naturally in
/// tests.
pub fn ctx_for_env(env: &crate::common::TestEnv) -> Result<PlanContext> {
    ctx_for(env.tmp.path())
}

/// Build a `PlanContext` pointing at the workspace's own
/// `master-plan/` tree (the one this repo's mp CLI is configured
/// against). Use for tests that exercise the live repo plan
/// (e.g. the M161 `repo_plan_validates_via_mini_schema` style).
pub fn ctx_for_workspace() -> Result<PlanContext> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let project_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("workspace root")?
        .to_path_buf();
    Ok(PlanContext {
        project_root: project_root.clone(),
        plan_dir: project_root.join("master-plan"),
    })
}

/// `mp validate --format json` — runs the full plan validator and
/// returns its JSON report (same shape the CLI emits).
///
/// **Subprocess equivalent:** `env.run(&["validate", "--format", "json"])`.
pub fn validate(ctx: &PlanContext) -> Result<Value> {
    let report = validate_plan(ctx)?;
    Ok(serde_json::to_value(report)?)
}
