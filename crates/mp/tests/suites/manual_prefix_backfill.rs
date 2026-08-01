//! M177 S3 — `mp migrate manual-prefix-backfill` CLI integration tests.

use std::path::Path;

use crate::common::TestEnv;

fn write_milestone_with_acs(dir: &Path, id: &str, acs: &[(&str, &str)]) {
    let ac_json: Vec<String> = acs
        .iter()
        .map(|(ac_id, ver)| {
            format!(
                r#"{{"id":"{ac_id}","description":"d","verification":"{ver}","status":"pending","evidence":""}}"#
            )
        })
        .collect();
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
    "created": "2026-07-15",
    "updated": "2026-07-15"
  }},
  "intent": {{ "outcome": "test" }},
  "problem": {{ "description": "smoke" }},
  "scope": {{
    "in_scope": ["x"],
    "out_of_scope": ["y", "z"]
  }},
  "acceptance_criteria": [{}],
  "design_decisions": [],
  "steps": [],
  "work_packages": []
}}"#,
        ac_json.join(",")
    );
    std::fs::write(dir.join(format!("{id}.json")), body).expect("write milestone");
}

fn read_verification(dir: &Path, id: &str, ac_id: &str) -> String {
    let body = std::fs::read_to_string(dir.join(format!("{id}.json"))).expect("read");
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    v["acceptance_criteria"]
        .as_array()
        .unwrap()
        .iter()
        .find(|ac| ac["id"] == ac_id)
        .and_then(|ac| ac["verification"].as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn manual_prefix_backfill_dry_run_previews_without_write() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone_with_acs(
        &milestones_dir,
        "alpha",
        &[
            ("AC-01", "cargo test -p mp"),
            (
                "AC-02",
                "crates/raul/tests/tui_view_state.rs (grep-based test)",
            ),
        ],
    );

    let out = env.run(&[
        "migrate",
        "manual-prefix-backfill",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["files_modified"], 0);
    assert_eq!(json["acs_rewritten"], 1);
    assert!(
        !read_verification(&milestones_dir, "alpha", "AC-02").starts_with("manual:"),
        "dry-run must not rewrite disk"
    );
}

#[test]
fn manual_prefix_backfill_yes_applies_and_is_idempotent() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone_with_acs(
        &milestones_dir,
        "beta",
        &[
            ("AC-01", "cargo test -p mp"),
            (
                "AC-02",
                "crates/raul/tests/keybinds.rs + rg for hardcoded legends",
            ),
        ],
    );

    let out1 = env.run(&[
        "migrate",
        "manual-prefix-backfill",
        "--yes",
        "--format",
        "json",
    ]);
    assert!(
        out1.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );
    let json1: serde_json::Value = serde_json::from_slice(&out1.stdout).unwrap();
    assert_eq!(json1["acs_rewritten"], 1);
    assert_eq!(json1["files_modified"], 1);
    let after = read_verification(&milestones_dir, "beta", "AC-02");
    assert!(after.starts_with("manual: "), "got {after}");
    assert!(after.contains("[manual-auto-prefix:"));
    assert_eq!(
        read_verification(&milestones_dir, "beta", "AC-01"),
        "cargo test -p mp",
        "runnable AC must not be rewritten"
    );

    let out2 = env.run(&[
        "migrate",
        "manual-prefix-backfill",
        "--yes",
        "--format",
        "json",
    ]);
    assert!(out2.status.success());
    let json2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(json2["acs_rewritten"], 0);
    assert_eq!(json2["idempotent_run"], true);
}

#[test]
fn manual_prefix_backfill_dry_run_wins_over_yes() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone_with_acs(
        &milestones_dir,
        "gamma",
        &[(
            "AC-01",
            "crates/foo/tests/bar.rs (integration harness check)",
        )],
    );

    let out = env.run(&[
        "migrate",
        "manual-prefix-backfill",
        "--dry-run",
        "--yes",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["files_modified"], 0);
    assert_eq!(json["acs_rewritten"], 1);
    assert!(
        !read_verification(&milestones_dir, "gamma", "AC-01").starts_with("manual:"),
        "dry-run must win over --yes"
    );
}
