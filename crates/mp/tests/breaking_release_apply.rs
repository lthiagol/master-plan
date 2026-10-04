//! M233 / AC-02: `mp breaking-release apply` direct coverage.
//!
//! Two behavioral tests covering the production branches in
//! `crates/mp/src/commands/breaking_release.rs::cmd_breaking_release_apply`:
//!
//! 1. `apply_after_blocked_preflight_returns_nonzero_with_bail_message`
//!    — sets up a plan whose preflight MUST refuse (no shipped
//!    release covers both M208 and M219), runs `apply`, asserts
//!    non-zero exit AND that stderr carries the
//!    "breaking-release preflight refuses to apply" bail wording.
//!    Verifies the `anyhow::bail!` arm at lines 63-71.
//! 2. `apply_after_clean_preflight_writes_marker_file` — sets up a
//!    valid preflight (M229 target_version recorded, shipped release
//!    covering both milestones), runs `apply`, asserts exit 0 AND
//!    `<plan_dir>/.mp/breaking_release.json` is written with the
//!    expected fields. Verifies the write-marker arm at lines 72-83.
//!
//! **No production-code changes** — the apply function is unchanged;
//! these tests only exercise the branches the AC requires.
//!
//! Fixture helpers are local copies of the patterns in
//! `crates/mp/tests/watch_cli.rs` (those are private to that binary).

mod common;

use crate::common::TestEnv;
use serde_json::Value;
use std::path::Path;

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

/// 1. Blocked preflight → apply exits non-zero with the bail message.
///    Verifies `anyhow::bail!` at lines 63-71 of breaking_release.rs.
#[test]
fn apply_after_blocked_preflight_returns_nonzero_with_bail_message() {
    let env = TestEnv::new();
    let plan_dir = env.tmp.path().join("master-plan");

    // M229 target_version is recorded BUT no shipped release covers
    // both M208 and M219, so the migration-window blocker fires and
    // `apply` must refuse.
    write_milestone_229(&plan_dir, "3.0.0");
    write_release_fixture(
        &plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208"]}
        ]),
    );

    let out = env.run(&[
        "breaking-release",
        "apply",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "apply must fail when preflight blockers exist; stderr={stderr}"
    );

    // The bail message must surface unchanged so operators see the
    // actionable text. The exact substring is the literal the
    // production code uses at line 64.
    assert!(
        stderr.contains("breaking-release preflight refuses to apply"),
        "stderr must carry the bail message; got: {stderr}"
    );

    // The marker file MUST NOT be written when apply refuses.
    let marker = plan_dir.join(".mp").join("breaking_release.json");
    assert!(
        !marker.exists(),
        "marker file must not exist when apply refuses: {}",
        marker.display()
    );
}

/// 2. Clean preflight → apply exits 0 and writes the marker file with
///    the expected fields. Verifies the write-marker arm at lines
///    72-83 of breaking_release.rs.
#[test]
fn apply_after_clean_preflight_writes_marker_file() {
    let env = TestEnv::new();
    let plan_dir = env.tmp.path().join("master-plan");

    write_milestone_229(&plan_dir, "3.0.0");
    write_release_fixture(
        &plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208", "219"]}
        ]),
    );

    let out = env.run(&[
        "breaking-release",
        "apply",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "apply must succeed when preflight is green; stderr={stderr}"
    );

    // Marker file lives at <plan_dir>/.mp/breaking_release.json.
    let marker = plan_dir.join(".mp").join("breaking_release.json");
    assert!(
        marker.exists(),
        "marker file must be written when apply succeeds: {}",
        marker.display()
    );

    let body = std::fs::read_to_string(&marker).unwrap();
    let payload: Value = serde_json::from_str(&body).expect("marker JSON");
    assert_eq!(
        payload["ok"].as_bool(),
        Some(true),
        "marker payload.ok must be true"
    );
    assert!(
        payload["applied_at"].as_str().is_some(),
        "marker must carry applied_at (RFC3339)"
    );
    assert_eq!(
        payload["target_version"].as_str(),
        Some("3.0.0"),
        "marker must carry the recorded target_version"
    );
    let evidence = payload["evidence_releases"].as_array().expect("evidence_releases");
    assert!(
        evidence
            .iter()
            .any(|v| v.as_str() == Some("2.0.0")),
        "marker must list 2.0.0 in evidence_releases; got {evidence:?}"
    );
}