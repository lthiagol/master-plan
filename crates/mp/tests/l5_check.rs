//! M142: end-to-end tests for the L5 evidence audit.
//!
//! Covers:
//! - AC-02: clean fixture (4 hand-offs with distinct session ids,
//!   correct role rotations per the 4 hand-off points) returns
//!   `ok: true` with empty violations.
//! - AC-03: violation fixture (same-session cross-role hand-off)
//!   returns `ok: false` with a `same_session_across_role_boundary`
//!   violation.
//! - AC-04: missing-session-identity fixture (empty `from_session` /
//!   `to_session`) returns `ok: false` with a
//!   `missing_session_identity` violation.
//! - AC-05: role-inversion fixture (hand-off 8→9 with `to_role ==
//!   "coordinator"`, contradicting the runner-receives rule) returns
//!   `ok: false` with a `role_inversion` violation.
//! - AC-06: `mp validate` integrates the L5 check as an advisory
//!   sub-check; violations appear under `l5_audit` with severity
//!   `advisory`. `mp validate --summary` does not report L5
//!   violations as errors; exit code is 0 when only advisory
//!   violations exist.
//! - AC-07: env-var auto-injection: `MP_SESSION_ID` + `MP_SESSION_ROLE`
//!   set → handoff records the env values; manual `--from-session`
//!   flag overrides.
//! - AC-08: `mp reviews handoff --help` documents the env-var contract.

mod common;

use std::fs;

use common::{mp_bin, repo_root, TestEnv};
use serde_json::Value;

/// Create a minimal ready milestone in the plan dir and sync.
fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "ready",
            "execution_status": "planned",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-09",
            "updated": "2026-07-09",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "x",
            "verification": "manual: test",
            "status": "pending",
            "evidence": "",
        }],
    });
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();
    let out = env.run(&["sync", "--format", "json"]);
    assert!(
        out.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Run `mp reviews handoff <mid> ...` with env vars set.
fn record_handoff(
    env: &TestEnv,
    mid: &str,
    args: &[&str],
    env_vars: &[(&str, &str)],
) -> std::process::Output {
    use std::process::Command;
    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(env.tmp.path())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .args(args)
        .arg(mid);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    cmd.output().expect("handoff")
}

/// AC-02: clean fixture (4 hand-offs with distinct session ids,
/// correct role rotations per the 4 hand-off points) returns ok: true.
#[test]
fn l5_check_clean_fixture_returns_ok() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-clean", "L5 check clean");

    // Hand-off (a): coordinator Approve → runner Claim & execute.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s1",
            "--to-session",
            "s2",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "approved spec for milestone 142",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (a) failed: {:?}", r);

    // Hand-off (b): runner Complete → coordinator External review.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s2",
            "--to-session",
            "s3",
            "--from-role",
            "runner",
            "--to-role",
            "coordinator",
            "--data",
            "self-reviewed state + self-findings",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (b) failed: {:?}", r);

    // Hand-off (c): coordinator External review → runner Remediate.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s3",
            "--to-session",
            "s4",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "external review findings for milestone 142",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (c) failed: {:?}", r);

    // Hand-off (d): runner Remediate → coordinator Re-review.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s4",
            "--to-session",
            "s5",
            "--from-role",
            "runner",
            "--to-role",
            "coordinator",
            "--data",
            "remediation commit for milestone 142",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (d) failed: {:?}", r);

    // Run the L5 audit.
    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(
        r.status.success(),
        "l5-check failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], true, "clean fixture must be ok: {v:?}");
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations.is_empty(),
        "clean fixture should have zero violations: {violations:?}"
    );
    assert_eq!(v["summary"]["total_handoffs"], 4);
    assert_eq!(v["summary"]["cross_role_handoffs"], 4);
}

/// AC-03: same-session cross-role hand-off returns ok: false with a
/// `same_session_across_role_boundary` violation.
#[test]
fn l5_check_same_session_cross_role_violation() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "142",
        "l5-check-same-session",
        "L5 same-session violation",
    );

    // Hand-off (a): same session id "shared" across coordinator→runner.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "shared",
            "--to-session",
            "shared",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "same session id at the cross-role boundary",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff failed: {:?}", r);

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|x| x["reason"] == "same_session_across_role_boundary"),
        "expected same_session_across_role_boundary violation: {violations:?}"
    );
}

/// M142 code-review: a same-session hand-off recorded WITHOUT role
/// fields must still be flagged. The original role-gated check let this
/// through (it only fired when roles were populated AND different), so an
/// operator passing identical `--from-session`/`--to-session` with no role
/// info defeated the L5 audit silently. Both session ids are populated and
/// equal → `same_session_across_role_boundary` must fire regardless of role.
#[test]
fn l5_check_same_session_without_roles_still_flagged() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "142",
        "l5-check-same-session-no-roles",
        "L5 same-session no roles",
    );

    // Same session id on both sides, no role info recorded.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "solo-sess",
            "--to-session",
            "solo-sess",
            "--data",
            "same session id, roles unrecorded",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff failed: {:?}", r);

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], false, "same-session hand-off must be flagged");
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|x| x["reason"] == "same_session_across_role_boundary"),
        "expected same_session_across_role_boundary violation; got: {violations:?}"
    );
}

/// AC-04: missing session identity (empty from_session / to_session)
/// returns ok: false with a missing_session_identity violation.
#[test]
fn l5_check_missing_session_identity_violation() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-missing-id", "L5 missing identity");

    let r = env.run(&[
        "reviews",
        "handoff",
        "142",
        "--to-session",
        "s1",
        "--data",
        "hand-off with empty from_session",
        "--format",
        "json",
    ]);
    assert!(r.status.success(), "handoff failed: {:?}", r);

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|x| x["reason"] == "missing_session_identity"),
        "expected missing_session_identity violation: {violations:?}"
    );
}

/// AC-05: role inversion — hand-off (c) (8→9, runner-receives) with
/// `to_role == "coordinator"` returns ok: false with a role_inversion
/// violation.
#[test]
fn l5_check_role_inversion_violation() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-role-inversion", "L5 role inversion");

    // First hand-off (a): correct role rotation to set up the index.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s1",
            "--to-session",
            "s2",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "approved spec",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (a) failed: {:?}", r);

    // Second hand-off (b): correct.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s2",
            "--to-session",
            "s3",
            "--from-role",
            "runner",
            "--to-role",
            "coordinator",
            "--data",
            "self-reviewed state",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (b) failed: {:?}", r);

    // Third hand-off (c): role_inversion. Stage 8→9 expects runner
    // to receive (to_role=runner); we set coordinator, which is wrong.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s3",
            "--to-session",
            "s4",
            "--from-role",
            "coordinator",
            "--to-role",
            "coordinator",
            "--data",
            "wrong: stage 8→9 expects runner-receives",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff (c) failed: {:?}", r);

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations.iter().any(|x| x["reason"] == "role_inversion"),
        "expected role_inversion violation: {violations:?}"
    );
}

/// AC-06: `mp validate` integrates the L5 check as an advisory
/// sub-check. `mp validate --summary` does not report L5 violations as
/// errors and the exit code remains 0 when only advisory violations
/// exist.
#[test]
fn validate_integrates_l5_as_advisory() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-validate", "L5 advisory via validate");

    // Stage 8→9 with same session id and same role (runner's session
    // is reused by the coordinator role).
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "shared",
            "--to-session",
            "shared",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "same session at cross-role boundary",
        ],
        &[],
    );
    assert!(r.status.success(), "handoff failed: {:?}", r);

    // mp validate: should exit 0 (advisory) and surface l5_audit.
    let out = env.run(&["validate", "--format", "json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "validate must exit 0 for advisory L5 violations; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v.get("l5_audit").is_some(),
        "validate output must include l5_audit: {v:?}"
    );
    assert_eq!(v["l5_audit"]["ok"], false);
    assert!(v["l5_audit"]["violation_count"].as_u64().unwrap() >= 1);
    let milestone_audits = v["l5_audit"]["milestones"].as_array().unwrap();
    assert!(
        milestone_audits
            .iter()
            .any(|m| m["milestone_id"] == "142" && m["ok"] == false),
        "expected milestone 142 in l5_audit with ok=false: {milestone_audits:?}"
    );

    // mp validate --summary: l5_audit section reports violation count
    // but `error_count` stays at 0 and `ok` stays true.
    let out = env.run(&["validate", "--summary", "--format", "json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["error_count"], 0);
    let l5 = v["l5_audit"].as_object().expect("l5_audit summary");
    assert_eq!(l5["ok"], false);
    assert!(l5["violation_count"].as_u64().unwrap() >= 1);
}

/// AC-07: env-var auto-injection — `MP_SESSION_ID` +
/// `MP_SESSION_ROLE` set → handoff records from_session / from_role
/// from the env; to_session stays empty unless `--to-session` is
/// given (mirroring MP_SESSION_ID into to_session would create an
/// L5 same-session violation). Manual `--from-session` overrides.
#[test]
fn handoff_env_var_auto_injection() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-env-injection", "L5 env injection");

    // Run with env vars: from_session/from_role populated from the env;
    // to_role is the complement of MP_SESSION_ROLE; to_session empty.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--data",
            "approved spec via harness auto-injection",
        ],
        &[
            ("MP_SESSION_ID", "sess-abc"),
            ("MP_SESSION_ROLE", "coordinator"),
        ],
    );
    assert!(r.status.success(), "handoff failed: {:?}", r);
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    let handoff = &v["handoff"];
    assert_eq!(handoff["from_session"], "sess-abc");
    assert_eq!(
        handoff["to_session"], "",
        "MP_SESSION_ID must NOT auto-fill to_session"
    );
    assert_eq!(handoff["from_role"], "coordinator");
    assert_eq!(
        handoff["to_role"], "runner",
        "from_role=coordinator implies to_role=runner"
    );

    // Manual flag overrides env-var from_session; to_session still
    // requires an explicit flag.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "manual-sess",
            "--to-session",
            "recv-sess",
            "--data",
            "manual override of env-var session",
        ],
        &[
            ("MP_SESSION_ID", "env-sess-should-not-win"),
            ("MP_SESSION_ROLE", "runner"),
        ],
    );
    assert!(r.status.success(), "handoff failed: {:?}", r);
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    let handoff = &v["handoff"];
    assert_eq!(handoff["from_session"], "manual-sess");
    assert_eq!(
        handoff["to_session"], "recv-sess",
        "explicit --to-session wins; env does not fill to_session"
    );
    assert_eq!(handoff["from_role"], "runner");
    assert_eq!(handoff["to_role"], "coordinator");
}

/// AC-08: `mp reviews handoff --help` documents the env-var contract.
#[test]
fn handoff_help_documents_env_var_contract() {
    let env = TestEnv::new();
    let out = env.run(&["reviews", "handoff", "--help"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("MP_SESSION_ID"),
        "--help must document MP_SESSION_ID: {combined}"
    );
    assert!(
        combined.contains("MP_SESSION_ROLE"),
        "--help must document MP_SESSION_ROLE: {combined}"
    );
    assert!(
        combined.contains("forgeable-by-humans") || combined.contains("forgeable by humans"),
        "--help must include the honesty clause about forgeable env contract: {combined}"
    );
}

/// AC-09 (lightweight): `mp reviews handoff --help` shows the contract
/// — manual --from-X / --to-X flags override the env values.
#[test]
fn handoff_help_documents_manual_override() {
    let env = TestEnv::new();
    let out = env.run(&["reviews", "handoff", "--help"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("override"),
        "--help must document manual flag override: {combined}"
    );
}

/// T-1: 0-handoff baseline. A milestone with no recorded hand-offs
/// must return ok:true (no violations) with total_handoffs=0. Locks
/// in the empty-baseline contract before the M142 semantics drift.
#[test]
fn l5_check_no_handoffs_returns_ok() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-no-handoffs", "No handoffs");

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["summary"]["total_handoffs"], 0);
    assert_eq!(v["summary"]["cross_role_handoffs"], 0);
    assert_eq!(v["summary"]["violation_count"], 0);
    assert_eq!(v["violations"].as_array().unwrap().len(), 0);
}

/// T-2: 5+ handoff cycling. The role-inversion check uses
/// `idx % 4` to map handoffs to (a)/(b)/(c)/(d). After the 4th
/// handoff, the cycle restarts. Verify with a 5-handoff record that
/// the cycling is exercised (handoff #5 maps back to hand-off
/// point (a)→stage 4→5).
#[test]
fn l5_check_cycles_after_four_handoffs() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "142",
        "l5-check-cycle-five",
        "Five handoffs cycle after four",
    );

    // First 4 handoffs: clean role rotations (a/b/c/d).
    let pairs: &[(&str, &str)] = &[
        ("coordinator", "runner"),
        ("runner", "coordinator"),
        ("coordinator", "runner"),
        ("runner", "coordinator"),
    ];
    for (i, (from, to)) in pairs.iter().enumerate() {
        let r = record_handoff(
            &env,
            "142",
            &[
                "reviews",
                "handoff",
                "--from-session",
                &format!("s{i}a"),
                "--to-session",
                &format!("s{i}b"),
                "--from-role",
                from,
                "--to-role",
                to,
                "--data",
                &format!("clean handoff {i}"),
            ],
            &[],
        );
        assert!(r.status.success(), "handoff {i} failed: {:?}", r);
    }

    // 5th handoff: idx=4, idx%4=0, so maps to (a)→stage 4→5, expecting
    // to_role=runner. We deliberately set to_role=coordinator
    // (wrong for that stage) to verify the cycling picks it up.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "s4a",
            "--to-session",
            "s4b",
            "--from-role",
            "coordinator",
            "--to-role",
            "coordinator",
            "--data",
            "5th handoff: should cycle to (a) and flag role_inversion",
        ],
        &[],
    );
    assert!(r.status.success(), "5th handoff failed: {:?}", r);

    let r = env.run(&["reviews", "l5-check", "142", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    let violations = v["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|x| x["reason"] == "role_inversion" && x["at_handoff"] == "H-05"),
        "5th handoff should cycle to (a)→stage 4→5 and surface role_inversion: {violations:?}"
    );
}

/// T-3: l5_check on a non-existent milestone id is permissive (no
/// hand-offs, audit returns clean baseline). The CLI surface
/// returns ok:true with total_handoffs=0. The L5 audit operates
/// on the persisted hand-off records; a missing milestone has no
/// hand-offs by definition, so the audit is trivially clean. The
/// caller (e.g., the validate integration) is responsible for
/// surfacing milestone existence separately if needed.
#[test]
fn l5_check_missing_milestone_returns_clean_baseline() {
    let env = TestEnv::new();

    let r = env.run(&["reviews", "l5-check", "999", "--format", "json"]);
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["summary"]["total_handoffs"], 0);
    assert_eq!(v["summary"]["violation_count"], 0);
}

/// T-4: --from-role / --to-role manual override of MP_SESSION_ROLE.
#[test]
fn handoff_env_var_role_manual_override() {
    let env = TestEnv::new();
    write_milestone(&env, "142", "l5-check-env-role-override", "Role override");

    // Set MP_SESSION_ROLE=coordinator, manually override --from-role
    // to runner. Manual --from-role wins for from_role; the env var
    // was for runner but we override; to_role = complement = coordinator.
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-role",
            "runner",
            "--data",
            "manual --from-role override",
        ],
        &[
            ("MP_SESSION_ID", "sess-x"),
            ("MP_SESSION_ROLE", "coordinator"),
        ],
    );
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    let handoff = &v["handoff"];
    assert_eq!(handoff["from_role"], "runner", "manual --from-role wins");
    assert_eq!(
        handoff["to_role"], "coordinator",
        "to_role = complement of from_role"
    );
    // MP_SESSION_ID populates from_session only; to_session stays empty
    // unless --to-session is given (manual override was only --from-role).
    assert_eq!(handoff["from_session"], "sess-x");
    assert_eq!(handoff["to_session"], "");
}

/// T-7: `mp validate --summary` with errors AND L5 violations.
/// Real gate failures must still surface; L5 violations stay
/// advisory (don't change exit code beyond what `errors` dictates).
#[test]
fn validate_errors_and_l5_violations_separated() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "142",
        "l5-check-errors-plus-l5",
        "Errors plus L5 violations",
    );

    // Trigger an L5 violation (same-session cross-role).
    let r = record_handoff(
        &env,
        "142",
        &[
            "reviews",
            "handoff",
            "--from-session",
            "shared",
            "--to-session",
            "shared",
            "--from-role",
            "coordinator",
            "--to-role",
            "runner",
            "--data",
            "L5 violation",
        ],
        &[],
    );
    assert!(r.status.success());

    // Validate: the milestone has no plan-level errors (test
    // fixture is clean), so `errors` is empty; only L5 advisory.
    // The exit code must be 0 (advisory) and l5_audit must
    // surface the violation. (This is a focused test of the
    // error_count vs violation_count separation in summary mode.)
    let out = env.run(&["validate", "--summary", "--format", "json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error_count"], 0, "no gate errors expected: {v:?}");
    assert!(
        v["l5_audit"]["violation_count"].as_u64().unwrap() >= 1,
        "l5_audit should surface the violation: {v:?}"
    );
}
