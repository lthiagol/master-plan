//! M100 ER-1 regression: each setter writes both the legacy
//! `execution_status` field AND the unified `lifecycle` field plus the
//! relevant overlay boolean (`blocked`, `deferred`, `cancelled`). Pre-ER-1
//! the setters wrote only the legacy field, leaving the milestone file
//! structurally inconsistent on every transition.
//!
//! These tests pin the post-state of every transition the M100 review
//! identified (set_execution_status variants, block, unblock, defer,
//! reopen, complete).

use std::collections::BTreeMap;

use serde_json::json;

mod common;
use common::TestEnv;

/// Build a milestone via the public CLI so we exercise the same write
/// path the team uses. Returns the milestone id.
fn make_milestone(env: &TestEnv, slug: &str) -> String {
    let create = json!({
        "title": format!("ER-1 {}", slug),
        "intent": {"outcome": "ER-1 regression pin"},
        "problem": {"description": "Setter writes lifecycle + overlay."},
        "scope": {
            "in_scope": ["pin setter behavior"],
            "out_of_scope": ["unrelated setters", "downstream readers"]
        },
        "acceptance_criteria": [
            {"description": "ping", "verification": "manual: accepted — test pin"}
        ]
    });
    let out = env.run(&["milestone", "create", "--json", &create.to_string()]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

fn show_milestone(env: &TestEnv, id: &str) -> serde_json::Value {
    let out = env.run(&["show", "milestone", id, "--format", "raw"]);
    assert!(
        out.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn milestone_meta(env: &TestEnv, id: &str) -> BTreeMap<String, serde_json::Value> {
    let doc = show_milestone(env, id);
    let obj = doc["milestone"].as_object().unwrap().clone();
    obj.into_iter().collect()
}

/// Promote a milestone to `in-progress` so it can be blocked / completed
/// / reopened. Uses the public CLI transitions; the regression asserts
/// on the post-state, not the path.
fn approve_and_start(env: &TestEnv, id: &str) {
    let r = env.run(&["milestone", "approve", id]);
    assert!(
        r.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let r = env.run(&["milestone", "set-status", id, "in-progress"]);
    assert!(
        r.status.success(),
        "set-status in-progress failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
}

#[test]
fn complete_milestone_routes_to_complete_lifecycle() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "complete-lifecycle");
    approve_and_start(&env, &id);

    // M196: the review gate. To reach terminal `complete` on a
    // non-track milestone with no recorded review, the test must
    // explicitly bypass the gate via `--skip-review` (which records
    // `[skip-review]` as debt in evidence). The intent of this test
    // is to pin the lifecycle routing, not the review gate.
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "All ACs passed; ER-1 regression pin.",
        "--executor",
        "test",
        "--skip-review",
    ]);
    assert!(
        out.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["lifecycle"],
        json!("complete"),
        "complete_milestone must set lifecycle=complete (M100 ER-1); got: {}",
        meta["lifecycle"]
    );
    assert_eq!(
        meta["blocked"],
        json!(false),
        "complete must clear the blocked overlay; got: {}",
        meta["blocked"]
    );
    assert_eq!(meta["deferred"], json!(false));
    assert_eq!(meta["spec_status"], json!("verified"));
    assert_eq!(meta["execution_status"], json!("done"));
}

#[test]
fn block_milestone_sets_overlay_and_keeps_lifecycle() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "block-overlay");
    approve_and_start(&env, &id);

    let out = env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "regression-pin",
        "--by",
        "test",
    ]);
    assert!(
        out.status.success(),
        "block failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["blocked"],
        json!(true),
        "block_milestone must set the blocked overlay (M100 ER-1); got: {}",
        meta["blocked"]
    );
    // blocked is an overlay; lifecycle stays at the prior state.
    assert_eq!(meta["lifecycle"], json!("in-progress"),
        "lifecycle should remain 'in-progress' after block (overlay doesn't own a lifecycle transition); got: {}", meta["lifecycle"]);
    assert_eq!(meta["execution_status"], json!("blocked"));
}

#[test]
fn unblock_milestone_clears_overlay() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "unblock-overlay");
    approve_and_start(&env, &id);
    let rb = env.run(&["milestone", "block", &id, "--reason", "needs-research"]);
    assert!(
        rb.status.success(),
        "block failed: {}",
        String::from_utf8_lossy(&rb.stderr)
    );

    let out = env.run(&["milestone", "unblock", &id]);
    assert!(
        out.status.success(),
        "unblock failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["blocked"],
        json!(false),
        "unblock_milestone must clear the blocked overlay (M100 ER-1); got: {}",
        meta["blocked"]
    );
}

#[test]
fn defer_milestone_sets_overlay_and_deferred_reason() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "defer-overlay");
    approve_and_start(&env, &id);

    let out = env.run(&[
        "milestone",
        "defer",
        &id,
        "--reason",
        "needs-research",
        "--by",
        "test",
    ]);
    assert!(
        out.status.success(),
        "defer failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["deferred"],
        json!(true),
        "defer_milestone must set the deferred overlay (M100 ER-1); got: {}",
        meta["deferred"]
    );
    assert_eq!(
        meta["deferred_reason"],
        json!("needs-research"),
        "defer_milestone should record the reason in deferred_reason (M100 ER-1); got: {}",
        meta["deferred_reason"]
    );
    assert_eq!(meta["execution_status"], json!("deferred"));
}

#[test]
fn set_execution_status_to_blocked_sets_overlay() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "blocked-overlay");
    approve_and_start(&env, &id);

    let out = env.run(&["milestone", "set-status", &id, "blocked"]);
    assert!(
        out.status.success(),
        "set-status blocked failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["execution_status"], json!("blocked"));
    assert_eq!(
        meta["blocked"],
        json!(true),
        "blocked overlay must be set when execution_status='blocked' (M100 ER-1); got: {}",
        meta["blocked"]
    );
}

#[test]
fn set_execution_status_to_deferred_sets_deferred_overlay() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "deferred-overlay");
    approve_and_start(&env, &id);

    let out = env.run(&["milestone", "set-status", &id, "deferred"]);
    assert!(
        out.status.success(),
        "set-status deferred failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["execution_status"], json!("deferred"));
    assert_eq!(
        meta["deferred"],
        json!(true),
        "deferred overlay must be set when execution_status='deferred' (M100 ER-1); got: {}",
        meta["deferred"]
    );
}

#[test]
fn set_execution_status_to_cancelled_sets_overlay_keeps_lifecycle() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "cancelled-overlay");
    approve_and_start(&env, &id);

    let out = env.run(&["milestone", "set-status", &id, "cancelled"]);
    assert!(
        out.status.success(),
        "set-status cancelled failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["cancelled"],
        json!(true),
        "cancelled overlay must be set when execution_status='cancelled' (M100 ER-1); got: {}",
        meta["cancelled"]
    );
    // cancelled is an overlay; lifecycle is preserved (M100 ER-3 will
    // reconcile whether `cancelled` is a lifecycle value too).
    assert_eq!(
        meta["lifecycle"],
        json!("in-progress"),
        "lifecycle should remain 'in-progress' after set-status cancelled (overlay); got: {}",
        meta["lifecycle"]
    );
    assert_eq!(meta["execution_status"], json!("cancelled"));
}

#[test]
fn reopen_milestone_sets_lifecycle_in_progress_and_clears_overlays() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "reopen-lifecycle");
    approve_and_start(&env, &id);
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "first pass complete",
        "--executor",
        "test",
    ]);

    let out = env.run(&["milestone", "reopen", &id]);
    assert!(
        out.status.success(),
        "reopen failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["lifecycle"],
        json!("in-progress"),
        "reopen must set lifecycle=in-progress (M100 ER-1); got: {}",
        meta["lifecycle"]
    );
    assert_eq!(meta["blocked"], json!(false));
    assert_eq!(meta["deferred"], json!(false));
    assert_eq!(meta["execution_status"], json!("in-progress"));
}

#[test]
fn set_execution_status_to_in_progress_routes_to_in_progress_lifecycle() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "in-progress-lifecycle");
    // approve only (not started); then set-status in-progress.
    let r = env.run(&["milestone", "approve", &id]);
    assert!(
        r.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    let out = env.run(&["milestone", "set-status", &id, "in-progress"]);
    assert!(
        out.status.success(),
        "set-status in-progress failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["lifecycle"],
        json!("in-progress"),
        "set-status in-progress must land lifecycle=in-progress (M100 ER-1); got: {}",
        meta["lifecycle"]
    );
    assert_eq!(meta["blocked"], json!(false));
}

#[test]
fn defer_then_set_status_planned_clears_deferred_overlay() {
    // ER-8 follow-up: a milestone that gets deferred then has its
    // execution_status reset to planned via set-execution-status
    // must NOT keep `deferred=true`. Pre-fix the deferred overlay
    // stuck because set-execution-status only updated blocked on
    // transition away from deferred.
    let env = TestEnv::new();
    let id = make_milestone(&env, "defer-undefer");
    approve_and_start(&env, &id);

    env.run(&["milestone", "defer", &id, "--reason", "needs-research"]);
    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["deferred"], json!(true));

    env.run(&["milestone", "set-status", &id, "planned"]);
    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["deferred"],
        json!(false),
        "set-status planned must clear the deferred overlay (ER-8 follow-up); got: {}",
        meta["deferred"]
    );
    assert_eq!(
        meta["execution_status"],
        json!("in-progress"),
        "clearing deferred resumes the canonical in-progress phase; the legacy setter cannot regress it to planned"
    );
}

#[test]
fn block_then_set_status_deferred_clears_blocked_overlay() {
    // ER-8 follow-up: blocked + deferred must be mutually exclusive.
    let env = TestEnv::new();
    let id = make_milestone(&env, "block-then-defer");
    approve_and_start(&env, &id);

    env.run(&["milestone", "block", &id, "--reason", "stuck"]);
    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["blocked"], json!(true));

    env.run(&["milestone", "set-status", &id, "deferred"]);
    let meta = milestone_meta(&env, &id);
    assert_eq!(
        meta["blocked"],
        json!(false),
        "transitioning to deferred must clear the blocked overlay (ER-8 follow-up); got: {}",
        meta["blocked"]
    );
    assert_eq!(meta["deferred"], json!(true));
}

#[test]
fn set_lifecycle_is_rejected_as_migration_only() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "set-lifecycle-aliases");
    let out = env.run(&["milestone", "set-lifecycle", &id, "in-progress"]);
    assert!(
        !out.status.success(),
        "public set-lifecycle must not permit arbitrary jumps"
    );
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(message.contains("migration-only"), "{message}");
    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["lifecycle"], json!("draft"));
}

#[test]
fn bulk_set_lifecycle_is_rejected_as_migration_only() {
    let env = TestEnv::new();
    let id1 = make_milestone(&env, "bulk-set-1");
    let id2 = make_milestone(&env, "bulk-set-2");

    let out = env.run(&[
        "milestone",
        "bulk",
        "set-lifecycle",
        "--ids",
        &format!("{id1},{id2}"),
        "--status",
        "approved",
    ]);
    assert!(
        !out.status.success(),
        "bulk raw lifecycle assignment must be migration-only"
    );
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(message.contains("migration-only"), "{message}");
    for id in [&id1, &id2] {
        let meta = milestone_meta(&env, id);
        assert_eq!(meta["lifecycle"], json!("draft"), "{} ", id);
    }
}

#[test]
fn bulk_set_spec_status_dry_run_uses_transition_table() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "bulk-spec-dry-run");
    approve_and_start(&env, &id);
    // M196: pass `--skip-review` so the milestone reaches terminal
    // `complete` (the test pins the transition table for the
    // terminal→non-terminal case).
    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "terminal fixture",
        "--skip-review",
    ]);
    assert!(complete.status.success());

    let preview = env.run(&[
        "milestone",
        "bulk",
        "set-spec-status",
        "--ids",
        &id,
        "--status",
        "review",
        "--dry-run",
    ]);
    assert!(
        preview.status.success(),
        "bulk dry-run reports per-id failures in JSON while exiting zero"
    );
    let payload: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(payload["failed"], 1);
    assert_eq!(payload["results"][0]["ok"], false);
    let message = payload["results"][0]["error"].as_str().unwrap();
    assert!(
        message.contains("invalid milestone transition"),
        "{message}"
    );
    assert_eq!(milestone_meta(&env, &id)["lifecycle"], json!("complete"));
}

// ── M189 stage-9 external findings F-02 / F-03 / F-05 / F-06 / F-07 ──

#[test]
fn set_spec_status_verified_refuses_complete_ceremony_bypass() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "no-verified-bypass");
    approve_and_start(&env, &id);

    let out = env.run(&["milestone", "set-spec-status", &id, "verified"]);
    assert!(
        !out.status.success(),
        "set-spec-status verified must refuse Complete bypass"
    );
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        message.contains("milestone complete") || message.contains("complete ceremony"),
        "{message}"
    );
    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["lifecycle"], json!("in-progress"));
    assert_ne!(meta["spec_status"], json!("verified"));
}

#[test]
fn set_status_dry_run_reports_gates_matching_live_path() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "set-status-dry-run-honesty");

    let preview = env.run(&["milestone", "set-status", &id, "in-progress", "--dry-run"]);
    assert!(
        preview.status.success(),
        "dry-run exits 0: {}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let gates = payload["gates"].as_array().expect("gates present");
    assert!(
        !gates.is_empty(),
        "draft→in-progress dry-run must surface start gates; got {payload}"
    );
    assert!(
        payload["fields"]
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(true),
        "blocked dry-run must not claim field flips; got {payload}"
    );

    let live = env.run(&["milestone", "set-status", &id, "in-progress"]);
    assert!(!live.status.success(), "live path must also reject");
}

#[test]
fn set_status_done_dry_run_requires_verified_like_live() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "done-needs-verified-dry");
    approve_and_start(&env, &id);

    let preview = env.run(&["milestone", "set-status", &id, "done", "--dry-run"]);
    assert!(preview.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let gates = payload["gates"].as_array().unwrap();
    assert!(
        gates.iter().any(|g| {
            g["message"]
                .as_str()
                .unwrap_or("")
                .contains("requires spec_status verified")
        }),
        "dry-run must report verified-required; got {payload}"
    );
}

#[test]
fn complete_and_approve_dry_run_show_lifecycle_transition() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "dry-run-lifecycle-flips");
    // Fresh create is draft; approve dry-run after grooming via set-spec-status.
    let _ = env.run(&["milestone", "set-spec-status", &id, "review"]);
    // Ready may be gated; use approve live only after ensuring gates pass via approve path.
    // For dry-run of approve on draft: transition table still projects approved.
    let approve_preview = env.run(&["milestone", "approve", &id, "--dry-run"]);
    assert!(approve_preview.status.success());
    let ap: serde_json::Value = serde_json::from_slice(&approve_preview.stdout).unwrap();
    assert!(
        ap["fields"]["lifecycle"].is_object() || ap["fields"]["spec_status"].is_object(),
        "approve dry-run must show transition effects; got {ap}"
    );

    approve_and_start(&env, &id);
    let complete_preview = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "dry-run lifecycle pin",
        "--dry-run",
    ]);
    assert!(
        complete_preview.status.success(),
        "{}",
        String::from_utf8_lossy(&complete_preview.stderr)
    );
    let cp: serde_json::Value = serde_json::from_slice(&complete_preview.stdout).unwrap();
    assert_eq!(
        cp["fields"]["lifecycle"]["after"],
        json!("complete"),
        "complete dry-run must preview lifecycle=complete; got {cp}"
    );
}

#[test]
fn block_and_cancel_refuse_on_complete() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "no-overlay-on-complete");
    approve_and_start(&env, &id);
    // M196: pass `--skip-review` so the milestone reaches terminal
    // `complete` (the test wants to assert the overlay refusal on a
    // terminal milestone, not the review gate).
    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "terminal for overlay refuse",
        "--skip-review",
    ]);
    assert!(complete.status.success());

    let block = env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "should-fail",
        "--by",
        "test",
    ]);
    assert!(!block.status.success(), "block on complete must fail");
    let cancel = env.run(&["milestone", "set-status", &id, "cancelled"]);
    assert!(!cancel.status.success(), "cancel on complete must fail");
    let meta = milestone_meta(&env, &id);
    assert_eq!(meta["lifecycle"], json!("complete"));
    assert_eq!(meta["blocked"], json!(false));
    assert_eq!(meta["cancelled"], json!(false));
}
