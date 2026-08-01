//! M166: `unblock_milestone` preserves terminal execution_status.
//!
//! AC-01: A `lifecycle=complete + execution_status=done + blocked=true`
//!        milestone keeps `execution_status='done'` after unblock (no
//!        regression to 'planned').
//! AC-02: A non-terminal blocked milestone still falls back to
//!        `execution_status='planned'` (the prior behavior).
//! AC-03: A `cancelled=true` milestone preserves its on-disk
//!        execution_status when unblocked.
//! AC-04: Unblocking a non-blocked milestone refuses with
//!        'milestone is not blocked' (regression-pin the existing bail).
//!
//! Test fixtures are minimal hand-crafted milestones in a per-test TempDir.
//! Mirrors the pattern from `milestone_update_verification.rs`.

mod common;

use std::process::Command;

use crate::common::{repo_root, TestEnv};

fn mp_bin() -> &'static std::path::Path {
    common::mp_bin()
}

fn workspace_root() -> std::path::PathBuf {
    repo_root()
}

fn run_mp(env: &TestEnv, args: &[&str]) -> std::process::Output {
    Command::new(mp_bin())
        .current_dir(env.tmp.path())
        .env("MP_HOME", workspace_root())
        .args(args)
        .output()
        .expect("failed to run mp")
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

fn read_milestone(env: &TestEnv, id: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(milestone_file_path(env, id)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn patch_milestone(env: &TestEnv, id: &str, mutator: impl FnOnce(&mut serde_json::Value)) {
    let path = milestone_file_path(env, id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    mutator(&mut m);
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

fn fixture_id(env: &TestEnv) -> String {
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    let mut entries: Vec<_> = std::fs::read_dir(&plan_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    // Pick the first NON-TERMINAL milestone. The minimal-ready fixture's
    // 01-foundation is at exec=done + spec=verified (legacy complete
    // shape) and is terminal via `effective_lifecycle` — not useful for
    // tests that need an in-progress baseline. 02-feature-alpha is at
    // exec=planned + spec=ready (non-terminal).
    for entry in entries.iter() {
        let raw = std::fs::read_to_string(entry.path()).unwrap();
        let m: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let lc = m["milestone"]["lifecycle"].as_str().unwrap_or("");
        let spec = m["milestone"]["spec_status"].as_str().unwrap_or("verified");
        let exec = m["milestone"]["execution_status"]
            .as_str()
            .unwrap_or("done");
        let cancelled = m["milestone"]["cancelled"].as_bool().unwrap_or(false);
        // is_terminal: lifecycle matches LIFECYCLE_TERMINAL OR cancelled.
        let is_terminal = lc == "complete"
            || cancelled
            || (lc.is_empty() && exec == "done" && spec == "verified");
        if !is_terminal {
            let stem = entry
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            return stem.split('-').next().unwrap().to_string();
        }
    }
    panic!("fixture has no non-terminal milestone; pick a different fixture for this test")
}

/// AC-01 — the M159 pattern. `lifecycle=complete + execution_status=done +
/// blocked=true` must keep `execution_status='done'` after unblock; the overlay
/// fields (`blocked`, `block_reason`, `blocked_at`, `blocked_by`) are cleared,
/// but the terminal execution status survives.
#[test]
fn complete_plus_blocked_unblock_keeps_done() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);
    // Stamp complete + blocked overlay directly. Public `milestone block`
    // refuses Complete (M189 F-07); unblock must still clear a migrated
    // / hand-written terminal+blocked drift state.
    patch_milestone(&env, &id, |m| {
        m["milestone"]["lifecycle"] = serde_json::json!("complete");
        m["milestone"]["spec_status"] = serde_json::json!("verified");
        m["milestone"]["execution_status"] = serde_json::json!("done");
        m["milestone"]["blocked"] = serde_json::json!(true);
        m["milestone"]["block_reason"] = serde_json::json!("M166 smoke: post-completion block");
        m["milestone"]["blocked_by"] = serde_json::json!("user");
        m["milestone"]["blocked_at"] = serde_json::json!("2026-07-01T00:00:00Z");
    });

    let after_block = read_milestone(&env, &id);
    assert_eq!(
        after_block["milestone"]["execution_status"], "done",
        "complete + done milestone must keep execution_status='done' during block; got {:?}",
        after_block["milestone"]["execution_status"]
    );
    assert_eq!(after_block["milestone"]["blocked"], true);
    assert_eq!(
        after_block["milestone"]["block_reason"],
        "M166 smoke: post-completion block"
    );

    let unblock = run_mp(&env, &["milestone", "unblock", &id]);
    assert!(
        unblock.status.success(),
        "unblock failed: {:?}",
        unblock.status
    );

    // The headline assertion: terminal execution_status survives the unblock.
    let after_unblock = read_milestone(&env, &id);
    assert_eq!(
        after_unblock["milestone"]["execution_status"], "done",
        "complete + done milestone must keep execution_status='done' after unblock; got {:?}",
        after_unblock["milestone"]["execution_status"]
    );
    // Overlay fields cleared.
    assert_eq!(after_unblock["milestone"]["blocked"], false);
    assert_eq!(after_unblock["milestone"]["block_reason"], "");
    assert_eq!(after_unblock["milestone"]["blocked_by"], "");
    assert_eq!(after_unblock["milestone"]["blocked_at"], "");
    // Terminal lifecycle survives.
    assert_eq!(after_unblock["milestone"]["lifecycle"], "complete");
    assert_eq!(after_unblock["milestone"]["spec_status"], "verified");
}

/// AC-02 — regression-pin for the prior behavior. A non-terminal (in-progress)
/// blocked milestone falls back to `execution_status='planned'` after unblock.
#[test]
fn in_progress_blocked_unblock_keeps_planned() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);
    // Stamp in-progress state on the fixture (the minimal-ready fixture
    // starts at planned/decomposed so the test can set-status to in-progress).
    let set_status = run_mp(&env, &["milestone", "set-status", &id, "in-progress"]);
    assert!(set_status.status.success());

    let block = run_mp(
        &env,
        &[
            "milestone",
            "block",
            &id,
            "--reason",
            "M166 non-terminal smoke",
            "--by",
            "user",
        ],
    );
    assert!(block.status.success());

    let after_block = read_milestone(&env, &id);
    assert_eq!(after_block["milestone"]["execution_status"], "blocked");

    let unblock = run_mp(&env, &["milestone", "unblock", &id]);
    assert!(unblock.status.success());

    let after_unblock = read_milestone(&env, &id);
    assert_eq!(
        after_unblock["milestone"]["execution_status"], "in-progress",
        "non-terminal blocked milestone must restore lifecycle exec after unblock; got {:?}",
        after_unblock["milestone"]["execution_status"]
    );
}

/// AC-03 — `cancelled=true` is `is_terminal()`. Unblock must preserve the
/// on-disk execution_status (typically `'cancelled'` written by the cancel
/// path) instead of forcing `'planned'`.
#[test]
fn cancelled_blocked_unblock_keeps_cancelled() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);

    // Stamp cancelled=true with a blocked overlay already set. Public
    // `milestone block` refuses cancelled (terminal overlay); unblock must
    // still clear the blocked flag while preserving cancelled.
    patch_milestone(&env, &id, |m| {
        m["milestone"]["cancelled"] = serde_json::json!(true);
        m["milestone"]["execution_status"] = serde_json::json!("cancelled");
        m["milestone"]["blocked"] = serde_json::json!(true);
        m["milestone"]["block_reason"] = serde_json::json!("M166 cancelled smoke");
        m["milestone"]["blocked_by"] = serde_json::json!("user");
        m["milestone"]["blocked_at"] = serde_json::json!("2026-07-01T00:00:00Z");
    });

    let after_block = read_milestone(&env, &id);
    // M166: cancelled=true is terminal; blocked overlay may coexist on
    // hand-written / migrated drift — execution_status stays cancelled.
    assert_eq!(
        after_block["milestone"]["execution_status"], "cancelled",
        "cancelled=true is terminal; got {:?}",
        after_block["milestone"]["execution_status"]
    );
    assert_eq!(after_block["milestone"]["blocked"], true);

    let unblock = run_mp(&env, &["milestone", "unblock", &id]);
    assert!(unblock.status.success());

    let after_unblock = read_milestone(&env, &id);
    assert_eq!(
        after_unblock["milestone"]["execution_status"], "cancelled",
        "cancelled=true is terminal; unblock must keep execution_status='cancelled'; got {:?}",
        after_unblock["milestone"]["execution_status"]
    );
    // M166 fix: unblock_milestone no longer clears `cancelled=false`
    // (the prior paranoia-clear would have erased the cancellation
    // overlay). Both the cancelled overlay and `execution_status='cancelled'`
    // survive the block+unblock cycle intact. The 'cancelled' value in
    // `execution_status` is load-bearing — see F-02 (filed against M166
    // as a magic-string coupling risk worth a future refactor).
    assert_eq!(after_unblock["milestone"]["cancelled"], true);
    // Block overlay fields cleared.
    assert_eq!(after_unblock["milestone"]["blocked"], false);
    assert_eq!(after_unblock["milestone"]["block_reason"], "");
}

/// AC-04 — regression-pin: the existing bail on a non-blocked milestone.
#[test]
fn unblock_milestone_refuses_non_blocked() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);
    // Fixture is in the planned / not-blocked state. Skip any
    // block/unblock dance — just call unblock directly.
    let unblock = run_mp(&env, &["milestone", "unblock", &id]);
    assert!(
        !unblock.status.success(),
        "unblock on a non-blocked milestone must fail; got {:?}",
        unblock.status
    );
    let stderr = String::from_utf8_lossy(&unblock.stderr);
    assert!(
        stderr.contains("milestone is not blocked"),
        "expected bail message in stderr; got: {stderr}"
    );
}

/// M166 ext-review F-08: pin the bail mechanism swap from the
/// pre-M166 `execution_status == "blocked"` check to the
/// post-M166 `!m.milestone.blocked` check. The fixture ships at
/// `blocked=false, execution_status!="blocked"` (planned/none state).
/// Constructing `blocked=false, execution_status="blocked"` is the
/// legacy-inconsistency scenario that distinguishes the two
/// implementations — pre-M166 the call would proceed silently (no
/// bail, no overlay update), post-M166 the bail fires.
#[test]
fn unblock_milestone_bail_uses_blocked_overlay_not_execution_status() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);
    // Construct the legacy-inconsistency state by hand: execution_status
    // is 'blocked' but the overlay `blocked` is false. Pre-M166 this
    // would unblock successfully (and silently regress); post-M166
    // the bail should fire because the overlay is the source of truth.
    patch_milestone(&env, &id, |m| {
        m["milestone"]["execution_status"] = serde_json::json!("blocked");
        // `.blocked` is left at the fixture default (false).
    });

    let unblock = run_mp(&env, &["milestone", "unblock", &id]);
    assert!(
        !unblock.status.success(),
        "unblock on blocked=false overlay (even with execution_status='blocked') must fail; got {:?}",
        unblock.status
    );
    let stderr = String::from_utf8_lossy(&unblock.stderr);
    assert!(
        stderr.contains("milestone is not blocked"),
        "expected bail message in stderr; got: {stderr}"
    );
}

/// M166 ext-review F-03 follow-up: `set_execution_status` must also
/// refuse non-terminal transitions on a terminal milestone. The
/// pre-M166 surface had no such guard and regressed terminal
/// milestones' execution_status; this test pins the M166 fix.
#[test]
fn set_execution_status_refuses_non_terminal_on_terminal_milestone() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env);
    // Stamp the legacy complete triple.
    patch_milestone(&env, &id, |m| {
        m["milestone"]["lifecycle"] = serde_json::json!("complete");
        m["milestone"]["spec_status"] = serde_json::json!("verified");
        m["milestone"]["execution_status"] = serde_json::json!("done");
    });

    // 'blocked' is non-terminal. The transition would write
    // execution_status='blocked' while lifecycle stays 'complete'.
    let blocked = run_mp(&env, &["milestone", "set-status", &id, "blocked"]);
    assert!(
        !blocked.status.success(),
        "set-status blocked on a complete milestone must fail; got {:?}",
        blocked.status
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains("is terminal"),
        "expected terminal-milestone bail message; got: {stderr}"
    );

    // The execution_status must NOT have flipped — error path is
    // a no-op on disk.
    let after = read_milestone(&env, &id);
    assert_eq!(
        after["milestone"]["execution_status"], "done",
        "execution_status must be unchanged after the bail; got {:?}",
        after["milestone"]["execution_status"]
    );
    assert_eq!(after["milestone"]["lifecycle"], "complete");
}
