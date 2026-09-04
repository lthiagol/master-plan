//! M216 AC-08: cross-milestone telemetry.
//!
//! Active-session telemetry aggregates:
//! - Total time-in-active-stage across all three panes
//!   (Working / Reviewing / Deciding).
//! - Attempts-per-stage across the queue
//!   (Dispatching / WaitingRunner / Reviewing / Deciding /
//   AwaitingUser).
//! - Per-AC pass/fail counts across milestones.
//!
//! Aggregates are computed from session.json on each
//! refresh; they reset on session start (the operator
//! restarts the picker / refresh on a new session).

use raul::tui::autopilot::Telemetry;

/// Synthetic session payload with:
/// - 1 shipped milestone (M207) — `complete` outcome.
/// - 1 in-flight milestone (M209) — `reviewing` / `working`
///   role state with `last_state_change_at`.
/// - 1 pending milestone (M211) — no role state, no
///   history rows yet.
fn synthetic_session() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "working_on": {"milestone_id": "M209"},
            "queue": [
                {
                    "milestone_id": "M207",
                    "title": "Pilot S2",
                    "ac_pass_fail": [
                        {"id": "AC-01", "description": "picker", "status": "passed"},
                        {"id": "AC-02", "description": "panel", "status": "passed"},
                    ],
                },
                {
                    "milestone_id": "M209",
                    "title": "Coordination",
                    "ac_pass_fail": [
                        {"id": "AC-03", "description": "refresh", "status": "passed"},
                        {"id": "AC-04", "description": "violation", "status": "failed"},
                        {"id": "AC-05", "description": "queue view", "status": "pending"},
                    ],
                },
                {
                    "milestone_id": "M211",
                    "title": "Reconcile",
                },
            ],
            "queue_cycle_history": [
                {"milestone_id": "M207", "cycle": 1, "outcome": "reviewer-pass"},
                {"milestone_id": "M209", "cycle": 1, "outcome": "reviewer-reject"},
                {"milestone_id": "M209", "cycle": 2, "outcome": "reviewer-pass"},
                {"milestone_id": "M209", "cycle": 3},
            ],
            "role_state": {
                "roles": {
                    "runner": {
                        "state": "working",
                        "last_state_change_at": "2026-09-04T00:00:00Z",
                        "now": "2026-09-04T00:01:30Z",
                    },
                    "coordinator": {
                        "state": "reviewing",
                        "last_state_change_at": "2026-09-04T00:00:30Z",
                        "now": "2026-09-04T00:01:30Z",
                    },
                    "reviewer": {
                        "state": "idle",
                        "last_state_change_at": "2026-09-04T00:00:00Z",
                    },
                },
            },
        },
    })
}

/// AC-08: telemetry parses all three aggregates from
/// the session-show envelope.
#[test]
fn telemetry_parses_all_three_aggregates_from_session() {
    let t = Telemetry::from_payload(&synthetic_session());
    // Total time-in-active-stage: runner (90s) +
    // coordinator (60s); idle is excluded.
    assert_eq!(t.total_time_in_active_stage, "2m 30s");
    // Attempts-per-stage: reviewer-pass (2), reviewer-reject (1).
    let reviewer_pass = t
        .attempts_per_stage
        .iter()
        .find(|(s, _)| s == "reviewer-pass")
        .map(|(_, c)| *c);
    let reviewer_reject = t
        .attempts_per_stage
        .iter()
        .find(|(s, _)| s == "reviewer-reject")
        .map(|(_, c)| *c);
    assert_eq!(reviewer_pass, Some(2));
    assert_eq!(reviewer_reject, Some(1));
    // Per-AC pass/fail counts across milestones.
    // AC-03: 1 passed / 0 failed.
    // AC-04: 0 passed / 1 failed.
    let ac03 = t
        .per_ac_pass_fail
        .iter()
        .find(|(id, _, _)| id == "AC-03")
        .expect("AC-03 must appear");
    assert_eq!(ac03, &("AC-03".to_string(), 1, 0));
    let ac04 = t
        .per_ac_pass_fail
        .iter()
        .find(|(id, _, _)| id == "AC-04")
        .expect("AC-04 must appear");
    assert_eq!(ac04, &("AC-04".to_string(), 0, 1));
}

/// AC-08: the renderer surfaces all three aggregates
/// in canonical order. The golden-file test pins the
/// format.
#[test]
fn telemetry_renderer_canonical_order() {
    let t = Telemetry::from_payload(&synthetic_session());
    let rendered = t.render_to_string();
    // The renderer surfaces the labeled key-value block
    // in alphabetical order (BTreeMap-backed) so the
    // golden output is stable across runs.
    let expected = "\
Telemetry
 total_time_in_active_stage = 2m 30s
 attempts_per_stage:
  reviewer-pass = 2
  reviewer-reject = 1
 per_ac_pass_fail:
  AC-01 = passed:1 failed:0
  AC-02 = passed:1 failed:0
  AC-03 = passed:1 failed:0
  AC-04 = passed:0 failed:1
  AC-05 = passed:0 failed:0
";
    assert_eq!(rendered, expected);
}

/// AC-08: idle role states are excluded from the
/// time-in-active-stage sum. Only Working / Reviewing /
/// Deciding count.
#[test]
fn telemetry_idle_states_excluded_from_time_in_active_stage() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
            "role_state": {
                "roles": {
                    "runner": {"state": "idle", "last_state_change_at": "2026-09-04T00:00:00Z", "now": "2026-09-04T00:01:30Z"},
                    "coordinator": {"state": "blocked", "last_state_change_at": "2026-09-04T00:00:30Z", "now": "2026-09-04T00:01:30Z"},
                },
            },
        },
    });
    let t = Telemetry::from_payload(&payload);
    assert_eq!(t.total_time_in_active_stage, "0m 0s");
}

/// AC-08: the attempts-per-stage aggregate counts
/// empty-outcome entries as zero. The `queue_cycle_history`
/// may include entries with no `outcome` (in-flight cycles).
#[test]
fn telemetry_skips_empty_outcome_entries() {
    let payload = serde_json::json!({
        "session": {
            "queue_cycle_history": [
                {"milestone_id": "M209", "cycle": 1, "outcome": "reviewer-pass"},
                {"milestone_id": "M209", "cycle": 2}, // no outcome
                {"milestone_id": "M209", "cycle": 3, "outcome": ""}, // empty outcome
            ],
        },
    });
    let t = Telemetry::from_payload(&payload);
    let pass = t
        .attempts_per_stage
        .iter()
        .find(|(s, _)| s == "reviewer-pass")
        .map(|(_, c)| *c);
    assert_eq!(pass, Some(1));
    assert_eq!(t.attempts_per_stage.len(), 1);
}

/// AC-08: empty `role_state` block → total_time_in_active_stage
/// is "0m 0s" (no working / reviewing / deciding roles).
#[test]
fn telemetry_empty_role_state_yields_zero_time() {
    let payload = serde_json::json!({"session": {}});
    let t = Telemetry::from_payload(&payload);
    assert_eq!(t.total_time_in_active_stage, "0m 0s");
    assert!(t.attempts_per_stage.is_empty());
    assert!(t.per_ac_pass_fail.is_empty());
}

/// AC-08: the per-AC pass / fail counts aggregate across
/// milestones. AC-01 appears in M207 only (1 passed) —
/// not "duplicated" by an empty AC-01 row in M209.
#[test]
fn telemetry_per_ac_pass_fail_aggregates_across_milestones() {
    let payload = serde_json::json!({
        "session": {
            "queue": [
                {
                    "milestone_id": "M207",
                    "ac_pass_fail": [
                        {"id": "AC-01", "status": "passed"},
                        {"id": "AC-02", "status": "passed"},
                    ],
                },
                {
                    "milestone_id": "M209",
                    "ac_pass_fail": [
                        {"id": "AC-01", "status": "failed"},
                        {"id": "AC-03", "status": "passed"},
                    ],
                },
            ],
        },
    });
    let t = Telemetry::from_payload(&payload);
    let ac01 = t
        .per_ac_pass_fail
        .iter()
        .find(|(id, _, _)| id == "AC-01")
        .expect("AC-01 must appear");
    assert_eq!(ac01, &("AC-01".to_string(), 1, 1));
    let ac02 = t
        .per_ac_pass_fail
        .iter()
        .find(|(id, _, _)| id == "AC-02")
        .expect("AC-02 must appear");
    assert_eq!(ac02, &("AC-02".to_string(), 1, 0));
    let ac03 = t
        .per_ac_pass_fail
        .iter()
        .find(|(id, _, _)| id == "AC-03")
        .expect("AC-03 must appear");
    assert_eq!(ac03, &("AC-03".to_string(), 1, 0));
}

/// AC-08: `Telemetry` round-trips through serde. The
/// wire format pins `total_time_in_active_stage` /
/// `attempts_per_stage` / `per_ac_pass_fail`; the
/// round-trip pins the contract.
#[test]
fn telemetry_round_trips_through_serde() {
    let t = Telemetry {
        milestone_id: "209".to_string(),
        total_time_in_active_stage: "2m 30s".to_string(),
        attempts_per_stage: vec![("reviewer-pass".to_string(), 2)],
        per_ac_pass_fail: vec![("AC-01".to_string(), 1, 0)],
    };
    let v = serde_json::to_value(&t).unwrap();
    let back: Telemetry = serde_json::from_value(v).unwrap();
    assert_eq!(back, t);
}

/// AC-08: production-path regression. The telemetry
/// block is reachable from the lane state through
/// `app.autopilot.telemetry()`. The renderer reads this
/// field to draw the cross-milestone aggregates.
#[test]
fn telemetry_is_reachable_from_the_lane_state() {
    use raul::tui::autopilot::{refresh::refresh_from_json, AutopilotLaneState};
    let mut state = AutopilotLaneState::empty();
    assert!(state.telemetry().is_none());

    refresh_from_json(
        &mut state,
        &synthetic_session(),
        &serde_json::json!({"run_state": {"kind": "live"}}),
    );
    let t = state.telemetry().expect("telemetry populated");
    assert_eq!(t.milestone_id, "209");
    assert!(!t.attempts_per_stage.is_empty());
    assert!(!t.per_ac_pass_fail.is_empty());
}

/// AC-08: the telemetry block is computed from
/// session.json state. It is independent of the
/// picker state — it does not consume `mp list
/// milestones`.
#[test]
fn telemetry_does_not_consume_list_milestones() {
    let t = Telemetry::from_payload(&synthetic_session());
    // The function signature takes only a session-show
    // payload — no list-milestones payload, no
    // filesystem reads. The contract is pinned by the
    // signature; a future addition of a list-milestones
    // argument would change every call site.
    let _: fn(&serde_json::Value) -> Telemetry = Telemetry::from_payload;
    // Sanity: the synthetic session has no `list_milestones`
    // block, yet telemetry produces 5 per-AC rows + 2 attempts.
    assert_eq!(t.per_ac_pass_fail.len(), 5);
    assert_eq!(t.attempts_per_stage.len(), 2);
}

/// AC-08: aggregates reset on session start. The
/// `Telemetry::from_payload` adapter is called from
/// `refresh_from_json`, which is called from the
/// dispatcher's `Action::AutopilotRefresh`. The
/// operator restarts the picker + refresh on a new
/// session, so the aggregates are recomputed from
/// the new session.json state.
#[test]
fn telemetry_recomputes_on_session_refresh() {
    use raul::tui::autopilot::Telemetry;

    // Session 1 (alpha).
    let t1 = Telemetry::from_payload(&synthetic_session());
    assert_eq!(t1.milestone_id, "209");

    // Session 2 (beta) — fresh state, different
    // milestone.
    let session_beta = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M211"},
            "queue": [
                {"milestone_id": "M211", "ac_pass_fail": []},
            ],
            "queue_cycle_history": [],
            "role_state": {"roles": {}},
        },
    });
    let t2 = Telemetry::from_payload(&session_beta);
    assert_eq!(t2.milestone_id, "211");
    // Aggregates reset — t2 has 0 attempts, 0 per-AC rows.
    assert!(t2.attempts_per_stage.is_empty());
    assert!(t2.per_ac_pass_fail.is_empty());
}