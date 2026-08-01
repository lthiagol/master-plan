//! M177 S8 — `mp edit strip-deferred-reason` integration tests.

use std::path::Path;

use crate::common::TestEnv;

fn write_milestone(dir: &Path, id: &str, deferred: bool, reason: &str) {
    let body = format!(
        r#"{{
  "milestone": {{
    "id": "{id}",
    "title": "Sample {id}",
    "slug": "sample-{id}",
    "spec_status": "ready",
    "execution_status": "planned",
    "lifecycle": "approved",
    "depends_on": [],
    "priority": "low",
    "risk": "low",
    "effort": "S",
    "deferred": {deferred},
    "deferred_reason": "{reason}",
    "blocked": false,
    "block_reason": "",
    "cancelled": false,
    "created": "2026-07-15",
    "updated": "2026-07-15"
  }},
  "intent": {{ "outcome": "test" }},
  "problem": {{ "description": "smoke" }},
  "scope": {{
    "in_scope": ["x"],
    "out_of_scope": ["y", "z"]
  }},
  "acceptance_criteria": [],
  "design_decisions": [],
  "steps": [],
  "work_packages": [],
  "verification": {{ "branch": "", "date": "", "evidence": "" }}
}}"#
    );
    std::fs::write(dir.join(format!("{id}.json")), body).expect("write milestone");
}

fn read_reason(dir: &Path, id: &str) -> String {
    let body = std::fs::read_to_string(dir.join(format!("{id}.json"))).expect("read");
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    v["milestone"]["deferred_reason"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn strip_deferred_reason_clears_stale_text() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "158", false, "Not adopting in this cycle…");

    let out = env.run(&["edit", "strip-deferred-reason", "--yes", "--format", "json"]);
    assert!(
        out.status.success(),
        "strip-deferred-reason failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["files_modified"], 1);
    assert_eq!(json["idempotent_run"], false);
    assert_eq!(read_reason(&milestones_dir, "158"), "");
}

#[test]
fn strip_deferred_reason_is_idempotent() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "alpha", false, "stale rationale");

    let out1 = env.run(&["edit", "strip-deferred-reason", "--yes", "--format", "json"]);
    assert!(out1.status.success());
    let out2 = env.run(&["edit", "strip-deferred-reason", "--yes", "--format", "json"]);
    assert!(out2.status.success());
    let json2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(json2["files_modified"], 0);
    assert_eq!(json2["idempotent_run"], true);
    assert_eq!(read_reason(&milestones_dir, "alpha"), "");
}

#[test]
fn strip_deferred_reason_dry_run_previews_without_write() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "beta", false, "preview me");

    let out = env.run(&[
        "edit",
        "strip-deferred-reason",
        "--dry-run",
        "--yes",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["files_modified"], 0, "dry-run never writes");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["idempotent_run"], false);
    assert!(
        json["removed_by_file"]
            .as_object()
            .map(|o| !o.is_empty())
            .unwrap_or(false),
        "dry-run must still report candidates in removed_by_file"
    );
    assert_eq!(
        read_reason(&milestones_dir, "beta"),
        "preview me",
        "dry-run must not clear deferred_reason"
    );
}

#[test]
fn strip_deferred_reason_preserves_active_deferral() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "gamma", true, "still deferred");

    let out = env.run(&["edit", "strip-deferred-reason", "--yes", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["files_modified"], 0);
    assert_eq!(read_reason(&milestones_dir, "gamma"), "still deferred");
}

#[test]
fn strip_deferred_reason_noop_on_empty_reason() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "delta", false, "");

    let out = env.run(&["edit", "strip-deferred-reason", "--yes", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["files_modified"], 0);
    assert_eq!(json["idempotent_run"], true);
}
