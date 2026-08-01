//! M110 AC-02 (S2): per-milestone affected-crate derivation in `mp plan verify-lint`.

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
    assert!(
        out.status.success(),
        "verify-lint must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json stdout")
}

fn flagged_files(report: &Value) -> Vec<String> {
    report["warnings"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|w| w["milestone_file"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn flags_multi_crate_on_single_crate_milestone_only() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plan_dir = tmp.path().join("master-plan");
    let milestones = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones).expect("mkdir");

    let broad = r#"{
  "milestone": {"id":"801","slug":"broad","title":"Broad","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"cargo test -p mp -p raul && mp validate --summary","evidence":"","status":"pending"}],
  "steps": [{"id":"S1","action":"a","status":"pending","tests":"cargo test -p mp --lib","done_when":"d","files":["crates/mp/src/lib.rs"],"covers_ac":["AC-1"],"depends_on_steps":[],"order":1,"work_package":"WP1"}]
}"#;

    let narrow = r#"{
  "milestone": {"id":"802","slug":"narrow","title":"Narrow","lifecycle":"draft","effort":"S","risk":"low","spec_status":"draft","execution_status":"planned","depends_on":[],"created":"2026-07-05","updated":"2026-07-05"},
  "intent": {"outcome":"x"},
  "problem": {"description":"x"},
  "scope": {"in_scope":["x"],"out_of_scope":["a","b"]},
  "acceptance_criteria": [{"id":"AC-1","description":"x","verification":"cargo test -p mp --lib reviews::tests","evidence":"","status":"pending"}],
  "steps": [{"id":"S1","action":"a","status":"pending","tests":"cargo test -p mp --lib","done_when":"d","files":["crates/mp/src/lib.rs"],"covers_ac":["AC-1"],"depends_on_steps":[],"order":1,"work_package":"WP1"}]
}"#;

    std::fs::write(milestones.join("801-broad.json"), broad).unwrap();
    std::fs::write(milestones.join("802-narrow.json"), narrow).unwrap();

    let report = run_lint(&plan_dir);
    let files = flagged_files(&report);

    assert!(
        files.iter().any(|f| f.contains("801-broad")),
        "broad milestone must warn; files={files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("802-narrow")),
        "narrow milestone must not warn; files={files:?}"
    );
}
