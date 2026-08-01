//! M110 AC-03 (S3): macOS-portability patterns in `mp plan verify-lint`.

use crate::common::{mp_bin, repo_root};
use serde_json::Value;
use std::process::Command;

fn run_lint(plan_dir: &std::path::Path) -> Value {
    let project_root = plan_dir.parent().unwrap();
    let out = Command::new(mp_bin())
        .current_dir(project_root)
        .env("MP_HOME", repo_root())
        .arg("--plan-dir")
        .arg(plan_dir)
        .args(["plan", "verify-lint"])
        .output()
        .expect("mp plan verify-lint");
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).expect("json")
}

fn warning_count_for_file(report: &Value, slug: &str) -> usize {
    report["warnings"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|w| w["milestone_file"].as_str().unwrap_or("").contains(slug))
        .count()
}

fn has_pattern_for_file(report: &Value, slug: &str, needle: &str) -> bool {
    report["warnings"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .any(|w| {
            w["milestone_file"].as_str().unwrap_or("").contains(slug)
                && w["pattern"].as_str().unwrap_or("").contains(needle)
        })
}

#[test]
fn flags_wc_l_without_xargs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let brittle = r#"{
  "milestone": {"id":"901","slug":"brittle-wc","title":"Brittle","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"mp list milestones | wc -l)","evidence":"","status":"pending"}]
}"#;

    let clean = r#"{
  "milestone": {"id":"902","slug":"clean-wc","title":"Clean","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"cargo test -p mp --lib","evidence":"","status":"pending"}]
}"#;

    std::fs::write(milestones.join("901-brittle-wc.json"), brittle).unwrap();
    std::fs::write(milestones.join("902-clean-wc.json"), clean).unwrap();

    let report = run_lint(&plan_dir);
    assert!(warning_count_for_file(&report, "901-brittle-wc") > 0);
    assert!(has_pattern_for_file(&report, "901-brittle-wc", "wc -l"));
    assert_eq!(warning_count_for_file(&report, "902-clean-wc"), 0);
}

#[test]
fn flags_grep_l_without_true_guard() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let brittle = r#"{
  "milestone": {"id":"903","slug":"brittle-grep","title":"Brittle","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"grep -rl foo master-plan | grep -l bar","evidence":"","status":"pending"}]
}"#;

    let guarded = r#"{
  "milestone": {"id":"904","slug":"guarded-grep","title":"Guarded","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"grep -rl foo master-plan | grep -l bar || true","evidence":"","status":"pending"}]
}"#;

    std::fs::write(milestones.join("903-brittle-grep.json"), brittle).unwrap();
    std::fs::write(milestones.join("904-guarded-grep.json"), guarded).unwrap();

    let report = run_lint(&plan_dir);
    assert!(warning_count_for_file(&report, "903-brittle-grep") > 0);
    assert_eq!(warning_count_for_file(&report, "904-guarded-grep"), 0);
}

#[test]
fn flags_raw_jq_without_pipe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let brittle = r#"{
  "milestone": {"id":"905","slug":"brittle-jq","title":"Brittle","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"test \"$(jq .error_count master-plan/plan.json)\" = 0","evidence":"","status":"pending"}]
}"#;

    let piped = r#"{
  "milestone": {"id":"906","slug":"piped-jq","title":"Piped","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"mp validate --summary | jq .error_count","evidence":"","status":"pending"}]
}"#;

    std::fs::write(milestones.join("905-brittle-jq.json"), brittle).unwrap();
    std::fs::write(milestones.join("906-piped-jq.json"), piped).unwrap();

    let report = run_lint(&plan_dir);
    assert!(warning_count_for_file(&report, "905-brittle-jq") > 0);
    assert_eq!(warning_count_for_file(&report, "906-piped-jq"), 0);
}
