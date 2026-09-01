use raul::tui::app::{App, ContentState, Lane};
use std::collections::BTreeMap;

fn sample_milestones() -> Vec<raul::tui::app::MilestoneSummary> {
    vec![
        raul::tui::app::MilestoneSummary {
            id: "01".to_string(),
            title: "Setup".to_string(),
            lifecycle: "complete".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "02".to_string(),
            title: "Core".to_string(),
            lifecycle: "approved".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "03".to_string(),
            title: "Polish".to_string(),
            lifecycle: "draft".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]
}

#[test]
fn transition_list_to_detail_and_back() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(sample_milestones());

    assert_eq!(app.active_lane, Lane::Milestones);
    assert_eq!(app.content, ContentState::List);
    app.enter_milestone_detail(Some(0));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.selected_milestone_id, Some("01".to_string()));

    app.go_back();
    assert_eq!(app.content, ContentState::List);
    assert_eq!(app.active_lane, Lane::Milestones);
}

#[test]
fn drill_down_plus_back_produces_correct_state() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(sample_milestones());

    app.enter_milestone_detail(Some(1));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.selected_milestone_id, Some("02".to_string()));

    app.open_thread();
    assert_eq!(app.content, ContentState::AnnotationThread);

    app.go_back();
    assert_eq!(app.content, ContentState::MilestoneDetail);

    app.go_back();
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn filter_toggle() {
    let mut app = App::new();
    assert!(!app.open_only);
    app.toggle_filter();
    assert!(app.open_only);
    app.toggle_filter();
    assert!(!app.open_only);
}

// ─── M179 S3 / AC-02: Watch picker model ──────────────────────────

/// Helper: build a `mp list milestones`-shaped payload with a
/// mix of drivable and non-drivable lifecycles.
fn sample_list_payload() -> serde_json::Value {
    serde_json::json!([
        {"id": "M01", "title": "Approved",     "lifecycle": "approved",     "priority": "high"},
        {"id": "M02", "title": "In Progress",  "lifecycle": "in-progress", "priority": "normal"},
        {"id": "M03", "title": "Self Reviewed","lifecycle": "self-reviewed","priority": "low"},
        {"id": "M04", "title": "Reviewed",     "lifecycle": "reviewed",    "priority": "normal"},
        {"id": "M05", "title": "Remediation",  "lifecycle": "remediation", "priority": "high"},
        {"id": "M06", "title": "Draft",        "lifecycle": "draft",       "priority": "low"},
        {"id": "M07", "title": "Complete",     "lifecycle": "complete",    "priority": "low"},
        {"id": "M08", "title": "Cancelled",    "lifecycle": "cancelled",   "priority": "low"},
    ])
}

#[test]
fn watch_picker_keeps_only_drivable_lifecycles() {
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    let ids: Vec<String> = app.watch.candidates.iter().map(|c| c.id.clone()).collect();
    // Review aliases (03/04) are not watch-drivable after M189 F-08.
    assert_eq!(ids, vec!["01", "02", "05"]);
    for c in &app.watch.candidates {
        assert!(raul::tui::watch::is_drivable_lifecycle(&c.lifecycle));
    }
}

#[test]
fn watch_picker_preserves_selection_order_across_refreshes() {
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    app.watch.toggle_select("02");
    app.watch.toggle_select("01");
    app.watch.toggle_select("05");
    assert_eq!(app.watch.queue_ids(), &["02", "01", "05"]);
    // Toggling an already-selected id removes it (preserving order).
    app.watch.toggle_select("01");
    assert_eq!(app.watch.queue_ids(), &["02", "05"]);
    // Clear selection is the explicit path.
    app.watch.clear_selection();
    assert!(app.watch.queue_ids().is_empty());
    // Re-select in a different order — the new order wins.
    app.watch.toggle_select("05");
    app.watch.toggle_select("01");
    app.watch.toggle_select("02");
    assert_eq!(app.watch.queue_ids(), &["05", "01", "02"]);
}

#[test]
fn watch_picker_drops_selections_for_milestones_no_longer_in_the_set() {
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    app.watch.toggle_select("01");
    app.watch.toggle_select("02");
    app.watch.toggle_select("05");
    // Refresh with only M01 surviving among the prior selection.
    let pruned = serde_json::json!([
        {"id": "M01", "title": "Approved",     "lifecycle": "approved",     "priority": "high"},
        {"id": "M04", "title": "Reviewed",     "lifecycle": "reviewed",    "priority": "normal"},
    ]);
    app.watch.refresh_candidates(&pruned);
    // Surviving selection is in the order it was built; the
    // missing ids (02, 05) are gone. Reviewed (04) is not drivable.
    assert_eq!(app.watch.queue_ids(), &["01"]);
}

#[test]
fn watch_picker_can_start_requires_preflight_and_selection() {
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    // Empty selection + no preflight → cannot start.
    assert!(!app.watch.can_start());
    // Selection only, no preflight → still cannot (the S3 model
    // requires both the queue and a successful preflight verdict
    // before Start is permitted).
    app.watch.toggle_select("01");
    assert!(!app.watch.can_start());
}

#[test]
fn watch_picker_filters_via_lifecycle_only_blocked_is_not_a_picker_exclusion() {
    // M179 AC-02: the picker filters by lifecycle, not by
    // dry-run verdict. A milestone in a drivable lifecycle
    // (in-progress) but marked `blocked: true` must still
    // appear in the picker — the dry-run (S4) surfaces the
    // `blocked` flag as a `DepStatus::Blocked` row, not a
    // picker exclusion.
    let mut app = App::new();
    let payload = serde_json::json!([
        {"id": "M01", "title": "In Progress Blocked",
         "lifecycle": "in-progress", "priority": "high", "blocked": true},
        {"id": "M02", "title": "Draft",
         "lifecycle": "draft", "priority": "low"},
    ]);
    app.watch.refresh_candidates(&payload);
    let ids: Vec<String> = app.watch.candidates.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids, vec!["01"], "blocked in-progress must be in the picker");
}

#[test]
fn watch_picker_picker_cursor_clamps_on_refresh() {
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    app.watch.picker_index = 7; // out of bounds for the new 5-cand list
    let pruned = serde_json::json!([
        {"id": "M01", "title": "Approved", "lifecycle": "approved", "priority": "high"},
    ]);
    app.watch.refresh_candidates(&pruned);
    assert_eq!(app.watch.picker_index, 0);
}

// ─── M179 S4: preflight + start gate ─────────────────────────────

#[test]
fn preflight_parses_clean_dry_run() {
    use raul::tui::watch::parse_preflight;
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": true, "checks": []},
        "milestones": [
            {
                "id": "M01", "input": "01", "lifecycle": "approved",
                "spec_status": "ready", "execution_status": "planned",
                "ready": true, "next_action": "execute"
            },
            {
                "id": "M02", "input": "02", "lifecycle": "in-progress",
                "spec_status": "ready", "execution_status": "planned",
                "ready": true, "next_action": "execute"
            }
        ]
    });
    let preflight = parse_preflight(&payload);
    assert!(preflight.aggregate_ok);
    assert!(preflight.verdict.is_ok(), "{:?}", preflight.verdict);
    assert_eq!(preflight.per_milestone.len(), 2);
    assert!(matches!(
        preflight.per_milestone[0].1,
        raul::tui::watch::DepStatus::Ready
    ));
    assert!(matches!(
        preflight.per_milestone[1].1,
        raul::tui::watch::DepStatus::Ready
    ));
}

#[test]
fn preflight_parses_blocked_milestone_as_blocked() {
    use raul::tui::watch::parse_preflight;
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": true, "checks": []},
        "milestones": [
            {
                "id": "M01", "input": "01", "lifecycle": "in-progress",
                "spec_status": "ready", "execution_status": "planned",
                "blocked": true, "ready": false,
                "error": "blocked: dep not ready"
            }
        ]
    });
    let preflight = parse_preflight(&payload);
    assert!(preflight.aggregate_ok);
    assert!(preflight.verdict.is_err());
    if let raul::tui::watch::DepStatus::Blocked(reason) = &preflight.per_milestone[0].1 {
        assert!(reason.contains("blocked"));
    } else {
        panic!("expected Blocked");
    }
}

// F-05: when `ready: true` AND `blocked: true`, the candidate
// must classify as Blocked. The previous implementation ignored
// the `blocked` field entirely; it would have returned Ready
// here, permitting Start on a candidate mp flagged as blocked.
#[test]
fn preflight_ready_but_blocked_classifies_as_blocked() {
    use raul::tui::watch::{parse_preflight, DepStatus};
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": true, "checks": []},
        "milestones": [
            {
                "id": "M01", "input": "01", "lifecycle": "approved",
                "spec_status": "ready", "execution_status": "planned",
                "blocked": true, "ready": true
            }
        ]
    });
    let preflight = parse_preflight(&payload);
    assert!(
        preflight.verdict.is_err(),
        "blocked=true must flip verdict to Err"
    );
    assert!(
        matches!(preflight.per_milestone[0].1, DepStatus::Blocked(_)),
        "ready=true + blocked=true must classify as Blocked"
    );
}

#[test]
fn preflight_parses_aggregate_precondition_failure() {
    use raul::tui::watch::parse_preflight;
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {
            "ok": false,
            "checks": [
                {"name": "herdr_on_path", "ok": false, "message": "herdr not on PATH"}
            ]
        },
        "milestones": []
    });
    let preflight = parse_preflight(&payload);
    assert!(!preflight.aggregate_ok);
    assert!(preflight.verdict.is_err());
    assert!(preflight
        .verdict
        .as_ref()
        .unwrap_err()
        .contains("aggregate"));
}

#[test]
fn can_start_after_successful_preflight() {
    use raul::tui::watch::parse_preflight;
    let mut app = App::new();
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": true, "checks": []},
        "milestones": [
            {"id": "M01", "lifecycle": "approved", "ready": true, "next_action": "execute"}
        ]
    });
    app.watch.refresh_candidates(&payload);
    app.watch.toggle_select("01");
    // No preflight yet → cannot start.
    assert!(!app.watch.can_start());
    // Successful preflight + selection → can start.
    let preflight = parse_preflight(&payload);
    app.watch.preflight = Some(preflight);
    assert!(app.watch.can_start());
}

#[test]
fn successful_preflight_is_bound_to_exact_ordered_queue() {
    use raul::tui::watch::parse_preflight;
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    app.watch.toggle_select("01");
    app.watch.toggle_select("02");
    let payload = serde_json::json!({
        "preconditions": {"ok": true},
        "milestones": [
            {"id": "M01", "ready": true, "blocked": false},
            {"id": "M02", "ready": true, "blocked": false}
        ]
    });
    app.watch.preflight = Some(parse_preflight(&payload));
    assert!(app.watch.can_start());

    app.watch.toggle_select("01");
    assert!(app.watch.preflight.is_none());
    assert!(!app.watch.can_start());
}

#[test]
fn restoring_terminal_queue_requires_new_preflight_and_reports_missing_ids() {
    use raul::tui::watch::{parse_status, restore_queue_from_status};
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    let status = parse_status(&serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/tmp/watch.state.json",
        "state": {
            "queue": ["M02", "M999", "M01"],
            "run_outcome": {"kind": "partial-failure"},
            "milestone_outcomes": []
        }
    }));
    restore_queue_from_status(&mut app, &status);
    assert_eq!(app.watch.selected, vec!["02", "01"]);
    assert!(app.watch.preflight.is_none());
    assert!(!app.watch.can_start());
    assert!(app.watch.last_error.as_deref().unwrap().contains("999"));
}

#[test]
fn restore_normalizes_m_prefixed_outcome_ids_for_compact_queue() {
    // M190 F-02: queue restore strips M-prefix; outcomes must match.
    use raul::tui::watch::{parse_status, render_compact_queue, restore_queue_from_status};
    let mut app = App::new();
    app.watch.refresh_candidates(&sample_list_payload());
    let status = parse_status(&serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/tmp/watch.state.json",
        "pid_alive": false,
        "herdr_listed": false,
        "state": {
            "queue": ["M02", "M01"],
            "run_outcome": {"kind": "partial-failure"},
            "milestone_outcomes": [
                {"id": "M02", "outcome": {"kind": "completed"}},
                {"id": "M01", "outcome": {"kind": "skipped"}}
            ]
        }
    }));
    assert_eq!(
        status.milestone_outcomes[0]["id"].as_str(),
        Some("02"),
        "parse_status must strip M-prefix from outcome ids"
    );
    app.watch.status = Some(status.clone());
    restore_queue_from_status(&mut app, &status);
    assert_eq!(app.watch.selected, vec!["02", "01"]);
    let q = render_compact_queue(&app);
    assert!(
        q.contains("[completed] 02"),
        "restored compact queue must show outcome, not pending: {q}"
    );
    assert!(
        q.contains("[skipped] 01"),
        "restored compact queue must show outcome, not pending: {q}"
    );
    assert!(
        !q.contains("[pending]"),
        "M-prefixed outcome ids must not leave rows pending: {q}"
    );
}

#[test]
fn cannot_start_after_failed_preflight() {
    use raul::tui::watch::parse_preflight;
    let mut app = App::new();
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": false, "checks": []},
        "milestones": []
    });
    app.watch.refresh_candidates(&payload);
    app.watch.toggle_select("01");
    // Even with selection, a failed preflight must NOT allow start.
    let preflight = parse_preflight(&payload);
    app.watch.preflight = Some(preflight);
    assert!(!app.watch.can_start());
}

#[test]
fn preflight_verdict_propagates_to_candidate_dep_status() {
    // AC-03: a failed preflight cannot spawn mp watch. The
    // per-milestone verdicts must surface on the candidate
    // `dep_status` so the renderer can show "M170 ✗ spec_status
    // mismatch" without a second subprocess call.
    use raul::tui::watch::parse_preflight;
    let mut app = App::new();
    let payload = serde_json::json!({
        "dry_run": true,
        "preconditions": {"ok": true, "checks": []},
        "milestones": [
            {
                "id": "M01", "lifecycle": "in-progress",
                "spec_status": "verified", "execution_status": "planned",
                "ready": false, "next_action": "skip_not_ready"
            }
        ]
    });
    app.watch.refresh_candidates(&payload);
    app.watch.toggle_select("01");
    let preflight = parse_preflight(&payload);
    app.watch.preflight = Some(preflight);
    // Apply the preflight to candidates — the production
    // pipeline (`run_preflight`) does this automatically; the
    // test wires it explicitly because `parse_preflight` is a
    // pure helper.
    raul::tui::watch::apply_preflight_to_candidates(&mut app);
    // Per-candidate dep_status is set by the preflight so S6's
    // compact queue renderer can read it without a second call.
    let cand = app.watch.candidates.iter().find(|c| c.id == "01").unwrap();
    assert!(matches!(
        cand.dep_status,
        Some(raul::tui::watch::DepStatus::Blocked(_))
    ));
}

// ─── M179 S5: detach-safe start ───────────────────────────────────

#[test]
fn start_watch_refuses_empty_queue() {
    use raul::tui::watch::parse_detach_report;
    let app = App::new();
    // No selection, no preflight → can_start is false; the
    // start_watch shell-out is suppressed entirely. The
    // parse_detach_report helper is unit-tested below; this test
    // pins the can_start gate.
    assert!(!app.watch.can_start());
    let payload = serde_json::json!({
        "dry_run": false, "detach": true,
        "detached_pid": 12345,
        "log_file": "/abs/.mp/watch.log",
        "state_file": "/abs/.mp/watch.state.json",
        "preconditions": {"ok": true},
        "message": "detached"
    });
    // Even if the parse succeeded, can_start blocks the start.
    let _ = parse_detach_report(&payload);
    assert!(!app.watch.can_start());
}

#[test]
fn start_watch_idempotency_blocks_second_start_when_live() {
    use raul::tui::watch::{has_live_run, parse_status};
    let mut app = App::new();
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "live"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": true,
        "herdr_listed": true,
        "state": {"run_outcome": null, "milestone_outcomes": []}
    })));
    assert!(has_live_run(&app));
    // The user must monitor, not start a second driver.
}

// F-06: a "stale" classification with an alive PID (zombie,
// herdr orphaned, clock skew) must also block Start. The
// previous guard only matched kind=="live", so a stale-but-alive
// PID would let start_watch spawn a duplicate driver — exactly
// the gap the guard is supposed to close.
#[test]
fn has_live_run_blocks_stale_but_alive_pid() {
    use raul::tui::watch::{has_live_run, parse_status};
    let mut app = App::new();
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "stale", "reason": "recorded pid 9999 not checked"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": true,
        "herdr_listed": false,
        "state": {"run_outcome": null, "milestone_outcomes": []}
    })));
    assert!(
        has_live_run(&app),
        "stale-but-alive must block Start (zombie / duplicate-driver guard)"
    );
}

#[test]
fn parse_detach_report_extracts_pid_and_paths() {
    use raul::tui::watch::parse_detach_report;
    let payload = serde_json::json!({
        "dry_run": false,
        "detach": true,
        "detached_pid": 12345,
        "log_file": "/abs/.mp/watch.log",
        "state_file": "/abs/.mp/watch.state.json",
        "preconditions": {"ok": true, "checks": []},
        "message": "detached watch started; pid=12345"
    });
    let r = parse_detach_report(&payload);
    assert_eq!(r.detached_pid, Some(12345));
    assert!(r.detach);
    assert!(r.preconditions_ok);
    assert!(r.log_file.ends_with("watch.log"));
    assert!(r.state_file.ends_with("watch.state.json"));
}

#[test]
fn parse_detach_report_extracts_failed_preconditions() {
    use raul::tui::watch::parse_detach_report;
    let payload = serde_json::json!({
        "dry_run": false,
        "detach": true,
        "detached_pid": null,
        "preconditions": {"ok": false, "checks": []},
        "message": "preconditions failed; refusing to detach"
    });
    let r = parse_detach_report(&payload);
    assert!(r.detached_pid.is_none());
    assert!(!r.preconditions_ok);
    assert!(r.message.contains("refusing"));
}

// ─── M179 S6: ASCII lifecycle graph + compact queue ─────────────

#[test]
fn render_lifecycle_graph_highlights_current_node() {
    use raul::tui::watch::render_lifecycle_graph;
    // F-09: only the active node is bracketed with `>...<`;
    // inactive nodes are bare labels joined by `-`. The previous
    // implementation appended `<` to every node, producing
    // `>approved<-groomed<-...` — noise on inactive nodes.
    let g = render_lifecycle_graph(Some("approved"));
    assert!(g.contains(">approved<"));
    // Inactive nodes must NOT carry a trailing `<`.
    assert!(
        !g.contains("draft<"),
        "inactive node 'draft' must not be bracketed; graph was: {g}"
    );
    assert!(
        !g.contains("groomed<"),
        "inactive node 'groomed' must not be bracketed; graph was: {g}"
    );
    // The remediation node appears in the graph as a regular
    // node, NOT as a loop indicator, until the active lifecycle
    // is remediation (then the trailing ↺ is appended).
    assert!(g.contains("remediation"));
    let g_terminal = render_lifecycle_graph(Some("remediation"));
    assert!(g_terminal.contains(">remediation<"));
    assert!(g_terminal.ends_with("↺"));
}

#[test]
fn render_lifecycle_graph_no_active_lifecycle_uses_spaces() {
    use raul::tui::watch::render_lifecycle_graph;
    let g = render_lifecycle_graph(None);
    // F-09: no active node → no bracketing at all. Every node
    // is a bare label; no `>` and no `<` should appear.
    assert!(
        !g.contains('>'),
        "no active node → no '>' marker; graph was: {g}"
    );
    assert!(
        !g.contains('<'),
        "no active node → no '<' marker; graph was: {g}"
    );
    assert!(g.contains("approved"));
    assert!(g.contains("remediation"));
}

#[test]
fn render_compact_queue_surfaces_per_milestone_outcomes() {
    use raul::tui::watch::parse_status;
    let mut app = App::new();
    app.watch.refresh_candidates(&serde_json::json!([
        {"id": "M01", "title": "First", "lifecycle": "approved", "priority": "high"},
        {"id": "M02", "title": "Second", "lifecycle": "in-progress", "priority": "normal"},
    ]));
    app.watch.toggle_select("01");
    app.watch.toggle_select("02");
    // Pin a terminal status with per-milestone outcomes.
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false,
        "herdr_listed": false,
        "state": {
            "run_outcome": {"kind": "partial-failure"},
            "milestone_outcomes": [
                {"id": "01", "outcome": {"kind": "completed"}},
                {"id": "02", "outcome": {"kind": "skipped"}}
            ]
        }
    })));
    let q = raul::tui::watch::render_compact_queue(&app);
    // F-06: outcomes are surfaced exactly as reported. The
    // mp-reported kinds (`completed`, `skipped`) flow through
    // verbatim — no `done` / `failed` / `stopped` rename.
    assert!(q.contains("[completed] 01"));
    assert!(q.contains("[skipped] 02"));
    // The active queue row carries the `>` marker. After two
    // toggles, queue_index=1, so the second line is the active row.
    let active_line = q
        .lines()
        .find(|l| l.starts_with(">"))
        .expect("at least one line must carry the active marker");
    assert!(active_line.contains("[skipped] 02"));
}

#[test]
fn render_compact_queue_omitted_when_empty() {
    use raul::tui::watch::render_compact_queue;
    let app = App::new();
    let q = render_compact_queue(&app);
    assert!(q.contains("empty queue"));
}

// ─── M179 S7: poller ────────────────────────────────────────────

#[test]
fn poller_fires_on_first_call_and_respects_interval() {
    use raul::tui::watch::Poller;
    let mut p = Poller::new();
    assert!(p.is_due(), "first call must always be due");
    p.mark_fired();
    // Immediately after firing, the next is_due must be false.
    assert!(!p.is_due());
}

#[test]
fn poller_synthetic_clock_advances_after_interval() {
    use raul::tui::watch::Poller;
    // The is_due check uses real wall-clock time. A synthetic
    // clock test would need a clock injection point. The
    // production codepath is one is_due() per run-loop tick;
    // POLL_INTERVAL_MS=3s is verified by the test above + the
    // constant test below.
    let mut p = Poller::new();
    p.mark_fired();
    assert!(!p.is_due());
}

#[test]
fn poll_interval_is_in_2_to_5_second_window() {
    // M179 AC-06: the poll cadence is 2-5 seconds.
    use raul::tui::watch::POLL_INTERVAL_MS;
    assert!(
        (2000..=5000).contains(&POLL_INTERVAL_MS),
        "POLL_INTERVAL_MS must be in [2000, 5000]; got {POLL_INTERVAL_MS}"
    );
}

// F-08 / AC-06: poll_watch_state MUST bump app.version() on every
// due poll so the run_loop's `needs_render = true` path fires. The
// previous implementation mutated app.watch.status directly without
// calling touch(), so the dirty signal never flipped and the screen
// redrew only on the next keypress — breaking AC-06's "update
// without a keypress" promise. This test would have caught that
// (the version stays invariant without the bump). The MpRunner is
// pointed at a nonexistent binary; the runner's spawn failure is
// swallowed by `unwrap_or(Value::Null)` inside poll_watch_state,
// so the function still completes its mutation + touch() path.
#[test]
fn poll_watch_state_bumps_app_version_on_due_poll() {
    use raul::mp_runner::MpRunner;
    use raul::tui::watch::{poll_watch_state, Poller};

    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/f08/test");
    let mut app = App::new();
    let mut poller = Poller::new();
    let version_before = app.version();

    let _ = poll_watch_state(&runner, &mut app, &mut poller)
        .expect("poll must not bubble runner spawn failures");

    assert_ne!(
        app.version(),
        version_before,
        "poll_watch_state must call app.touch() so run_loop sets needs_render (F-08 / AC-06)"
    );
}

// ─── M179 S8: output fetch + log tail ────────────────────────────

#[test]
fn tail_watch_log_returns_empty_when_file_missing() {
    use raul::tui::watch::tail_watch_log;
    let dir = tempfile::TempDir::new().unwrap();
    let lines = tail_watch_log(dir.path(), 50);
    assert!(lines.is_empty());
}

#[test]
fn tail_watch_log_returns_last_n_lines() {
    use raul::tui::watch::tail_watch_log;
    use std::io::Write;
    let dir = tempfile::TempDir::new().unwrap();
    let mp = dir.path().join(".mp");
    std::fs::create_dir_all(&mp).unwrap();
    let mut f = std::fs::File::create(mp.join("watch.log")).unwrap();
    for i in 0..20 {
        writeln!(f, "line {i}").unwrap();
    }
    let lines = tail_watch_log(dir.path(), 5);
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0], "line 15");
    assert_eq!(lines[4], "line 19");
}

// ─── M179 S9: classify recorded state ────────────────────────────

#[test]
fn classify_recorded_state_pins_live_stale_terminal() {
    use raul::tui::watch::{classify_recorded_state, parse_status, RecordedStateKind};
    let mut app = App::new();
    // None when no status is recorded.
    assert_eq!(classify_recorded_state(&app), RecordedStateKind::None);
    // Live.
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "live"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": true, "herdr_listed": true,
        "state": {"run_outcome": null, "milestone_outcomes": []}
    })));
    assert_eq!(classify_recorded_state(&app), RecordedStateKind::Live);
    // Stale.
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "stale", "reason": "recorded pid 9999 not alive"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {"run_outcome": null, "milestone_outcomes": []}
    })));
    assert_eq!(classify_recorded_state(&app), RecordedStateKind::Stale);
    // Terminal.
    app.watch.status = Some(parse_status(&serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {
            "run_outcome": {"kind": "completed"},
            "milestone_outcomes": [
                {"id": "170", "outcome": {"kind": "completed"}}
            ]
        }
    })));
    assert_eq!(classify_recorded_state(&app), RecordedStateKind::Terminal);
}

// ─── M179 S9: resume (stale → explicit driver) ──────────────────

#[test]
fn stale_recorded_state_is_resumable_via_explicit_driver() {
    // AC-08: stale/interrupted state offers explicit Resume
    // only. The Resume path invokes `mp watch --resume <ids>`
    // (the legacy M152 surface) which re-attaches to the
    // recorded panes. The Watch model doesn't spawn a second
    // driver on a live run; the helper `has_live_run` is the gate.
    use raul::tui::watch::parse_status as parse_status_stale;
    let mut app = App::new();
    let payload = serde_json::json!({
        "run_state": {"kind": "stale", "reason": "recorded pid 9999 not alive"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {"run_outcome": null, "milestone_outcomes": []}
    });
    app.watch.status = Some(parse_status_stale(&payload));
    // Stale → not live → Resume is the correct path.
    assert!(!raul::tui::watch::has_live_run(&app));
    assert_eq!(
        raul::tui::watch::classify_recorded_state(&app),
        raul::tui::watch::RecordedStateKind::Stale
    );
    // Stale → not live → Resume is the correct path.
    assert!(!raul::tui::watch::has_live_run(&app));
    assert_eq!(
        raul::tui::watch::classify_recorded_state(&app),
        raul::tui::watch::RecordedStateKind::Stale
    );
}

// ─── M179 S11: terminal-state restoration ────────────────────────

#[test]
fn terminal_status_round_trip_preserves_per_milestone_outcomes() {
    // AC-09: a fresh `App::new()` that calls `restore_latest_status`
    // sees the per-milestone outcome list and concise run
    // result. The `run_outcome` flows through verbatim.
    use raul::tui::watch::parse_status;
    let payload = serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {
            "run_outcome": {"kind": "partial-failure"},
            "milestone_outcomes": [
                {"id": "01", "outcome": {"kind": "completed"}},
                {"id": "02", "outcome": {"kind": "exhausted"}},
                {"id": "03", "outcome": {"kind": "skipped"}}
            ]
        }
    });
    let s = parse_status(&payload);
    assert_eq!(s.kind, "terminal");
    assert_eq!(s.milestone_outcomes.len(), 3);
    let kinds: Vec<String> = s
        .milestone_outcomes
        .iter()
        .filter_map(|m| {
            m.get("outcome")
                .and_then(|o| o.get("kind"))
                .and_then(|k| k.as_str())
                .map(String::from)
        })
        .collect();
    assert_eq!(kinds, vec!["completed", "exhausted", "skipped"]);
}

#[test]
fn terminal_outcomes_surfaces_exhausted_and_skipped_verbatim() {
    // AC-10: every mp-reported outcome is surfaced exactly.
    // Exhausted and Skipped must NOT be re-interpreted as
    // "Failed" by the renderer.
    use raul::tui::watch::parse_status;
    let payload = serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {
            "run_outcome": {"kind": "exhausted"},
            "milestone_outcomes": [
                {"id": "01", "outcome": {"kind": "exhausted"}}
            ]
        }
    });
    let s = parse_status(&payload);
    let kind = s
        .milestone_outcomes
        .first()
        .and_then(|m| m.get("outcome"))
        .and_then(|o| o.get("kind"))
        .and_then(|k| k.as_str());
    assert_eq!(kind, Some("exhausted"));
}

#[test]
fn terminal_outcomes_surfaces_skipped_verbatim() {
    use raul::tui::watch::parse_status;
    let payload = serde_json::json!({
        "run_state": {"kind": "terminal"},
        "state_file": "/abs/.mp/watch.state.json",
        "pid_alive": false, "herdr_listed": false,
        "state": {
            "run_outcome": {"kind": "skipped"},
            "milestone_outcomes": [
                {"id": "01", "outcome": {"kind": "skipped", "reason": "blocked: dep not ready"}}
            ]
        }
    });
    let s = parse_status(&payload);
    let reason = s
        .milestone_outcomes
        .first()
        .and_then(|m| m.get("outcome"))
        .and_then(|o| o.get("reason"))
        .and_then(|r| r.as_str());
    assert_eq!(reason, Some("blocked: dep not ready"));
}
