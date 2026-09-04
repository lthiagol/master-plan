//! M229 / AC-01: breaking-release preflight gate.
//!
//! Public-API removal is gated on two recorded preconditions. Without
//! both, the preflight refuses:
//!
//! 1. **Recorded next-major target version.** The breaking-release
//!    milestone (`M229`) carries an explicit `target_version` on
//!    disk, so the release registry is updated and the operator can
//!    point at the exact release that ships the cleanup.
//!
//! 2. **Migration-window evidence.** The plan's release registry has
//!    at least one entry with `status == "shipped"` whose milestone
//!    list includes both `M208` (the rename pivot, which introduced
//!    the deprecation alias) and `M219` (the deprecation warning).
//!    Without a shipped release that carried the warning, removing
//!    the warning now would skip the compatibility window the spec
//!    promises adopters.
//!
//! Once both preconditions are recorded, [`preflight`] returns
//! `{ok: true, …}` and removal work may proceed. The preflight is the
//! hard gate — [`crate::commands::breaking_release::cmd_breaking_release_apply`]
//! is the soft gate that records the audit trail (a typed
//! `breaking_release.json` marker under `<plan_dir>/.mp/`).
//!
//! The preflight is implemented as a pure function over
//! [`PlanContext`] so the unit tests in [`crate::breaking_release`] and
//! the CLI tests in `crates/mp/tests/watch_cli.rs` exercise the same
//! surface without going through the binary.

use serde::Serialize;

use crate::model::{PlanFile, ReleaseEntry};
use crate::paths::PlanContext;
use crate::store;

/// The breaking-release milestone id. The preflight resolves this
/// milestone's on-disk file to read its `target_version`.
pub const BREAKING_RELEASE_MILESTONE_ID: &str = "229";

/// Milestone ids that introduced the deprecation warning (M219) and
/// the rename alias (M208). Migration-window evidence requires at
/// least one shipped release covering BOTH of these.
pub const MIGRATION_WINDOW_MILESTONE_IDS: &[&str] = &["208", "219"];

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    /// True when both preconditions are satisfied.
    pub ok: bool,
    /// Resolved target release version (e.g. "3.0.0"). Empty when
    /// the milestone has no `target_version` recorded.
    pub target_version: String,
    /// Release-versions that carry the migration window evidence.
    /// Empty when no shipped release covers `MIGRATION_WINDOW_MILESTONE_IDS`.
    pub evidence_releases: Vec<String>,
    /// Ordered list of human-readable blockers. Empty when `ok` is true.
    pub blockers: Vec<String>,
}

impl PreflightReport {
    pub fn blockers_joined(&self) -> String {
        self.blockers.join("; ")
    }
}

#[derive(Debug)]
pub enum PreflightError {
    /// Underlying plan load or milestone file load failed.
    PlanLoad(String),
    /// Milestone file IO failed.
    Io(std::io::Error),
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreflightError::PlanLoad(s) => write!(f, "preflight plan load error: {s}"),
            PreflightError::Io(e) => write!(f, "preflight io error: {e}"),
        }
    }
}

impl std::error::Error for PreflightError {}

impl From<std::io::Error> for PreflightError {
    fn from(e: std::io::Error) -> Self {
        PreflightError::Io(e)
    }
}

impl From<anyhow::Error> for PreflightError {
    fn from(e: anyhow::Error) -> Self {
        PreflightError::PlanLoad(format!("{e}"))
    }
}

/// Run the breaking-release preflight. Always returns a [`PreflightReport`];
/// errors only signal that the plan or milestone file could not be read.
pub fn preflight(ctx: &PlanContext) -> std::result::Result<PreflightReport, PreflightError> {
    let plan = store::load_plan(ctx)?;
    let target_version = read_target_version(ctx);
    let evidence_releases = shipped_releases_covering(&plan, MIGRATION_WINDOW_MILESTONE_IDS);

    let mut blockers = Vec::new();
    if target_version.is_empty() {
        blockers.push(format!(
            "no recorded next-major target version on milestone M{BREAKING_RELEASE_MILESTONE_ID} (set with `mp milestone set-target-version {BREAKING_RELEASE_MILESTONE_ID} <X.Y.Z>`)"
        ));
    }
    if evidence_releases.is_empty() {
        blockers.push(format!(
            "no shipped release covers the migration window milestones {:?} (M208 + M219 must ship in at least one full release before the breaking cut)",
            MIGRATION_WINDOW_MILESTONE_IDS
        ));
    }

    Ok(PreflightReport {
        ok: blockers.is_empty(),
        target_version,
        evidence_releases,
        blockers,
    })
}

/// Read the target_version recorded on the breaking-release milestone.
/// Returns `""` when the milestone file is missing or the field is
/// unset (deferred state).
fn read_target_version(ctx: &PlanContext) -> String {
    let milestone_path = ctx
        .plan_dir
        .join("milestones")
        .join(format!("{BREAKING_RELEASE_MILESTONE_ID}-*.json"));
    let entries = match std::fs::read_dir(milestone_path.parent().unwrap_or(&ctx.plan_dir)) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&format!("{BREAKING_RELEASE_MILESTONE_ID}-"))
            || !name.ends_with(".json")
        {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(tv) = v
            .get("milestone")
            .and_then(|m| m.get("target_version"))
            .and_then(|t| t.as_str())
        {
            return tv.to_string();
        }
    }
    String::new()
}

/// Filter `plan.releases` to entries with `status == "shipped"` whose
/// milestone list covers every id in `required_ids`.
fn shipped_releases_covering(plan: &PlanFile, required_ids: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in &plan.releases {
        if !is_shipped(entry) {
            continue;
        }
        if required_ids
            .iter()
            .all(|id| entry.milestones.iter().any(|m| m == *id))
        {
            hits.push(entry.version.clone());
        }
    }
    hits
}

fn is_shipped(entry: &ReleaseEntry) -> bool {
    entry.status == "shipped"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanFile, ProjectMeta, ReleaseEntry};
    use std::path::Path;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    #[test]
    fn shipped_releases_covering_returns_only_shipped_with_all_required() {
        let plan = PlanFile {
            project: ProjectMeta {
                name: "x".into(),
                description: String::new(),
                stack: vec![],
                platforms: vec![],
                created: "2026-09-04".into(),
                target_version: String::new(),
                planning_status: "in-execution".into(),
                planning_phase: "charter".into(),
            },
            ..PlanFile::default()
        };
        let mut plan = plan;
        plan.releases = vec![
            ReleaseEntry {
                version: "2.0.0".into(),
                status: "shipped".into(),
                date: "2026-07-04".into(),
                milestones: vec!["208".into(), "219".into()],
            },
            ReleaseEntry {
                version: "1.9.0".into(),
                status: "shipped".into(),
                date: "2026-05-01".into(),
                milestones: vec!["208".into()],
            },
            ReleaseEntry {
                version: "3.0.0".into(),
                status: "planned".into(),
                date: String::new(),
                milestones: vec!["208".into(), "219".into(), "229".into()],
            },
        ];

        let got = shipped_releases_covering(&plan, MIGRATION_WINDOW_MILESTONE_IDS);
        assert_eq!(got, vec!["2.0.0".to_string()]);
    }

    #[test]
    fn preflight_refuses_when_target_version_is_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ctx_in(dir.path());
        std::fs::create_dir_all(ctx.plan_dir.clone()).unwrap();
        std::fs::write(
            ctx.plan_dir.join("plan.json"),
            r#"{"project":{"name":"x","description":"","stack":[],"platforms":[],"created":"2026-09-04","target_version":"","planning_status":"in-execution","planning_phase":"charter"},"charter":{"goals":[],"non_goals":[],"deferred":[],"principles":[]},"metrics":{"lines_of_code":0,"unit_tests":0,"integration_tests":0,"coverage_percent":0.0,"checked_at":"2026-09-04"},"execution":{"strategy":"resume_then_ready","interleave":"milestone","mode":"autonomous","handoff_at":"","handoff_by":"","focus_milestone":"","focus_through_step":"","adoption_order":[],"handoff_changed_milestones":[],"handoff_baseline":{}},"milestones":[],"releases":[]}"#,
        )
        .unwrap();
        // No milestone file on disk → target_version reads as "".
        let report = preflight(&ctx).unwrap();
        assert!(!report.ok);
        assert!(report.target_version.is_empty());
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("no recorded next-major target version")));
    }

    #[test]
    fn preflight_blocks_report_lists_each_blocker_once() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ctx_in(dir.path());
        std::fs::create_dir_all(ctx.plan_dir.clone()).unwrap();
        std::fs::write(
            ctx.plan_dir.join("plan.json"),
            r#"{"project":{"name":"x","description":"","stack":[],"platforms":[],"created":"2026-09-04","target_version":"","planning_status":"in-execution","planning_phase":"charter"},"charter":{"goals":[],"non_goals":[],"deferred":[],"principles":[]},"metrics":{"lines_of_code":0,"unit_tests":0,"integration_tests":0,"coverage_percent":0.0,"checked_at":"2026-09-04"},"execution":{"strategy":"resume_then_ready","interleave":"milestone","mode":"autonomous","handoff_at":"","handoff_by":"","focus_milestone":"","focus_through_step":"","adoption_order":[],"handoff_changed_milestones":[],"handoff_baseline":{}},"milestones":[],"releases":[]}"#,
        )
        .unwrap();
        let report = preflight(&ctx).unwrap();
        assert!(!report.blockers.is_empty());
        let joined = report.blockers_joined();
        assert!(joined.contains("next-major"));
        assert!(joined.contains("migration window"));
    }
}
