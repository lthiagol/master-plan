//! M149 S0 / AC-01: `mp watch` startup preconditions.
//!
//! Exercises the precondition check via the library entry point
//! (`commands::autopilot_drive::watch_preconditions_report`) plus a CLI-level
//! smoke test that the watch command rejects a fresh project with
//! clear, aggregated, actionable errors.

mod common;

use crate::common::TestEnv;
use mp::autopilot::drive::check_preconditions;
use mp::config::ProjectConfig;
use serde_json::Value;

#[test]
fn fresh_project_fails_preconditions_with_all_failures_listed() {
    let env = TestEnv::new();
    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg_json = cfg_arg["config"].to_string();
    let cfg: ProjectConfig = serde_json::from_str(&cfg_json).expect("deserialize config");

    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    assert!(!report.ok, "fresh project has no herdr/harness config");
    let failed_names: Vec<&str> = report.failed().iter().map(|c| c.name.as_str()).collect();
    assert!(
        failed_names.contains(&"runner_config_present"),
        "runner harness missing must be reported: {failed_names:?}"
    );
    assert!(
        failed_names.contains(&"coordinator_config_present"),
        "coordinator harness missing must be reported: {failed_names:?}"
    );
}

#[test]
fn precondition_failures_carry_actionable_messages() {
    let env = TestEnv::new();
    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg: ProjectConfig =
        serde_json::from_str(&cfg_arg["config"].to_string()).expect("deserialize config");
    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    for c in report.failed() {
        if c.name.ends_with("_config_present") {
            assert!(
                c.message.contains("mp config set"),
                "message should suggest `mp config set`: {}",
                c.message
            );
        }
    }
}

#[test]
fn harnesses_present_pass_role_checks_regardless_of_herdr() {
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "cursor"]);

    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg: ProjectConfig =
        serde_json::from_str(&cfg_arg["config"].to_string()).expect("deserialize config");
    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    let runner_ok = report
        .checks
        .iter()
        .find(|c| c.name == "runner_config_present")
        .map(|c| c.ok)
        .unwrap_or(false);
    let coord_ok = report
        .checks
        .iter()
        .find(|c| c.name == "coordinator_config_present")
        .map(|c| c.ok)
        .unwrap_or(false);
    assert!(runner_ok, "runner harness set must pass role check");
    assert!(coord_ok, "coordinator harness set must pass role check");
    // herdr_on_path is environment-dependent; we don't assert on it
    // here. The role checks are the S0 contract that mp owns.
}

#[test]
fn log_path_under_writable_plan_dir_passes() {
    let env = TestEnv::new();
    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg: ProjectConfig =
        serde_json::from_str(&cfg_arg["config"].to_string()).expect("deserialize config");
    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    let log_check = report
        .checks
        .iter()
        .find(|c| c.name == "log_path_writable")
        .unwrap();
    assert!(log_check.ok, "{}", log_check.message);
}

#[test]
fn report_aggregates_all_failures_at_once() {
    // The contract is "clear aggregated error at startup listing
    // every invalid field" — multiple failures must all show up in
    // a single report, not require N restarts.
    let env = TestEnv::new();
    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg: ProjectConfig =
        serde_json::from_str(&cfg_arg["config"].to_string()).expect("deserialize config");
    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    // At minimum the two role-config failures should both be present
    // in the same report.
    let role_failures = report
        .failed()
        .iter()
        .filter(|c| c.name.ends_with("_config_present"))
        .count();
    assert!(
        role_failures >= 2,
        "expected at least 2 role-config failures in one report, got {role_failures}"
    );
}

#[test]
fn cli_watch_reports_preconditions_as_json() {
    // S0 ships only the precondition surface; the CLI `mp watch` arm
    // is wired in S2. This test pins that the library entry point
    // produces JSON-serializable output so S2 can emit it unchanged.
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);
    let cfg_arg = env.run_json(&["config", "show", "--format", "json"]);
    let cfg: ProjectConfig =
        serde_json::from_str(&cfg_arg["config"].to_string()).expect("deserialize config");
    let log_path = env
        .tmp
        .path()
        .join("master-plan")
        .join(".mp")
        .join("autopilot.log");
    let report = check_preconditions(&cfg, &log_path);

    let serialized: Value = serde_json::to_value(&report).expect("serialize report");
    assert!(serialized["checks"].is_array());
    assert!(
        serialized["checks"].as_array().unwrap().len() >= 4,
        "expected at least 4 precondition checks (herdr + 2 roles + log), got {}",
        serialized["checks"].as_array().unwrap().len()
    );
}
