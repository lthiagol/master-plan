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

// ── M202: flow_stages wiring + serde round-trip ─────────────────────────────
//
// AC-12 pins two contracts:
//   1. Pre-M202 milestone JSON without `flow_stages` loads cleanly; the
//      field populates via auto-derive on the next lifecycle transition
//      and round-trips through serde without altering the body.
//   2. FlowStage serializes with the expected shape ({status, at?}).
//
// AC-02..AC-05, AC-09, AC-19 pin the durable-writer wiring: every
// MilestoneEvent that flows through `apply_transition` also writes the
// corresponding mp-flow stage mutations. Hand-off is intentionally NOT
// touched by any auto-advance event (AC-11, covered separately in S11).

#[test]
fn legacy_milestone_without_flow_stages_loads_round_trip() {
    // Pre-M202 on-disk shape: a milestone JSON object with NO `flow_stages`
    // field at all. serde_json::from_str must load it without error and
    // produce a MilestoneMeta whose `flow_stages` is the empty BTreeMap.
    let legacy_json = r#"{
        "milestone": {
            "id": "01",
            "title": "Legacy pre-M202",
            "slug": "legacy",
            "lifecycle": "approved",
            "spec_status": "",
            "execution_status": "",
            "blocked": false,
            "needs_regrooming": false,
            "cancelled": false,
            "cancelled_at": null,
            "cancel_reason": null,
            "deferred": false,
            "deferred_reason": "",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-08-01",
            "updated": "2026-08-01",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
            "target_version": "",
            "executed_by": "",
            "remediation_pre_state": null
        },
        "intent": {"outcome": "ship"},
        "problem": {"description": "need"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [],
        "design_decisions": [],
        "open_questions": [],
        "work_packages": [],
        "steps": [],
        "findings": []
    }"#;
    let m1: mp_model::MilestoneFile =
        serde_json::from_str(legacy_json).expect("legacy JSON must load without flow_stages");
    assert!(
        m1.milestone.flow_stages.is_empty(),
        "legacy load must produce an empty flow_stages map; got: {:?}",
        m1.milestone.flow_stages
    );
    // Round-trip: re-serialize then re-load must produce identical bytes
    // for the body. The skip_serializing_if predicate omits the key when
    // the map is empty, so a healthy legacy file stays healthy legacy.
    let reserialized = serde_json::to_string(&m1).expect("serialize");
    let m2: mp_model::MilestoneFile =
        serde_json::from_str(&reserialized).expect("reload after serialize");
    let v1 = serde_json::to_value(&m1).unwrap();
    let v2 = serde_json::to_value(&m2).unwrap();
    assert_eq!(
        v1, v2,
        "round-trip must preserve the milestone body byte-identically"
    );
    // The reserialized body must NOT contain the `flow_stages` key when
    // the map is empty (skip_serializing_if contract).
    assert!(
        !reserialized.contains("\"flow_stages\""),
        "empty flow_stages must be omitted from serialized output; got: {reserialized}"
    );
}

#[test]
fn flow_stage_serde_round_trip() {
    // FlowStage serializes with the expected shape: a JSON object with
    // `status` always present and `at` present only when set. The skip
    // for `at: None` keeps pre-M202 fields byte-clean.
    use mp_model::FlowStage;
    let pending = FlowStage {
        status: "pending".to_string(),
        at: None,
    };
    let pending_json = serde_json::to_value(&pending).unwrap();
    assert_eq!(pending_json["status"], "pending");
    assert!(
        pending_json.get("at").is_none(),
        "at=None must skip the key on serialize; got: {pending_json}"
    );
    let done = FlowStage {
        status: "done".to_string(),
        at: Some("2026-09-01T00:00:00Z".to_string()),
    };
    let done_json = serde_json::to_value(&done).unwrap();
    assert_eq!(done_json["status"], "done");
    assert_eq!(done_json["at"], "2026-09-01T00:00:00Z");
    // Round-trip: serialize then deserialize — both objects must match.
    let done_round: FlowStage = serde_json::from_value(done_json.clone()).unwrap();
    assert_eq!(done_round.status, "done");
    assert_eq!(done_round.at.as_deref(), Some("2026-09-01T00:00:00Z"));
    let pending_round: FlowStage = serde_json::from_value(pending_json.clone()).unwrap();
    assert_eq!(pending_round.status, "pending");
    assert!(pending_round.at.is_none());
}

#[test]
fn groom_marks_draft_and_groom_done() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "groom-flow-stages");

    // AC-02: the Groom MilestoneEvent (Draft → Groomed transition)
    // results in `flow_stages.draft.status == "done"` and
    // `flow_stages.groom.status == "done"` with `at` timestamps set
    // on read. The mp milestone groom CLI is read-only (it produces a
    // GroomReport); the Groom transition fires through `mp milestone
    // set-spec-status review`, which routes through event_for_spec_status
    // → MilestoneEvent::Groom.
    let out = env.run(&["milestone", "set-spec-status", &id, "review"]);
    assert!(
        out.status.success(),
        "set-spec-status review failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages must be present after a lifecycle transition");
    assert_eq!(
        flow["draft"]["status"],
        json!("done"),
        "groom must flip flow_stages.draft.status to done"
    );
    assert_eq!(
        flow["groom"]["status"],
        json!("done"),
        "groom must flip flow_stages.groom.status to done"
    );
    assert!(
        flow["draft"]["at"].is_string() && !flow["draft"]["at"].as_str().unwrap().is_empty(),
        "draft.at must be a non-empty RFC3339 timestamp; got: {}",
        flow["draft"]["at"]
    );
    assert!(
        flow["groom"]["at"].is_string() && !flow["groom"]["at"].as_str().unwrap().is_empty(),
        "groom.at must be a non-empty RFC3339 timestamp; got: {}",
        flow["groom"]["at"]
    );
    // AC-11 negative: hand-off must NOT auto-advance on any event.
    assert!(
        flow.get("hand-off").is_none()
            || flow["hand-off"]["status"] != "done",
        "groom must never auto-advance hand-off; got: {}",
        flow.get("hand-off").cloned().unwrap_or_default()
    );
}

#[test]
fn approve_marks_specify_and_approve_done() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "approve-flow-stages");

    let out = env.run(&["milestone", "approve", &id]);
    assert!(
        out.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after approve");
    assert_eq!(flow["specify"]["status"], json!("done"));
    assert_eq!(flow["approve"]["status"], json!("done"));
    // draft may be null (legacy pre-groom milestone skipped Groom) OR
    // status==done (full draft→groom path). The contract here is just
    // that the Approve step wrote the two new stages; do not over-pin.
    let draft = flow.get("draft");
    assert!(
        draft.is_none() || draft.unwrap() == &serde_json::Value::Null,
        "draft must be absent or null when approve fires without groom; got: {draft:?}"
    );
}

#[test]
fn start_marks_execute_in_progress() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "start-flow-stages");
    approve_and_start(&env, &id);

    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after start");
    assert_eq!(
        flow["execute"]["status"],
        json!("in_progress"),
        "start must flip flow_stages.execute.status to in_progress"
    );
    // self-review must NOT have been flipped to in_progress yet — that
    // happens at Complete / FinishExecution.
    let self_review = flow.get("self-review");
    assert!(
        self_review.is_none() || self_review.unwrap()["status"] != "in_progress",
        "self-review must stay pending after Start; got: {self_review:?}"
    );
}

#[test]
fn complete_marks_execute_self_review_and_complete_done() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "complete-flow-stages");
    approve_and_start(&env, &id);
    // AC-05: complete_milestone must set execute, self-review, AND
    // complete stages to done in one transition (bundled per M148).
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "M202 flow_stages pin",
        "--skip-review",
    ]);
    assert!(
        out.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after complete");
    assert_eq!(flow["execute"]["status"], json!("done"));
    assert_eq!(flow["self-review"]["status"], json!("done"));
    assert_eq!(flow["complete"]["status"], json!("done"));
    // All three must share a timestamp from the completion event — pin
    // that the same `now_rfc3339()` call set them in one write.
    let exec_at = flow["execute"]["at"].as_str().unwrap();
    assert_eq!(flow["self-review"]["at"].as_str().unwrap(), exec_at);
    assert_eq!(flow["complete"]["at"].as_str().unwrap(), exec_at);
}

#[test]
fn cancel_marks_remaining_stages_skipped() {
    let env = TestEnv::new();
    let id = make_milestone(&env, "cancel-flow-stages");
    // Run the full draft→groom→approve→start path so draft, groom,
    // specify, approve all flip to done and execute is in_progress.
    // Cancel then leaves execute + the rest as skipped.
    let groom = env.run(&["milestone", "set-spec-status", &id, "review"]);
    assert!(
        groom.status.success(),
        "groom failed: {}",
        String::from_utf8_lossy(&groom.stderr)
    );
    let approve = env.run(&["milestone", "approve", &id]);
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let start = env.run(&["milestone", "set-status", &id, "in-progress"]);
    assert!(
        start.status.success(),
        "set-status in-progress failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );

    let out = env.run(&["milestone", "set-status", &id, "cancelled"]);
    assert!(
        out.status.success(),
        "set-status cancelled failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after cancel");
    // Pre-cancel: draft, groom, specify, approve are done; execute is
    // in_progress. After cancel, the remaining non-done stages must
    // flip to skipped.
    assert_eq!(flow["execute"]["status"], json!("skipped"));
    assert_eq!(flow["self-review"]["status"], json!("skipped"));
    assert_eq!(flow["complete"]["status"], json!("skipped"));
    assert_eq!(flow["external-review"]["status"], json!("skipped"));
    // Done stages must stay done — cancel must NOT clobber them.
    assert_eq!(flow["draft"]["status"], json!("done"));
    assert_eq!(flow["groom"]["status"], json!("done"));
    assert_eq!(flow["specify"]["status"], json!("done"));
    assert_eq!(flow["approve"]["status"], json!("done"));
    // Hand-off must NEVER auto-advance, even on cancel (AC-11).
    assert!(
        flow.get("hand-off").is_none() || flow["hand-off"]["status"] != "done",
        "cancel must never auto-advance hand-off"
    );
}

#[test]
fn stage_list_prints_twelve_rows() {
    // AC-07: `mp milestone stage list <id>` prints all 12 stages as a
    // CLI table with id, status, and `at` (or `—` when unset). Exit 0.
    let env = TestEnv::new();
    let id = make_milestone(&env, "stage-list");
    let out = env.run(&["milestone", "stage", "list", &id]);
    assert!(
        out.status.success(),
        "stage list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // All 12 canonical stage slugs must appear in the output.
    for slug in [
        "draft",
        "groom",
        "specify",
        "approve",
        "execute",
        "self-review",
        "complete",
        "external-review",
        "remediate",
        "re-review",
        "document",
        "hand-off",
    ] {
        assert!(
            stdout.contains(slug),
            "stage list must include slug {slug}; got: {stdout}"
        );
    }
    // Brand-new milestone has no flow_stages entries — every row must
    // show `pending` and `—`.
    let pending_rows = stdout.matches("pending").count();
    let em_dash_rows = stdout.matches('—').count();
    assert!(
        pending_rows >= 12,
        "expected at least 12 pending rows for a fresh milestone; got {pending_rows}\n{stdout}"
    );
    assert!(
        em_dash_rows >= 12,
        "expected at least 12 em-dash timestamp rows for a fresh milestone; got {em_dash_rows}\n{stdout}"
    );
}

#[test]
fn stage_set_rejects_invalid_status() {
    // AC-08: invalid status exits non-zero with a precise error listing
    // the allowed values; the milestone file is unchanged.
    let env = TestEnv::new();
    let id = make_milestone(&env, "stage-set-bad-status");
    let out = env.run(&["milestone", "stage", "set", &id, "draft", "bogus"]);
    assert!(
        !out.status.success(),
        "stage set bogus must exit non-zero; got status={:?}",
        out.status.code()
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "invalid status must exit 2; got {:?}",
        out.status.code()
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("invalid status") || combined.contains("bogus"),
        "error must reference the bad value; got: {combined}"
    );
    assert!(
        combined.contains("pending")
            && combined.contains("done")
            && combined.contains("in_progress")
            && combined.contains("skipped"),
        "error must enumerate the 4 allowed statuses; got: {combined}"
    );
    // On-disk milestone must be unchanged.
    let meta = milestone_meta(&env, &id);
    let flow = meta.get("flow_stages").and_then(|v| v.as_object());
    assert!(
        flow.is_none() || flow.unwrap().is_empty(),
        "invalid stage set must not touch flow_stages; got: {flow:?}"
    );
}

#[test]
fn stage_set_rejects_unknown_stage_key() {
    // Same guard shape as AC-08 but for the stage-slug side.
    let env = TestEnv::new();
    let id = make_milestone(&env, "stage-set-bad-key");
    let out = env.run(&["milestone", "stage", "set", &id, "not-a-stage", "done"]);
    assert!(
        !out.status.success(),
        "stage set bogus-stage must exit non-zero; got status={:?}",
        out.status.code()
    );
    assert_eq!(out.status.code(), Some(2));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(combined.contains("invalid stage"), "{combined}");
    assert!(combined.contains("not-a-stage"), "{combined}");
    // On-disk milestone must be unchanged.
    let meta = milestone_meta(&env, &id);
    let flow = meta.get("flow_stages").and_then(|v| v.as_object());
    assert!(
        flow.is_none() || flow.unwrap().is_empty(),
        "invalid stage key must not touch flow_stages; got: {flow:?}"
    );
}

#[test]
fn hand_off_only_advances_via_explicit_set() {
    // AC-11: hand-off must NEVER auto-advance. The only path that
    // touches flow_stages.hand-off.status is `mp milestone stage set
    // <id> hand-off done`. This test pins the explicit path so a
    // future widening of the auto-advance table can be caught by a
    // failing test rather than a silent regression.
    let env = TestEnv::new();
    let id = make_milestone(&env, "hand-off-explicit");
    approve_and_start(&env, &id);
    // Drive the full lifecycle to complete so every other stage has
    // a real status. hand-off must remain pending (or absent).
    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "M202 hand-off pin",
        "--skip-review",
    ]);
    assert!(complete.status.success());

    // Step 1: hand-off must NOT have been auto-flipped.
    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after complete");
    let hand_off = flow.get("hand-off");
    assert!(
        hand_off.is_none()
            || hand_off.unwrap()["status"] != "done",
        "hand-off must NOT auto-advance on complete; got: {hand_off:?}"
    );

    // Step 2: explicit set is the ONLY path that touches it.
    let explicit = env.run(&["milestone", "stage", "set", &id, "hand-off", "done"]);
    assert!(
        explicit.status.success(),
        "explicit stage set hand-off done failed: {}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let meta2 = milestone_meta(&env, &id);
    let flow2 = meta2
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after explicit set");
    assert_eq!(
        flow2["hand-off"]["status"], "done",
        "explicit stage set must close hand-off"
    );
}

#[test]
fn no_event_auto_advances_hand_off() {
    // AC-11 negative pin: every event in the auto-advance table
    // (Groom, Approve, Start, FinishExecution, Complete, EnterRemediation,
    // ExitRemediation, Cancel) MUST leave hand-off pending. The
    // model-level unit tests cover this at the function level; the
    // integration version exercises the durable writers end-to-end.
    let env = TestEnv::new();
    let id = make_milestone(&env, "hand-off-negatives");
    // Drive a representative subset of events.
    let _ = env.run(&["milestone", "set-spec-status", &id, "review"]);
    let _ = env.run(&["milestone", "approve", &id]);
    let _ = env.run(&["milestone", "set-status", &id, "in-progress"]);
    let meta = milestone_meta(&env, &id);
    let flow = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after groom→approve→start");
    let hand_off = flow.get("hand-off");
    assert!(
        hand_off.is_none() || hand_off.unwrap()["status"] != "done",
        "groom/approve/start must not auto-advance hand-off; got: {hand_off:?}"
    );
    // Now complete the milestone (with --skip-review for non-track
    // gating). Hand-off must STILL stay pending — auto-advance never
    // touches it.
    let _ = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "M202 hand-off negatives pin",
        "--skip-review",
    ]);
    let meta2 = milestone_meta(&env, &id);
    let flow2 = meta2
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after complete");
    let hand_off2 = flow2.get("hand-off");
    assert!(
        hand_off2.is_none() || hand_off2.unwrap()["status"] != "done",
        "complete must not auto-advance hand-off; got: {hand_off2:?}"
    );
    // Stages 1-7 are done after complete; stage 8 (external-review)
    // is in_progress (the milestone is sitting in the review queue);
    // stages 9-12 (remediate, re-review, document, hand-off) stay
    // pending. The AC-11 invariant is that hand-off stays pending
    // — verify the post-complete stage map so a future widening of
    // the auto-advance table can't silently flip hand-off.
    for slug in [
        "draft",
        "groom",
        "specify",
        "approve",
        "execute",
        "self-review",
        "complete",
    ] {
        assert_eq!(
            flow2[slug]["status"], "done",
            "stage {slug} must be done after complete; got: {}",
            flow2[slug]["status"]
        );
    }
    assert_eq!(
        flow2["external-review"]["status"], "in_progress",
        "external-review must be in_progress after complete (review queue)"
    );
    for slug in ["remediate", "re-review", "document", "hand-off"] {
        let s = flow2.get(slug);
        assert!(
            s.is_none() || s.unwrap()["status"] != "done",
            "stage {slug} must NOT be done after complete; got: {s:?}"
        );
    }
}

#[test]
fn explicit_stage_set_survives_subsequent_lifecycle_transition() {
    // AC-06: `mp milestone stage set <id> external-review done` overrides
    // the auto-derived value for that stage only. Subsequent `complete`
    // lifecycle transitions update other stages but do NOT clobber the
    // explicitly-set `external-review`.
    let env = TestEnv::new();
    let id = make_milestone(&env, "explicit-override");
    approve_and_start(&env, &id);

    // Step 1: explicit set on external-review → done.
    let explicit = env.run(&[
        "milestone",
        "stage",
        "set",
        &id,
        "external-review",
        "done",
    ]);
    assert!(
        explicit.status.success(),
        "stage set external-review done failed: {}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let meta = milestone_meta(&env, &id);
    let flow_before = meta
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after explicit set");
    assert_eq!(flow_before["external-review"]["status"], json!("done"));

    // Step 2: run a lifecycle transition that WOULD normally write
    // external-review. We re-approve and complete (with --skip-review)
    // so the Complete transition fires.
    let _ = env.run(&["milestone", "approve", &id]);
    let complete = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "M202 explicit-set survives",
        "--skip-review",
    ]);
    assert!(complete.status.success());

    let meta_after = milestone_meta(&env, &id);
    let flow_after = meta_after
        .get("flow_stages")
        .and_then(|v| v.as_object())
        .expect("flow_stages present after complete");
    // Lifecycle flipped to complete; stage entries reflect that.
    assert_eq!(flow_after["complete"]["status"], json!("done"));
    // BUT external-review must STILL be done (the explicit override),
    // not in_progress which Complete would normally write.
    assert_eq!(
        flow_after["external-review"]["status"],
        json!("done"),
        "explicit stage set must survive a subsequent lifecycle transition (AC-06); got: {}",
        flow_after["external-review"]["status"]
    );
}
