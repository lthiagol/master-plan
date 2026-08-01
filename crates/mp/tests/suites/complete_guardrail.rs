use std::fs;

use mp::ac_verify::{classify, command_for_execution, Kind};
use serde_json::{json, Value};

use crate::common::lib_api;
use crate::common::TestEnv;

fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str, steps: &Value, acs: &Value) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let mut milestone = json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "ready",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-06-25",
            "updated": "2026-06-25",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "fixture" },
        "problem": { "description": "fixture" },
        "scope": { "in_scope": ["fixture"], "out_of_scope": ["a", "b"] },
        "verification": { "date": "", "branch": "", "evidence": "" },
        "steps": [],
        "acceptance_criteria": [],
    });
    if let serde_json::Value::Object(ref mut m) = milestone {
        m.insert("steps".to_string(), steps.clone());
        m.insert("acceptance_criteria".to_string(), acs.clone());
    }
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();
}

fn steps_failing() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "failing step",
        "tests": "test 1 -eq 2",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

fn steps_passing() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "passing step",
        "tests": "test 1 -eq 1",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

fn steps_manual() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "manual step",
        "tests": "manual: accepted — reviewed by human",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

fn steps_empty() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "empty test step",
        "tests": "",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

fn steps_mixed_failing() -> Value {
    json!([
        {
            "id": "S1",
            "work_package": "WP1",
            "order": 1,
            "action": "passing test",
            "tests": "test 1 -eq 1",
            "done_when": "",
            "status": "done",
            "covers_ac": [],
            "depends_on_steps": [],
        },
        {
            "id": "S2",
            "work_package": "WP1",
            "order": 2,
            "action": "failing test",
            "tests": "test 1 -eq 2",
            "done_when": "",
            "status": "done",
            "covers_ac": [],
            "depends_on_steps": [],
        },
        {
            "id": "S3",
            "work_package": "WP1",
            "order": 3,
            "action": "manual test",
            "tests": "manual: accepted — human",
            "done_when": "",
            "status": "done",
            "covers_ac": [],
            "depends_on_steps": [],
        },
    ])
}

fn acs_passing() -> Value {
    json!([{
        "id": "AC-01",
        "description": "passing",
        "verification": "test 1 -eq 1",
        "status": "passed",
        "evidence": "",
    }])
}

// S10/AC-01: classification of step tests field uses same logic as AC verification
#[test]
fn step_tests_classify_kinds() {
    assert_eq!(classify(""), Kind::Empty);
    assert_eq!(classify("   "), Kind::Empty);
    assert_eq!(classify("manual: reviewed step"), Kind::Manual);
    assert_eq!(classify("MANUAL: accepted — done"), Kind::Manual);
    assert_eq!(classify("cargo test -p mp"), Kind::Runnable);
    assert_eq!(classify("make test"), Kind::Runnable);
    assert_eq!(classify("./scripts/check.sh"), Kind::Runnable);
    assert_eq!(classify("scripts/audit-step-tests.sh"), Kind::Runnable);
    assert_eq!(classify("bash -c \"test 1 -eq 1\""), Kind::Runnable);
    assert_eq!(classify("mp validate"), Kind::Runnable);
    assert_eq!(
        classify("crates/mp/tests/workflow_gates.rs"),
        Kind::Runnable
    );
    assert_eq!(classify("integration test: prose only"), Kind::Runnable);
    let cmd = command_for_execution("crates/mp/tests/workflow_gates.rs");
    assert!(cmd.contains("cargo test -p mp --test suite_plan workflow_gates::"));
    assert!(cmd.contains("--include-ignored"));
}

/// M188 F-09: verify/complete rewrite must not map live mp-oracle binaries
/// through the mp-only LEGACY_TEST_BINARY_MAP (dogfood Entry 24).
#[test]
fn verify_rewrite_preserves_mp_oracle_mini_schema_parity() {
    let oracle = "cargo nextest run -p mp-oracle --test mini_schema_parity --no-fail-fast";
    let out = command_for_execution(oracle);
    assert_eq!(out, oracle, "mp-oracle target must stay untouched");
    assert!(!out.contains("suite_validate"));

    let mp_legacy = "cargo nextest run -p mp --test mini_schema_parity --no-fail-fast";
    let rewritten = command_for_execution(mp_legacy);
    assert!(
        rewritten.contains("--test suite_validate mini_schema_parity::"),
        "mp legacy --test still consolidates: {rewritten}"
    );
}

// S10/AC-02: complete refused when a step's runnable test fails
#[test]
fn complete_refused_on_failing_step_test() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "99",
        "guardrail-failing",
        "Guardrail Failing Step",
        &steps_failing(),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "99", "--format", "json"]);
    assert!(
        !out.status.success(),
        "complete must be refused on failing step test"
    );
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], false);
    assert_eq!(v["gate"], "step-tests");
    let failures = v["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["step_id"], "S1");
    assert!(v["message"].as_str().unwrap().contains("step test"));

    // Milestone must still be in-progress
    let still = lib_api::run(&env, &["show", "milestone", "99", "--format", "json"]);
    let s: Value = serde_json::from_slice(&still.stdout).unwrap();
    assert_eq!(s["milestone"]["execution_status"], "in-progress");
}

// S10/AC-02: complete succeeds when all step tests pass
#[test]
fn complete_succeeds_when_step_tests_pass() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "98",
        "guardrail-passing",
        "Guardrail Passing Step",
        &steps_passing(),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "98", "--format", "json"]);
    assert!(
        out.status.success(),
        "complete must succeed when step tests pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], true);
    assert_eq!(v["milestone"]["execution_status"], "done");
}

// S10/AC-03: manual tests do not block completion
#[test]
fn manual_step_tests_do_not_block() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "97",
        "guardrail-manual",
        "Guardrail Manual Step",
        &steps_manual(),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "97", "--format", "json"]);
    assert!(
        out.status.success(),
        "complete must succeed on manual step tests: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// S10/AC-03: empty tests do not block completion
#[test]
fn empty_step_tests_do_not_block() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "96",
        "guardrail-empty",
        "Guardrail Empty Step",
        &steps_empty(),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "96", "--format", "json"]);
    assert!(
        out.status.success(),
        "complete must succeed on empty step tests: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// S10/AC-04: --force bypasses the step-tests gate
#[test]
fn complete_force_bypasses_step_test_gate() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "99",
        "guardrail-force",
        "Guardrail Force",
        &steps_failing(),
        &acs_passing(),
    );

    let out = lib_api::run(
        &env,
        &["milestone", "complete", "99", "--force", "--format", "json"],
    );
    assert!(out.status.success(), "complete --force must succeed");
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], true);
    assert_eq!(v["milestone"]["execution_status"], "done");

    let file = env
        .tmp
        .path()
        .join("master-plan/milestones/99-guardrail-force.json");
    let content = fs::read_to_string(file).unwrap();
    assert!(
        content.contains("step-tests force-bypassed"),
        "evidence must record the step-tests force-bypass: {content}"
    );
}

// S10/AC-05: mixed steps: runnable fail blocks, manual + empty are non-blocking
#[test]
fn mixed_step_tests_runnable_fail_blocks() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "95",
        "guardrail-mixed",
        "Guardrail Mixed Steps",
        &steps_mixed_failing(),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "95", "--format", "json"]);
    assert!(
        !out.status.success(),
        "complete must be refused when any runnable step test fails"
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["gate"], "step-tests");
    let failures = v["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["step_id"], "S2");
}

// S10/AC-06: milestone with no steps passes step-tests gate (no failures)
#[test]
fn no_steps_passes_step_tests_gate() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "94",
        "guardrail-nosteps",
        "Guardrail No Steps",
        &json!([]),
        &acs_passing(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "94", "--format", "json"]);
    assert!(
        out.status.success(),
        "complete must succeed on milestone with no steps: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// S10/AC-06: combined gate: both AC verification and step-tests can block independently
#[test]
fn combined_gates_both_can_block() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "93",
        "guardrail-combined",
        "Guardrail Combined",
        &steps_failing(),
        &json!([{
            "id": "AC-01",
            "description": "failing ac",
            "verification": "test 1 -eq 2",
            "status": "passed",
            "evidence": "",
        }]),
    );

    // AC verification runs first and should block (before step-tests even run)
    let out = lib_api::run(&env, &["milestone", "complete", "93", "--format", "json"]);
    assert!(!out.status.success(), "complete must be blocked");
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(
        v["gate"], "ac-verification",
        "AC gate fires before step-tests gate"
    );
}

fn steps_bare_rs_failing() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "bare rs failing step",
        "tests": "crates/mp/tests/bare_rs_fail.rs",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

fn steps_bash_failing() -> Value {
    json!([{
        "id": "S1",
        "work_package": "WP1",
        "order": 1,
        "action": "bash failing step",
        "tests": "bash -c \"test 1 -eq 2\"",
        "done_when": "",
        "status": "done",
        "covers_ac": [],
        "depends_on_steps": [],
    }])
}

#[test]
fn complete_refused_on_failing_bare_rs_step_test() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "92",
        "guardrail-bare-rs",
        "Guardrail Bare Rs",
        &steps_bare_rs_failing(),
        &acs_passing(),
    );

    let out = lib_api::run_at_repo(&env, &["milestone", "complete", "92", "--format", "json"]);
    assert!(
        !out.status.success(),
        "complete must refuse bare .rs step test failure: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["gate"], "step-tests");
    assert_eq!(v["failures"][0]["step_id"], "S1");
}

#[test]
fn complete_refused_on_failing_bash_step_test() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "91",
        "guardrail-bash",
        "Guardrail Bash",
        &steps_bash_failing(),
        &acs_passing(),
    );

    let out = lib_api::run_at_repo(&env, &["milestone", "complete", "91", "--format", "json"]);
    assert!(
        !out.status.success(),
        "complete must refuse bash -c failure"
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["gate"], "step-tests");
}
