//! M229: breaking-release cleanup. This module owns the canonical
//! black-box coverage for AC-01 (preflight refusal + acceptance
//! fixture) and AC-02 (absence of `mp watch` / `mp watch-control`
//! and the legacy autopilot migrate verb from the canonical
//! surface after the cleanup).
//!
//! Originally the M149 / M208 `mp watch` CLI tests lived here; M229
//! rewrote them to test the absence + the preflight gate. The
//! preflight acceptance fixture used by AC-01 is written into a
//! scratch plan directory and never touches the live plan.

mod common;

use common::TestEnv;
use serde_json::Value;
use std::path::Path;

fn ctx_for_dir(dir: &Path) -> mp::paths::PlanContext {
    mp::paths::PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

fn write_release_fixture(plan_dir: &Path, releases: Value) {
    let payload = serde_json::json!({
        "project": {
            "name": "test",
            "description": "",
            "stack": [],
            "platforms": [],
            "created": "2026-09-04",
            "target_version": "",
            "planning_status": "in-execution",
            "planning_phase": "charter"
        },
        "charter": {"goals": [], "non_goals": [], "deferred": [], "principles": []},
        "metrics": {
            "lines_of_code": 0,
            "unit_tests": 0,
            "integration_tests": 0,
            "coverage_percent": 0.0,
            "checked_at": "2026-09-04"
        },
        "execution": {
            "strategy": "resume_then_ready",
            "interleave": "milestone",
            "mode": "autonomous",
            "handoff_at": "",
            "handoff_by": "",
            "focus_milestone": "",
            "focus_through_step": "",
            "adoption_order": [],
            "handoff_changed_milestones": [],
            "handoff_baseline": {}
        },
        "milestones": [],
        "releases": releases
    });
    std::fs::create_dir_all(plan_dir).unwrap();
    std::fs::write(
        plan_dir.join("plan.json"),
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .unwrap();
}

fn write_milestone_229(plan_dir: &Path, target_version: &str) {
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).unwrap();
    let payload = serde_json::json!({
        "milestone": {
            "id": "229",
            "target_version": target_version,
            "lifecycle": "approved",
            "spec_status": "ready",
            "execution_status": "planned"
        }
    });
    std::fs::write(
        milestones_dir.join("229-fixture.json"),
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .unwrap();
}

// ─── AC-01: preflight refuses without recorded target version
// ─── AC-01: preflight refuses without a shipped release covering the
//     migration window (M208 + M219)
// ─── AC-01: preflight accepts when both gates are recorded

#[test]
fn preflight_refuses_when_target_version_is_empty() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_release_fixture(&ctx.plan_dir, serde_json::json!([]));
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(!report.ok, "fresh fixture must refuse");
    assert!(report.target_version.is_empty());
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.contains("no recorded next-major target version")),
        "expected target-version blocker; got {:?}",
        report.blockers
    );
}

#[test]
fn preflight_refuses_when_no_shipped_release_covers_migration_window() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        !report.ok,
        "missing M219 in the shipped release must refuse; got {:?}",
        report.blockers
    );
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.contains("migration window")),
        "expected migration-window blocker"
    );
}

#[test]
fn preflight_refuses_when_release_status_is_planned_only() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "3.0.0", "status": "planned", "date": "", "milestones": ["208", "219"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        !report.ok,
        "planned-only release should not satisfy migration-window evidence"
    );
}

#[test]
fn preflight_accepts_when_target_version_and_shipped_release_are_recorded() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208", "219"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        report.ok,
        "valid release fixture must accept; blockers={:?}",
        report.blockers
    );
    assert_eq!(report.target_version, "3.0.0");
    assert_eq!(report.evidence_releases, vec!["2.0.0"]);
    assert!(report.blockers.is_empty());
}

#[test]
fn preflight_cli_exposes_status_to_operators() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    // Create the live plan fixture in the test env so the CLI can find it.
    let plan_dir = ctx.plan_dir.clone();
    write_milestone_229(&plan_dir, "3.0.0");
    write_release_fixture(
        &plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208", "219"]}
        ]),
    );
    let out = env.run(&[
        "--plan-dir",
        plan_dir.to_str().unwrap(),
        "breaking-release",
        "preflight",
    ]);
    assert!(
        out.status.success(),
        "preflight should exit 0 even when refusing; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["ok"], Value::Bool(true));
    assert_eq!(v["target_version"], Value::String("3.0.0".to_string()));
    assert_eq!(v["evidence_releases"], serde_json::json!(["2.0.0"]));
}

// ─── AC-02: post-removal absence tests. The legacy `mp watch`,
// `mp watch-control`, and `mp autopilot migrate` commands must be
// gone (or surface unknown-command errors) after the S2 cleanup.

#[test]
fn watch_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["watch", "--help"]);
    assert!(
        !out.status.success(),
        "mp watch must be removed; got exit 0 with stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), stderr);
    assert!(
        combined.to_lowercase().contains("unknown")
            || combined.contains("invalid subcommand")
            || combined.contains("unrecognized"),
        "mp watch must surface an unknown-command error; got stderr={stderr}"
    );
}

#[test]
fn watch_control_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["watch-control", "status"]);
    assert!(
        !out.status.success(),
        "mp watch-control must be removed; got exit 0"
    );
}

#[test]
fn autopilot_migrate_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "migrate"]);
    assert!(
        !out.status.success(),
        "mp autopilot migrate must be removed; got exit 0"
    );
}

#[test]
fn autopilot_help_no_longer_advertises_migrate_verb() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "--help"]);
    assert!(out.status.success(), "autopilot --help must still exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    let doc = v
        .get("tree")
        .map(|t| t.to_string())
        .unwrap_or_else(|| stdout.clone());
    // The migrate verb must not be present in any help output.
    assert!(
        !doc.contains("migrate") || doc.contains("migrate-role"),
        "autopilot --help must not advertise a 'migrate' verb; got {doc}"
    );
}
