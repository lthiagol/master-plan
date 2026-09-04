//! M216 AC-05: detail panel.
//!
//! The detail pane never reads `activity.json` or any
//! plan-zone file directly. It consumes:
//!
//! - `mp autopilot session show <id>` (events,
//!   queue_cycle_history, working_on, last_state_change_at,
//!   cycle_cap)
//! - `mp reviews finding list <mid>` (findings[])
//! - M213 next-action APIs (next_action.action)
//!
//! The golden-file tests cover: cycles (events),
//! findings (reviews), history (queue_cycle_history),
//! drift (last_state_change_at), cap (cycle_cap).

use raul::tui::autopilot::DetailPanel;

fn session_show_with_history() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "working_on": {
                "milestone_id": "M209",
                "cycle": 2,
                "role": "runner",
            },
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "runner", "body": "starting"},
                {"seq": 2, "kind": "transition", "actor": "runner", "body": "in-flight"},
                {"seq": 3, "kind": "verifier", "actor": "verifier", "body": "ok"},
            ],
            "queue_cycle_history": [
                {"milestone_id": "M209", "cycle": 1, "started_at": "2026-09-04T00:00:00Z", "outcome": "reviewer-pass"},
                {"milestone_id": "M209", "cycle": 2, "started_at": "2026-09-04T00:01:00Z"},
                {"milestone_id": "M211", "cycle": 1, "started_at": "2026-09-04T00:00:00Z", "outcome": "executed"},
            ],
            "last_state_change_at": "2026-09-04T00:01:00Z",
            "cycle_cap": 4,
        },
    })
}

/// AC-05: the detail panel renders cycles, findings,
/// history, drift, and cap. The golden-file tests pin
/// each block's format.
#[test]
fn detail_panel_renders_all_five_blocks() {
    let findings = vec![
        "F-01: missing notify on role-runner-1".to_string(),
        "F-02: stale cycle > cap".to_string(),
    ];
    let next_action = serde_json::json!({"action": "rerun"});
    let panel =
        DetailPanel::from_payloads(&session_show_with_history(), &findings, &Some(next_action));
    let rendered = panel.render_to_string();
    let expected = "\
Detail (209)
 drift=2026-09-04T00:01:00Z cap=4
 cycles:
  - dispatch (runner)
  - transition (runner)
  - verifier (verifier)
 findings:
  - F-01: missing notify on role-runner-1
  - F-02: stale cycle > cap
  - next-action: rerun
 history:
  - cycle=1 outcome=reviewer-pass
  - cycle=2 outcome=
";
    assert_eq!(rendered, expected);
}

/// AC-05: the milestone id is sourced from
/// `session.working_on.milestone_id` (or
/// `session.active_milestone`). The `M` prefix is
/// stripped so the detail pane matches the picker ids.
#[test]
fn detail_panel_strips_m_prefix_from_milestone_id() {
    let panel = DetailPanel::from_payloads(&session_show_with_history(), &[], &None);
    assert_eq!(panel.milestone_id, "209");
    assert!(!panel.milestone_id.starts_with('M'));
}

/// AC-05: history rows for OTHER milestones are
/// filtered out. The `history` block carries only the
/// rows matching the active milestone.
#[test]
fn detail_panel_history_filters_to_active_milestone_only() {
    let panel = DetailPanel::from_payloads(&session_show_with_history(), &[], &None);
    // M211 rows must NOT appear in M209's history.
    assert_eq!(panel.history.len(), 2);
    assert!(
        panel.history.iter().all(|h| h.starts_with("cycle=")),
        "history rows must start with 'cycle='"
    );
}

/// AC-05: drift defaults to "fresh" when no
/// `last_state_change_at` is set. The renderer
/// surfaces drift in the header.
#[test]
fn detail_panel_drift_defaults_to_fresh() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
        },
    });
    let panel = DetailPanel::from_payloads(&payload, &[], &None);
    assert_eq!(panel.drift, "fresh");
}

/// AC-05: cap defaults to `cap=∞` when no `cycle_cap`
/// is set. The renderer surfaces cap in the header.
#[test]
fn detail_panel_cap_defaults_to_infinity() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
        },
    });
    let panel = DetailPanel::from_payloads(&payload, &[], &None);
    assert_eq!(panel.cap, "cap=∞");
}

/// AC-05: next-action is appended as a finding-row
/// entry. The renderer reads it as a regular finding
/// so the operator sees the action hint in the same
/// block as the reviews findings.
#[test]
fn detail_panel_appends_next_action_as_a_finding_row() {
    let panel = DetailPanel::from_payloads(
        &session_show_with_history(),
        &[],
        &Some(serde_json::json!({"action": "rerun"})),
    );
    assert!(
        panel
            .findings
            .iter()
            .any(|f| f.contains("next-action: rerun")),
        "next-action must surface in findings; got {:?}",
        panel.findings
    );
}

/// AC-05: empty findings + no next-action → empty
/// `findings` Vec. The placeholder "(none)" is what
/// the renderer surfaces.
#[test]
fn detail_panel_handles_empty_findings() {
    let panel = DetailPanel::from_payloads(&session_show_with_history(), &[], &None);
    assert!(panel.findings.is_empty());
    assert!(
        panel.render_to_string().contains("findings:\n  (none)"),
        "empty findings must surface the '(none)' placeholder"
    );
}

/// AC-05: empty cycles → empty `cycles` Vec. The
/// placeholder "(none)" is what the renderer surfaces.
#[test]
fn detail_panel_handles_empty_cycles() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
            "events": [],
        },
    });
    let panel = DetailPanel::from_payloads(&payload, &[], &None);
    assert!(panel.cycles.is_empty());
}

/// AC-05: the `DetailPanel` payload round-trips
/// through serde. The wire format pins `milestone_id` /
/// `cycles / `findings` / `history` / `drift` / `cap`;
/// the round-trip pins the contract so a future
/// field addition is visible here.
#[test]
fn detail_panel_round_trips_through_serde() {
    let panel = DetailPanel {
        milestone_id: "209".to_string(),
        cycles: vec!["dispatch (runner)".to_string()],
        findings: vec!["F-01".to_string()],
        history: vec!["cycle=1 outcome=ok".to_string()],
        drift: "fresh".to_string(),
        cap: "cap=4".to_string(),
    };
    let v = serde_json::to_value(&panel).unwrap();
    let back: DetailPanel = serde_json::from_value(v).unwrap();
    assert_eq!(back, panel);
}

/// AC-05: the detail panel never reads plan-zone
/// files directly. The public surface exposes only
/// `from_payloads` (pure function) + `render_to_string`
/// and the accessors. The public surface has no path
/// argument and no `std::fs::read` call.
#[test]
fn detail_panel_has_no_filesystem_inputs() {
    let _: fn(&serde_json::Value, &[String], &Option<serde_json::Value>) -> DetailPanel =
        DetailPanel::from_payloads;
}

/// AC-05: production-path regression. The detail
/// panel is reachable from the lane state through
/// `app.autopilot.detail_panel()`. The detail-pane
/// dispatcher reads this field to render the panel.
#[test]
fn detail_panel_is_reachable_from_the_lane_state() {
    use raul::tui::autopilot::{refresh::refresh_from_json, AutopilotLaneState};
    let mut state = AutopilotLaneState::empty();
    assert!(state.detail_panel().is_none());

    refresh_from_json(
        &mut state,
        &session_show_with_history(),
        &serde_json::json!({"run_state": {"kind": "live"}}),
    );
    let panel = state.detail_panel().expect("detail panel populated");
    assert_eq!(panel.milestone_id, "209");
    assert!(!panel.cycles.is_empty());
    assert!(!panel.history.is_empty());
    assert_eq!(panel.cap, "cap=4");
}
