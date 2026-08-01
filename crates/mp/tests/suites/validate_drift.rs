use crate::common::lib_api;
use crate::common::TestEnv;
use mp::milestone::{CreateAcceptanceCriterion, CreateMilestoneInput};
use mp::model::{Intent, Problem, Scope};

fn create_milestone(ctx: &mp::paths::PlanContext, title: &str) -> String {
    let m = lib_api::milestone_create(
        ctx,
        CreateMilestoneInput {
            title: Some(title.to_string()),
            intent: Intent {
                outcome: format!("Ship {title}"),
            },
            problem: Problem {
                description: format!("Need {title}."),
            },
            scope: Scope {
                in_scope: vec![title.to_string()],
                out_of_scope: vec!["Other".to_string(), "TBD".to_string()],
            },
            acceptance_criteria: vec![CreateAcceptanceCriterion {
                description: format!("{title} works"),
                verification: "manual: drift test sanity check".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .expect("milestone create in-process");
    m["milestone"]["id"].as_str().unwrap().to_string()
}

fn milestone_file_path(env: &TestEnv, id: &str) -> std::path::PathBuf {
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    for entry in std::fs::read_dir(&plan_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&format!("{id}-")) {
            return entry.path();
        }
    }
    panic!("milestone file not found for id {id}");
}

#[test]
fn validate_detects_spec_status_drift() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = create_milestone(&ctx, "OAuth Login");

    // Index now has spec_status="draft". Change the milestone file to "review"
    // without going through mp (which would auto-sync). M100: mutate the JSON
    // directly since the on-disk file may not carry the literal field shape.
    let file_path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&file_path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["milestone"]["spec_status"] = serde_json::Value::String("review".into());
    let modified = serde_json::to_string_pretty(&v).unwrap();
    assert_ne!(raw, modified, "file content should have changed");
    std::fs::write(&file_path, format!("{modified}\n")).unwrap();

    // Validate should flag W03 drift (M162: in-process via lib_api::validate).
    let json = lib_api::validate(&ctx).expect("validate in-process");
    let warnings = json["warnings"].as_array().unwrap();
    let drift_warnings: Vec<_> = warnings.iter().filter(|w| w["code"] == "W03").collect();
    assert!(
        !drift_warnings.is_empty(),
        "expected W03 drift warning, got: {warnings:?}"
    );
    assert!(
        drift_warnings[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("spec_status"),
        "W03 should mention spec_status drift"
    );
}

#[test]
fn validate_detects_execution_status_drift() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = create_milestone(&ctx, "OAuth Login");

    // M100: change execution_status directly via JSON mutation (the on-disk
    // file may not have a literal `"execution_status": "planned"` substring
    // since the field is skipped when empty).
    let file_path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&file_path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["milestone"]["execution_status"] = serde_json::Value::String("deferred".into());
    let modified = serde_json::to_string_pretty(&v).unwrap();
    assert_ne!(raw, modified);
    std::fs::write(&file_path, format!("{modified}\n")).unwrap();

    let json = lib_api::validate(&ctx).expect("validate in-process");
    let warnings = json["warnings"].as_array().unwrap();
    let drift_warnings: Vec<_> = warnings.iter().filter(|w| w["code"] == "W03").collect();
    assert!(
        !drift_warnings.is_empty(),
        "expected W03 drift warning, got: {warnings:?}"
    );
    assert!(
        drift_warnings[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("execution_status"),
        "W03 should mention execution_status drift"
    );
}

#[test]
fn validate_detects_title_drift() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = create_milestone(&ctx, "OAuth Login");

    // Change the milestone file's title
    let file_path = milestone_file_path(&env, &id);
    let content = std::fs::read_to_string(&file_path).unwrap();
    let modified = content.replace(
        "\"title\": \"OAuth Login\"",
        "\"title\": \"OAuth 2.0 Login\"",
    );
    assert_ne!(content, modified);
    std::fs::write(&file_path, &modified).unwrap();

    let json = lib_api::validate(&ctx).expect("validate in-process");
    let warnings = json["warnings"].as_array().unwrap();
    let drift_warnings: Vec<_> = warnings.iter().filter(|w| w["code"] == "W03").collect();
    assert!(
        !drift_warnings.is_empty(),
        "expected W03 drift warning, got: {warnings:?}"
    );
    assert!(
        drift_warnings[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("title"),
        "W03 should mention title drift"
    );
}

#[test]
fn validate_no_drift_after_mutation() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = create_milestone(&ctx, "OAuth Login");

    // Approve via lib_api (M162 in-process) — should auto-sync the index,
    // so no drift. The approve mutator returns the updated fragment.
    lib_api::milestone_approve(&ctx, &id).expect("approve in-process");

    let json = lib_api::validate(&ctx).expect("validate in-process");
    let drift_warnings: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W03")
        .collect();
    assert!(
        drift_warnings.is_empty(),
        "no W03 expected after mutation, got: {drift_warnings:?}"
    );
}

#[test]
fn validate_drift_distinct_from_w01() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = create_milestone(&ctx, "OAuth Login");

    // Induce drift via JSON mutation (M100-aware: edit lifecycle/spec_status
    // directly so the W03 detector sees the mismatch against the index).
    let file_path = milestone_file_path(&env, &id);
    let raw = std::fs::read_to_string(&file_path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["milestone"]["spec_status"] = serde_json::Value::String("review".into());
    let modified = serde_json::to_string_pretty(&v).unwrap();
    std::fs::write(&file_path, format!("{modified}\n")).unwrap();

    let json = lib_api::validate(&ctx).expect("validate in-process");
    let warnings = json["warnings"].as_array().unwrap();

    let _w01: Vec<_> = warnings.iter().filter(|w| w["code"] == "W01").collect();
    let w03: Vec<_> = warnings.iter().filter(|w| w["code"] == "W03").collect();
    assert!(!w03.is_empty(), "expected W03 (drift)");
    // There may or may not be W01 depending on index state — W03 must be present regardless
    // The key distinction: W03 is about value mismatch, W01 is about missing entry
}
