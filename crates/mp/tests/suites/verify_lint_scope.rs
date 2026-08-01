//! M106 AC-03 / M110 (S2): regression tests for `mp plan verify-lint`.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::common::{mp_bin, repo_root};

fn run_lint(plan_dir: &Path) -> (i32, String, Value) {
    let project_root = plan_dir
        .parent()
        .unwrap_or_else(|| plan_dir.parent().unwrap());
    let out = Command::new(mp_bin())
        .current_dir(project_root)
        .env("MP_HOME", repo_root())
        .arg("--plan-dir")
        .arg(plan_dir)
        .args(["plan", "verify-lint"])
        .output()
        .expect("run mp plan verify-lint");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let json: Value =
        serde_json::from_slice(&out.stdout).expect("verify-lint emits JSON on stdout");
    (code, stderr, json)
}

fn warning_files(report: &Value) -> Vec<String> {
    report["warnings"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|w| w["milestone_file"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn warning_values(report: &Value) -> Vec<String> {
    report["warnings"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|w| w["value"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn lint_does_not_flag_m104_after_scope_cleanup() {
    let plan_dir = repo_root().join("master-plan");
    let (code, _stderr, report) = run_lint(&plan_dir);

    assert_eq!(code, 0, "verify-lint must always exit 0");
    assert_eq!(report["ok"], true);
    let files = warning_files(&report);
    assert!(
        !files
            .iter()
            .any(|f| f.contains("104-release-blocker-sweep")),
        "M104 should NOT be flagged after the M106 scope cleanup; warnings={files:?}"
    );
}

#[test]
fn lint_flags_synthetic_broad_scope_fixture() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let bad = r#"{
  "milestone": {"id":"999","slug":"bad","title":"Bad","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-04","updated":"2026-07-04"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"cargo test --workspace && mp validate","evidence":"","status":"pending"}]
}"#;
    std::fs::write(milestones.join("999-broad-scope-fixture.json"), bad).expect("write fixture");

    let (code, stderr, report) = run_lint(&plan_dir);

    assert_eq!(code, 0, "lint must always exit 0 (WARN, never FAIL)");
    assert!(
        warning_files(&report)
            .iter()
            .any(|f| f.contains("999-broad-scope-fixture")),
        "lint must surface the broad-scope synthetic fixture; report={report}"
    );
    assert!(
        stderr.contains("WARN:"),
        "human WARN lines go to stderr; stderr=\n{stderr}"
    );
}

#[test]
fn edge_cases() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let fixture = r#"{
  "milestone": {
    "id": "997",
    "slug": "edge-cases-fixture",
    "title": "Edge cases for verify-lint",
    "lifecycle": "draft",
    "effort": "S",
    "risk": "low",
    "spec_status": "draft",
    "execution_status": "planned",
    "depends_on": [],
    "created": "2026-07-04",
    "updated": "2026-07-04"
  },
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [
    {
      "id": "AC-1",
      "description": "make test at end-of-string",
      "verification": "echo prefix && make test",
      "evidence": "",
      "status": "pending"
    },
    {
      "id": "AC-2",
      "description": "escaped quotes around cargo test --workspace",
      "verification": "sh -c \"echo hello && cargo test --workspace\"",
      "evidence": "",
      "status": "pending"
    },
    {
      "id": "AC-3",
      "description": "clean scoped (negative control)",
      "verification": "cargo test -p mp --test sort_regression",
      "evidence": "",
      "status": "pending"
    },
    {
      "id": "AC-4",
      "description": "make test followed by semicolon (M108 ER-1)",
      "verification": "echo a; make test; echo b",
      "evidence": "",
      "status": "pending"
    }
  ],
  "steps": [
    {
      "id": "S1",
      "action": "scoped",
      "status": "pending",
      "tests": "cargo test -p mp --test sort_regression",
      "done_when": "d",
      "files": ["crates/mp/src/lib.rs"],
      "covers_ac": [],
      "depends_on_steps": [],
      "order": 1,
      "work_package": "WP1"
    }
  ]
}"#;
    std::fs::write(milestones.join("997-edge-cases.json"), fixture).expect("write fixture");

    let (code, _stderr, report) = run_lint(&plan_dir);
    let values = warning_values(&report);

    assert_eq!(code, 0, "lint must always exit 0 (WARN, never FAIL)");
    assert!(
        warning_files(&report)
            .iter()
            .any(|f| f.contains("997-edge-cases")),
        "edge-case fixture must surface as WARN; report={report}"
    );
    assert!(
        values
            .iter()
            .any(|v| v.contains("echo prefix && make test")),
        "AC-1 must be flagged; values={values:?}"
    );
    assert!(
        values
            .iter()
            .any(|v| v.contains("sh -c \"echo hello && cargo test --workspace\"")),
        "AC-2 must be flagged; values={values:?}"
    );
    assert!(
        values
            .iter()
            .any(|v| v.contains("echo a; make test; echo b")),
        "AC-4 must be flagged; values={values:?}"
    );
    let make_test_hits = values.iter().filter(|v| v.contains("make test")).count();
    assert!(
        make_test_hits >= 2,
        "Both AC-1 and AC-4 should match make-test (>=2 hits); got {make_test_hits}"
    );
    assert!(
        values.iter().any(|v| v.contains("--workspace")),
        "AC-2 should match --workspace; values={values:?}"
    );
    assert!(
        !values.iter().any(|v| v.contains("sort_regression")),
        "AC-3 scoped control must NOT warn; values={values:?}"
    );
}
