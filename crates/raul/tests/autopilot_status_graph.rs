//! M216 AC-01: live status graph.
//!
//! The status graph renders one row per pane in the live
//! session. Each row carries five fields:
//!
//! - `label` — the herdr pane label (`role-<role>-<N>`)
//! - `pane_id` — the herdr pane id (`%5` / `%7`)
//! - `role_skill` — the skill binding (`mp-runner` / `mp-coordinator`)
//! - `last_notify` — the most recent notify timestamp
//! - `last_verdict` — the most recent verifier verdict
//!
//! The graph is sourced from the combined `mp autopilot
//! session show <id>` + `mp autopilot status` payload pair
//! (no `autopilot-control` legacy, no plan-zone read) and
//! rendered as a multi-line `String` so the golden tests
//! can pin the format verbatim.

use raul::tui::autopilot::StatusGraph;

fn session_show_payload() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "queue": [
                {
                    "label": "role-runner-1",
                    "role": "runner",
                    "role_skill": "mp-runner",
                    "last_notify": "2026-09-04T00:01:00Z",
                    "verifier_verdict": "pass",
                },
                {
                    "label": "role-coordinator-1",
                    "role": "coordinator",
                    "role_skill": "mp-coordinator",
                    "last_notify": "2026-09-04T00:01:30Z",
                    "verifier_verdict": "needs-review",
                },
                {
                    "label": "role-reviewer-1",
                    "role": "reviewer",
                    "role_skill": "mp-reviewer",
                    "last_notify": "2026-09-04T00:00:30Z",
                    "verifier_verdict": "pass",
                },
            ],
        },
    })
}

fn status_payload() -> serde_json::Value {
    serde_json::json!({
        "run_state": {"kind": "live"},
        "state": {
            "pane_ids": {
                "runner": "%5",
                "coordinator": "%7",
                "reviewer": "%9",
            },
        },
    })
}

/// AC-01: the status graph parses one pane row per
/// `session.queue[]` entry, carrying the five mandated
/// fields. The runner is the first row; the cursor
/// marker (`>`) lives on it.
#[test]
fn status_graph_renders_one_row_per_pane_with_five_fields() {
    let graph = StatusGraph::from_payloads(&session_show_payload(), &status_payload());
    let rendered = graph.render_to_string();
    let expected = "\
 Status graph (alpha) — run_state=live
 label | pane | role_skill | last_notify | last_verdict
 ------+------+------------+-------------+--------------
> role-runner-1 | %5 | mp-runner | 2026-09-04T00:01:00Z | pass
  role-coordinator-1 | %7 | mp-coordinator | 2026-09-04T00:01:30Z | needs-review
  role-reviewer-1 | %9 | mp-reviewer | 2026-09-04T00:00:30Z | pass
";
    assert_eq!(rendered, expected);
}

/// AC-01: when `session.queue` is empty but
/// `state.pane_ids` is populated, the graph synthesizes
/// one row per pane id so the graph still draws (the
/// session hasn't recorded per-pane rows yet).
#[test]
fn status_graph_synthesizes_rows_when_queue_is_empty() {
    let session_show = serde_json::json!({
        "session_id": "beta",
        "session": {"id": "beta", "queue": []},
    });
    let graph = StatusGraph::from_payloads(&session_show, &status_payload());
    assert_eq!(graph.rows.len(), 3);
    assert_eq!(graph.rows[0].label, "role-runner-1");
    assert_eq!(graph.rows[0].pane_id, "%5");
    assert_eq!(graph.rows[1].label, "role-coordinator-1");
    assert_eq!(graph.rows[2].label, "role-reviewer-1");
}

/// AC-01: the run-state classifier (live / stale / terminal)
/// surfaces in the graph header. The renderer reads it
/// verbatim from `mp autopilot status`.
#[test]
fn status_graph_surfaces_run_state_in_header() {
    for kind in ["live", "stale", "terminal"] {
        let status = serde_json::json!({
            "run_state": {"kind": kind},
            "state": {"pane_ids": {"runner": "%5"}},
        });
        let graph = StatusGraph::from_payloads(&session_show_payload(), &status);
        assert_eq!(graph.run_state, kind);
        assert!(
            graph.render_to_string().contains(&format!("run_state={kind}")),
            "header must carry run_state={kind}"
        );
    }
}

/// AC-01: the cursor marker (`>`) is always on the first
/// row, regardless of which pane the row represents. The
/// graph is read-only — there is no picker cursor to
/// scroll the rows.
#[test]
fn status_graph_cursor_marker_lands_on_first_row() {
    let graph = StatusGraph::from_payloads(&session_show_payload(), &status_payload());
    let rendered = graph.render_to_string();
    let first_line_with_marker = rendered
        .lines()
        .find(|l| l.starts_with('>'))
        .expect("cursor row must exist");
    assert!(
        first_line_with_marker.contains("role-runner-1"),
        "first row must be the cursor row (always row 0); got {first_line_with_marker:?}"
    );
}

/// AC-01: an empty graph (no rows, no session) renders
/// the "(no panes recorded)" placeholder. The renderer
/// never crashes on an empty payload — the lane falls
/// back to a placeholder while waiting for the next
/// refresh.
#[test]
fn status_graph_empty_renders_placeholder() {
    let graph = StatusGraph::empty();
    let rendered = graph.render_to_string();
    assert!(rendered.contains("no panes recorded"));
}

/// AC-01: the typed `PaneRow` payload round-trips
/// through serde. The production renderer reads the
/// typed fields directly; the round-trip test pins the
/// wire format so a future field addition is visible
/// here.
#[test]
fn pane_row_round_trips_through_serde() {
    use raul::tui::autopilot::PaneRow;
    let row = PaneRow {
        label: "role-runner-1".to_string(),
        pane_id: "%5".to_string(),
        role_skill: "mp-runner".to_string(),
        last_notify: "2026-09-04T00:01:00Z".to_string(),
        last_verdict: "pass".to_string(),
    };
    let v = serde_json::to_value(&row).unwrap();
    let back: PaneRow = serde_json::from_value(v).unwrap();
    assert_eq!(back, row);
}

/// AC-01: production-path regression. The renderer is
/// reachable from the lane state through the public
/// `autopilot.status_graph().render_to_string()` path.
/// This is what `render_watch_lane` consumes in
/// production (M216 S01 hot-path wire).
#[test]
fn status_graph_is_reachable_from_the_lane_state() {
    use raul::tui::autopilot::AutopilotLaneState;
    let mut state = AutopilotLaneState::empty();
    assert!(state.status_graph().is_none());
    let graph = StatusGraph::from_payloads(&session_show_payload(), &status_payload());
    state.status_graph = Some(graph);
    let rendered = state.status_graph().unwrap().render_to_string();
    assert!(rendered.starts_with(" Status graph (alpha) — run_state=live"));
}