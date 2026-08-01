//! M30: AC verification — classification unit tests + integration for
//! `mp milestone verify` and the `mp milestone complete` verification gate.

use std::fs;

use mp::ac_verify::{classify, Kind};
use serde_json::{json, Value};

use crate::common::{lib_api, mp_bin, repo_root, TestEnv};

fn fixture_mixed() -> Value {
    json!([
        {
            "id": "AC-01",
            "description": "runnable pass",
            "verification": "test 1 -eq 1",
            "status": "passed",
            "evidence": "",
        },
        {
            "id": "AC-02",
            "description": "runnable fail",
            "verification": "test 1 -eq 2",
            "status": "passed",
            "evidence": "",
        },
        {
            "id": "AC-03",
            "description": "manual",
            "verification": "manual: human review",
            "status": "passed",
            "evidence": "",
        },
        {
            "id": "AC-04",
            "description": "empty",
            "verification": "",
            "status": "passed",
            "evidence": "",
        },
    ])
}

fn fixture_manual_only() -> Value {
    json!([
        {
            "id": "AC-01",
            "description": "manual",
            "verification": "manual: review",
            "status": "passed",
            "evidence": "",
        },
        {
            "id": "AC-02",
            "description": "empty",
            "verification": "",
            "status": "passed",
            "evidence": "",
        },
    ])
}

fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str, acs: &Value) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = json!({
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
        "acceptance_criteria": acs,
    });
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();
}

// AC-01: classification is pure and covers runnable / manual / empty.
#[test]
fn classify_kinds() {
    assert_eq!(classify(""), Kind::Empty);
    assert_eq!(classify("   "), Kind::Empty);
    assert_eq!(classify("manual: review"), Kind::Manual);
    assert_eq!(classify("MANUAL: review"), Kind::Manual);
    assert_eq!(classify("integration test: prose only"), Kind::Runnable);
    assert_eq!(classify("cargo test -p mp"), Kind::Runnable);
    assert_eq!(classify("make adopt-check"), Kind::Runnable);
    assert_eq!(classify("grep -c tera Cargo.toml"), Kind::Runnable);
    assert_eq!(classify("./scripts/audit-step-tests.sh"), Kind::Runnable);
    assert_eq!(
        classify("crates/mp/tests/milestone_verify.rs"),
        Kind::Runnable
    );
    assert_eq!(classify("rg something"), Kind::Runnable);
    assert_eq!(classify("rm -rf /"), Kind::Runnable);
}

// AC-02: verify executes runnable ACs and exits non-zero on failure, naming it.
#[test]
fn verify_reports_and_exits_nonzero_on_failure() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "99",
        "fixture-mixed",
        "Fixture mixed",
        &fixture_mixed(),
    );

    let out = lib_api::run(&env, &["milestone", "verify", "99", "--format", "json"]);
    assert!(
        !out.status.success(),
        "verify must exit non-zero on failure"
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["runnable_total"], 2);
    assert_eq!(v["runnable_failed"], 1);
    assert_eq!(v["manual"], 1);
    assert_eq!(v["empty"], 1);
    assert_eq!(v["ok"], false);
    let failing = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ac_id"] == "AC-02")
        .unwrap();
    assert_eq!(failing["passed"], false);
    assert_eq!(failing["exit_code"], 1);
}

// AC-02 (positive): verify on an all-passing milestone exits 0.
#[test]
fn verify_exits_zero_when_runnable_pass() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "98",
        "fixture-manual",
        "Fixture manual-only",
        &fixture_manual_only(),
    );
    let out = lib_api::run(&env, &["milestone", "verify", "98", "--format", "json"]);
    assert!(
        out.status.success(),
        "verify exits 0 when no runnable fails"
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], true);
    assert_eq!(v["empty"], 1); // empty is reported (flagged) but non-blocking
}

// AC-03: complete refuses when a runnable AC fails, naming it.
#[test]
fn complete_refused_on_failing_runnable_ac() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "99",
        "fixture-mixed",
        "Fixture mixed",
        &fixture_mixed(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "99", "--format", "json"]);
    assert!(!out.status.success(), "complete must be refused");
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], false);
    assert_eq!(v["gate"], "ac-verification");
    let failures = v["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["ac_id"], "AC-02");
    // milestone must still be in-progress (not flipped to done)
    let still = lib_api::run(&env, &["show", "milestone", "99", "--format", "json"]);
    let s: Value = serde_json::from_slice(&still.stdout).unwrap();
    assert_eq!(s["milestone"]["execution_status"], "in-progress");
}

// AC-04: --force bypasses the gate and records the bypass in evidence.
#[test]
fn complete_force_bypasses_gate() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "99",
        "fixture-mixed",
        "Fixture mixed",
        &fixture_mixed(),
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
        .join("master-plan/milestones/99-fixture-mixed.json");
    let content = fs::read_to_string(file).unwrap();
    assert!(
        content.contains("verification force-bypassed"),
        "evidence must record the force-bypass: {content}"
    );
}

// AC-05: manual + empty verifications do not block completion; empty flagged.
#[test]
fn manual_and_empty_do_not_block() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "98",
        "fixture-manual",
        "Fixture manual-only",
        &fixture_manual_only(),
    );

    let out = lib_api::run(&env, &["milestone", "complete", "98", "--format", "json"]);
    assert!(
        out.status.success(),
        "manual+empty must not block completion"
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["ok"], true);

    // empty is still flagged in the verify report
    let vf = lib_api::run(&env, &["milestone", "verify", "98", "--format", "json"]);
    let vr: Value = serde_json::from_slice(&vf.stdout).unwrap();
    assert_eq!(vr["empty"], 1);
}

// Smoke test using the repo's real installed-binary path resolution is not
// needed; tests above use CARGO_BIN_EXE_mp via TestEnv (mp_bin).
#[test]
fn mp_bin_resolves() {
    let _ = std::process::Command::new(mp_bin())
        .arg("--version")
        .output();
    let _ = repo_root();
}

#[test]
fn complete_refreshes_evidence_on_recomplete() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "97",
        "evidence-refresh",
        "Evidence refresh",
        &fixture_manual_only(),
    );

    let first = lib_api::run(
        &env,
        &[
            "milestone",
            "complete",
            "97",
            "--evidence",
            "first complete",
            "--format",
            "json",
        ],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    lib_api::run(&env, &["milestone", "reopen", "97", "--format", "json"]);

    let second = lib_api::run(
        &env,
        &[
            "milestone",
            "complete",
            "97",
            "--evidence",
            "second complete after remediation",
            "--format",
            "json",
        ],
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    let show = lib_api::run_json(&env, &["show", "milestone", "97", "--format", "json"]);
    assert_eq!(
        show["verification"]["evidence"],
        "second complete after remediation"
    );
    assert_eq!(
        show["acceptance_criteria"][0]["evidence"],
        "second complete after remediation"
    );
    assert!(!show["verification"]["evidence"]
        .as_str()
        .unwrap()
        .contains("first complete"));
}
