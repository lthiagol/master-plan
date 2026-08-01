//! M92 AC-01: TOML → JSON migration converts a plan dir with zero validate
//! errors and content equivalence on the normalized struct.
//!
//! Also covers the AC-01 path-resolution claim (milestones/{id}-{slug}.json).

use std::fs;

use crate::common::TestEnv;

/// Build a small TOML-on-disk plan dir (pre-migration shape, with M82 ceremony
/// fields present) and verify the migrate module produces a validating JSON plan.
#[test]
fn migrate_converts_toml_plan_dir_to_valid_json() {
    let env = TestEnv::blank();
    let plan_dir = env.tmp.path().join("master-plan");
    let ms_dir = plan_dir.join("milestones");
    fs::create_dir_all(&ms_dir).unwrap();

    // plan index (TOML)
    fs::write(
        plan_dir.join("plan.toml"),
        r#"[project]
name = "demo"
created = "2026-07-02"
planning_status = "ready-for-execution"
planning_phase = "execution"
"#,
    )
    .unwrap();
    // config (TOML)
    fs::write(
        plan_dir.join("config.toml"),
        r#"[workflow]
profile = "full"
[workflow.plan]
location = "master-plan"
"#,
    )
    .unwrap();
    // milestone with M82-dropped ceremony fields present — migration must drop them.
    fs::write(
        ms_dir.join("01-demo.toml"),
        r#"[milestone]
id = "01"
title = "Demo"
slug = "demo"
spec_status = "verified"
execution_status = "done"
effort = "S"
risk = "low"
created = "2026-07-02"
updated = "2026-07-02"

[intent]
outcome = "demo outcome"

[problem]
description = "demo problem"

[scope]
in_scope = ["a"]
out_of_scope = ["b", "c"]

[verification]
date = "2026-07-02"
branch = ""
evidence = "shipped"

[[acceptance_criteria]]
id = "AC-01"
description = "works"
verification = "manual: accepted — test"
status = "passed"
evidence = "test passed"

# M82-dropped ceremony fields — must NOT survive into JSON.
[behavior]
scenarios = []
edge_cases = []

[context]
related = []
references = []

success_criteria = []
assumptions = []
risks = []
follow_ups = []
"#,
    )
    .unwrap();

    let report = mp::migrate::migrate_plan_dir(&plan_dir).expect("migrate");
    assert!(!report.converted.is_empty(), "should convert files");
    assert!(
        report.skipped.is_empty(),
        "nothing skipped: {:?}",
        report.skipped
    );

    // No .toml remains under the plan dir.
    assert!(no_toml_under(&plan_dir));

    // Converted files include the milestone and the plan index.
    let ms_json = ms_dir.join("01-demo.json");
    let plan_json = plan_dir.join("plan.json");
    assert!(ms_json.exists(), "milestone json missing");
    assert!(plan_json.exists(), "plan json missing");

    // M82 ceremony fields dropped from the migrated milestone JSON.
    let ms_content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ms_json).unwrap()).unwrap();
    for dropped in [
        "behavior",
        "context",
        "success_criteria",
        "assumptions",
        "risks",
        "follow_ups",
    ] {
        assert!(
            ms_content.get(dropped).is_none(),
            "dropped ceremony field {dropped} survived migration"
        );
    }
    // Load-bearing fields preserved.
    assert_eq!(ms_content["milestone"]["id"].as_str().unwrap(), "01");
    assert_eq!(ms_content["milestone"]["title"].as_str().unwrap(), "Demo");

    // The migrated plan validates cleanly via mp (point mp at the temp plan dir).
    let out = std::process::Command::new(crate::common::mp_bin())
        .arg("--plan-dir")
        .arg(&plan_dir)
        .env("MP_HOME", crate::common::repo_root())
        .args(["validate", "--format", "json"])
        .output()
        .expect("run mp validate");
    assert!(
        out.status.success(),
        "validate failed: stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"].as_bool(), Some(true));
    assert!(v["errors"].as_array().unwrap().is_empty());
}

/// Re-running migrate on an already-JSON plan dir is a no-op (idempotent).
#[test]
fn migrate_is_idempotent_on_json_plan_dir() {
    let env = TestEnv::blank();
    let plan_dir = env.tmp.path().join("master-plan");
    fs::create_dir_all(plan_dir.join("milestones")).unwrap();
    fs::write(
        plan_dir.join("plan.json"),
        r#"{"project":{"name":"x","created":"2026-07-02"}}"#,
    )
    .unwrap();

    let report = mp::migrate::migrate_plan_dir(&plan_dir).expect("migrate");
    assert!(
        report.is_empty(),
        "re-run should convert nothing: {:?}",
        report
    );
    assert!(no_toml_under(&plan_dir));
}

/// A milestone decode error fails before any JSON is written (no half-converted tree).
#[test]
fn migrate_fails_fast_without_partial_writes() {
    let env = TestEnv::blank();
    let plan_dir = env.tmp.path().join("master-plan");
    let ms_dir = plan_dir.join("milestones");
    fs::create_dir_all(&ms_dir).unwrap();
    fs::write(
        plan_dir.join("plan.toml"),
        r#"[project]
name = "demo"
created = "2026-07-02"
"#,
    )
    .unwrap();
    fs::write(
        ms_dir.join("01-bad.toml"),
        r#"[milestone]
id = "01"
title = "Bad"
# missing required sections — decode should fail
"#,
    )
    .unwrap();

    let err = mp::migrate::migrate_plan_dir(&plan_dir).unwrap_err();
    assert!(
        err.to_string().contains("decode milestone"),
        "expected decode error, got: {err}"
    );
    assert!(
        plan_dir.join("plan.toml").exists(),
        "plan.toml must remain on failure"
    );
    assert!(
        !plan_dir.join("plan.json").exists(),
        "plan.json must not be written on failure"
    );
    assert!(
        ms_dir.join("01-bad.toml").exists(),
        "bad milestone toml must remain"
    );
    assert!(
        !ms_dir.join("01-bad.json").exists(),
        "bad milestone json must not be written"
    );
}

fn no_toml_under(dir: &std::path::Path) -> bool {
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.unwrap();
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("toml")
        {
            return false;
        }
    }
    true
}
