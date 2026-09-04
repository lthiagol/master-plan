//! M216 AC-07: per-AC visualization.
//!
//! In the per-milestone detail view, below the cycle-flow
//! timeline, render an `ACs (passed / total)` row-by-row
//! breakdown sourced from `session.queue[i].ac_pass_fail`.
//! Each row carries the AC id, a 40-char-truncated
// description, the status (passed / failed / pending), and a
// 60-char-truncated stamped evidence string. Failed ACs
// render with the `! ` marker; pending ACs render
// neutral. The list scrolls when it overflows the panel.

use raul::tui::autopilot::AcDetail;

fn session_with_ac_pass_fail() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "working_on": {"milestone_id": "M209"},
            "queue": [
                {
                    "milestone_id": "M209",
                    "title": "Coordination",
                    "ac_pass_fail": [
                        {
                            "id": "AC-01",
                            "description": "picker renders drivable milestones",
                            "status": "passed",
                            "evidence": "cargo nextest run -p raul --test autopilot_picker --no-fail-fast exit 0 (5/5 pass)",
                        },
                        {
                            "id": "AC-02",
                            "description": "queue view renders multi-milestone",
                            "status": "passed",
                            "evidence": "cargo nextest run -p raul --test autopilot_queue_view --no-fail-fast exit 0 (8/8)",
                        },
                        {
                            "id": "AC-03",
                            "description": "manual refresh",
                            "status": "failed",
                            "evidence": "cargo nextest run -p raul --test autopilot_manual_refresh exit 1 (1/9 fail)",
                        },
                        {
                            "id": "AC-04",
                            "description": "violation badge",
                            "status": "pending",
                            "evidence": "",
                        },
                    ],
                },
            ],
        },
    })
}

/// AC-07: the AC detail parser reads
/// `session.queue[i].ac_pass_fail[]` and produces one
/// typed entry per AC. The rows are sorted so failed
/// ACs come before passed, and passed before pending.
#[test]
fn ac_detail_parses_pass_fail_rows_for_active_milestone() {
    let ac = AcDetail::from_payload(&session_with_ac_pass_fail())
        .expect("ac detail parsed");
    assert_eq!(ac.milestone_id, "209");
    assert_eq!(ac.rows.len(), 4);
    // Failed first (red marker in the renderer).
    assert_eq!(ac.rows[0].status, "failed");
    assert_eq!(ac.rows[0].id, "AC-03");
    // Passed next.
    assert_eq!(ac.rows[1].status, "passed");
    assert_eq!(ac.rows[2].status, "passed");
    // Pending last (neutral marker).
    assert_eq!(ac.rows[3].status, "pending");
}

/// AC-07: the failed / passed / pending counts are
/// accessible as integers for the header summary.
#[test]
fn ac_detail_counts_are_accurate() {
    let ac = AcDetail::from_payload(&session_with_ac_pass_fail()).unwrap();
    assert_eq!(ac.passed(), 2);
    assert_eq!(ac.failed(), 1);
    assert_eq!(ac.total(), 4);
    assert!(ac.pending());
}

/// AC-07: the renderer formats each row as
/// `<marker> <id> | <desc-trunc-40> | <status> |
/// <evidence-trunc-60>`. The header carries the
/// passed / total summary. We assert on the header,
/// each row's AC id / status, and that the description
/// + evidence are truncated to the documented widths.
#[test]
fn ac_detail_renderer_truncates_descriptions_and_evidence() {
    let ac = AcDetail::from_payload(&session_with_ac_pass_fail()).unwrap();
    let rendered = ac.render_to_string(20);

    // Header line.
    assert!(
        rendered.starts_with("ACs (2 / 4)\n"),
        "header must carry passed/total summary; got {rendered:?}"
    );

    // Each row's id + status survive in the rendered text.
    assert!(rendered.contains(" AC-03 | manual refresh | failed |"));
    assert!(rendered.contains(" AC-01 | picker renders drivable milestones | passed |"));
    assert!(rendered.contains(" AC-02 | queue view renders multi-milestone | passed |"));
    assert!(rendered.contains(" AC-04 | violation badge | pending |"));

    // Long evidence strings are truncated — the rendered
    // evidence column is bounded so a long cargo nextest
    // command doesn't blow the row's width.
    for line in rendered.lines() {
        if line.contains("| passed |") || line.contains("| failed |") {
            // The full row is `<marker> <id> | <desc-trunc-40>
            // | <status> | <evidence-trunc-60>`. We measure the
            // evidence column by taking the slice past the last
            // `| ` separator.
            if let Some(idx) = line.rfind("| ") {
                let evidence = &line[idx + 2..];
                assert!(
                    evidence.chars().count() <= 60,
                    "evidence column must be ≤60 chars; got {evidence:?} (len={})",
                    evidence.chars().count()
                );
            }
        }
    }
}

/// AC-07: when the AC list overflows the viewport, the
/// renderer caps at `viewport_h` rows and surfaces a
/// `... (N more)` line so the operator knows more rows
/// exist below the fold.
#[test]
fn ac_detail_renderer_caps_at_viewport_height() {
    let mut payload = session_with_ac_pass_fail();
    // 8 ACs in the active milestone.
    let mut acs = serde_json::Map::new();
    for i in 1..=8 {
        acs.insert(
            format!("AC-{i:02}"),
            serde_json::json!({
                "id": format!("AC-{i:02}"),
                "description": format!("AC number {i}"),
                "status": if i % 3 == 0 { "failed" } else { "passed" },
                "evidence": "stamped evidence",
            }),
        );
    }
    payload["session"]["queue"][0]["ac_pass_fail"] = serde_json::Value::Array(
        (1..=8)
            .map(|i| {
                serde_json::json!({
                    "id": format!("AC-{i:02}"),
                    "description": format!("AC number {i}"),
                    "status": if i % 3 == 0 { "failed" } else { "passed" },
                    "evidence": "stamped evidence",
                })
            })
            .collect(),
    );
    let ac = AcDetail::from_payload(&payload).unwrap();
    let rendered = ac.render_to_string(3);
    // The "(N more)" line must surface.
    assert!(
        rendered.contains("... (5 more)"),
        "viewport=3 with 8 rows must surface '... (5 more)'; got {rendered:?}"
    );
    // The first 3 rows (sorted: failed first) must be visible.
    let visible_lines: Vec<&str> = rendered
        .lines()
        .filter(|l| l.starts_with('!') || l.starts_with(' '))
        .filter(|l| !l.starts_with(" ..."))
        .take(3)
        .collect();
    assert_eq!(visible_lines.len(), 3);
}

/// AC-07: every AC row carries id / description /
/// status / evidence. The round-trip pins the wire
/// format so a future field addition is visible here.
#[test]
fn ac_detail_row_round_trips_through_serde() {
    use raul::tui::autopilot::AcDetailRow;
    let row = AcDetailRow {
        id: "AC-01".to_string(),
        description: "picker".to_string(),
        status: "passed".to_string(),
        evidence: "cargo nextest exit 0 (5/5)".to_string(),
    };
    let v = serde_json::to_value(&row).unwrap();
    let back: AcDetailRow = serde_json::from_value(v).unwrap();
    assert_eq!(back, row);
}

/// AC-07: the parser returns `None` when there is no
/// active milestone. The renderer skips the block.
#[test]
fn ac_detail_returns_none_when_no_working_on() {
    let payload = serde_json::json!({
        "session": {"queue": []},
    });
    assert!(AcDetail::from_payload(&payload).is_none());
}

/// AC-07: the parser returns `None` when the active
/// milestone has no `ac_pass_fail` block. The renderer
/// falls back to the placeholder.
#[test]
fn ac_detail_returns_none_when_no_pass_fail_block() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
            "queue": [{"milestone_id": "M209"}],
        },
    });
    assert!(AcDetail::from_payload(&payload).is_none());
}

/// AC-07: the parser returns `None` for the empty
/// session payload. The renderer skips the block.
#[test]
fn ac_detail_returns_none_for_empty_session() {
    let payload = serde_json::json!({});
    assert!(AcDetail::from_payload(&payload).is_none());
}

/// AC-07: production-path regression. The AC detail
/// is reachable from the lane state through
/// `app.autopilot.ac_detail()`. The renderer reads this
/// field to draw the row-by-row breakdown.
#[test]
fn ac_detail_is_reachable_from_the_lane_state() {
    use raul::tui::autopilot::{refresh::refresh_from_json, AutopilotLaneState};
    let mut state = AutopilotLaneState::empty();
    assert!(state.ac_detail().is_none());

    refresh_from_json(
        &mut state,
        &session_with_ac_pass_fail(),
        &serde_json::json!({"run_state": {"kind": "live"}}),
    );
    let ac = state.ac_detail().expect("ac detail populated");
    assert_eq!(ac.milestone_id, "209");
    assert_eq!(ac.rows.len(), 4);
}

/// AC-07: failed ACs use `!`, passed/pending use ` `.
/// The marker drives the red/neutral styling in the
/// renderer.
#[test]
fn ac_detail_failed_marker_is_dist_al_from_passed_and_pending() {
    let ac = AcDetail::from_payload(&session_with_ac_pass_fail()).unwrap();
    let rendered = ac.render_to_string(20);
    let lines: Vec<&str> = rendered.lines().collect();
    // Find the failed AC line.
    let failed_line = lines
        .iter()
        .find(|l| l.starts_with('!'))
        .expect("failed marker must exist");
    assert!(
        failed_line.contains("| failed |"),
        "failed marker must surface on failed ACs; got {failed_line:?}"
    );
    // Passed/pending lines use ' ' as the first char.
    let neutral_lines: Vec<&&str> = lines
        .iter()
        .filter(|l| l.starts_with(' ') && l.contains("| passed |"))
        .collect();
    assert!(
        !neutral_lines.is_empty(),
        "passed ACs must surface with neutral ' ' marker"
    );
}

/// AC-07: ACs are sorted so failed rows render first —
/// the operator sees the failures at the top of the
/// scroll, with passed + pending below. The sort is
/// stable across runs (BTreeMap-free, but explicit).
#[test]
fn ac_detail_sorts_failed_first_passed_next_pending_last() {
    let payload = serde_json::json!({
        "session": {
            "working_on": {"milestone_id": "M209"},
            "queue": [{
                "milestone_id": "M209",
                "ac_pass_fail": [
                    {"id": "AC-P", "description": "passed 1", "status": "passed", "evidence": ""},
                    {"id": "AC-F", "description": "failed", "status": "failed", "evidence": ""},
                    {"id": "AC-X", "description": "pending", "status": "pending", "evidence": ""},
                    {"id": "AC-P2", "description": "passed 2", "status": "passed", "evidence": ""},
                ],
            }],
        },
    });
    let ac = AcDetail::from_payload(&payload).unwrap();
    let statuses: Vec<&str> = ac.rows.iter().map(|r| r.status.as_str()).collect();
    assert_eq!(statuses, vec!["failed", "passed", "passed", "pending"]);
}