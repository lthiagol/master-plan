//! M216 AC-02: multi-milestone queue view.
//!
//! The queue view renders when the active session has more
//! than one milestone. The active milestone — the one
//! matching `session.working_on.milestone_id` (or the first
//! queue item when no `working_on` is set) — is highlighted
//! with the `>` prefix; idle rows use `  `.

use raul::tui::autopilot::QueueView;

fn session_show_with_three_milestones() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "status": "active",
            "working_on": {
                "milestone_id": "M209",
                "cycle": 1,
                "role": "runner",
            },
            "queue": [
                {
                    "milestone_id": "M207",
                    "title": "Pilot S2",
                    "lifecycle": "approved",
                },
                {
                    "milestone_id": "M209",
                    "title": "Coordination",
                    "lifecycle": "in-progress",
                },
                {
                    "milestone_id": "M211",
                    "title": "Reconcile",
                    "lifecycle": "approved",
                },
            ],
        },
    })
}

/// AC-02: the queue view parses one row per
/// `session.queue[]` entry, marking the active milestone
/// (the one matching `working_on.milestone_id`) with the
/// `>` prefix.
#[test]
fn queue_view_renders_with_active_milestone_highlighted() {
    let qv = QueueView::from_session_show(&session_show_with_three_milestones());
    let rendered = qv.render_to_string();
    let expected = "\
Multi-milestone queue (alpha) — status=active
  207 | approved | Pilot S2
> 209 | in-progress | Coordination
  211 | approved | Reconcile
";
    assert_eq!(rendered, expected);
}

/// AC-02: when no `working_on` is set, the first queue
/// item is treated as the active milestone. The renderer
/// uses `>` on the first row so the operator always sees a
/// highlighted entry.
#[test]
fn queue_view_falls_back_to_first_row_when_no_working_on() {
    let payload = serde_json::json!({
        "session_id": "beta",
        "session": {
            "id": "beta",
            "status": "active",
            "queue": [
                {"milestone_id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
                {"milestone_id": "M211", "title": "Reconcile", "lifecycle": "approved"},
            ],
        },
    });
    let qv = QueueView::from_session_show(&payload);
    let rendered = qv.render_to_string();
    let expected = "\
Multi-milestone queue (beta) — status=active
> 207 | approved | Pilot S2
  211 | approved | Reconcile
";
    assert_eq!(rendered, expected);
}

/// AC-02: the milestone id is stripped of the `M`
/// prefix so the queue matches the picker ids (which use
/// the un-prefixed form per M214). A row carrying `M209`
/// surfaces as `209`.
#[test]
fn queue_view_strips_m_prefix_from_milestone_ids() {
    let payload = session_show_with_three_milestones();
    let qv = QueueView::from_session_show(&payload);
    for row in &qv.rows {
        assert!(
            !row.milestone_id.starts_with('M'),
            "milestone id must be stripped of M prefix; got {:?}",
            row.milestone_id
        );
    }
}

/// AC-02: `active_milestone_id()` returns the active
/// milestone's id (un-prefixed). The detail-pane
/// dispatcher reads this to decide which milestone's
/// detail panel to show.
#[test]
fn queue_view_active_milestone_id_returns_highlighted_row() {
    let qv = QueueView::from_session_show(&session_show_with_three_milestones());
    assert_eq!(qv.active_milestone_id(), Some("209"));
}

/// AC-02: empty queue renders the placeholder. The
/// renderer never crashes on a payload with an empty
/// queue array — the lane falls back to the placeholder
/// while waiting for the next refresh.
#[test]
fn queue_view_empty_queue_renders_placeholder() {
    let payload = serde_json::json!({
        "session_id": "gamma",
        "session": {"id": "gamma", "status": "draft", "queue": []},
    });
    let qv = QueueView::from_session_show(&payload);
    let rendered = qv.render_to_string();
    assert!(rendered.contains("queue empty"));
}

/// AC-02: `mark_terminal("cancelled")` updates the
/// status field. The renderer surfaces the new status in
/// the header so the operator can see at a glance that
/// the session has been cancelled (AC-06 cancel path).
#[test]
fn queue_view_mark_terminal_updates_status_field() {
    let mut qv = QueueView::from_session_show(&session_show_with_three_milestones());
    qv.mark_terminal("cancelled");
    assert!(qv.render_to_string().contains("status=cancelled"));
}

/// AC-02: production-path regression. The queue view is
/// reachable from the lane state through
/// `app.autopilot.queue_view()`. Single-milestone
/// sessions are gated — the refresher sets the field to
/// `None` when the queue has only one row, so the
/// renderer skips the block.
#[test]
fn queue_view_is_reachable_from_the_lane_state() {
    use raul::tui::autopilot::AutopilotLaneState;
    let mut state = AutopilotLaneState::empty();
    assert!(state.queue_view().is_none());

    // Multi-milestone: lane state has the view.
    let qv = QueueView::from_session_show(&session_show_with_three_milestones());
    state.queue_view = Some(qv);
    let rendered = state.queue_view().unwrap().render_to_string();
    assert!(rendered.starts_with("Multi-milestone queue (alpha)"));

    // Single-milestone: refresher would set the field to None.
    let single = serde_json::json!({
        "session_id": "delta",
        "session": {
            "id": "delta",
            "status": "active",
            "queue": [{"milestone_id": "M207", "title": "Pilot S2", "lifecycle": "approved"}],
        },
    });
    let qv_single = QueueView::from_session_show(&single);
    state.queue_view = if qv_single.rows.len() > 1 {
        Some(qv_single)
    } else {
        None
    };
    assert!(
        state.queue_view().is_none(),
        "single-milestone sessions must skip the queue block"
    );
}

/// AC-02: the `QueueRow` payload round-trips through
/// serde. The wire format carries `milestone_id` /
/// `title` / `lifecycle` / `active`; the round-trip
/// pins the contract so a future field addition is
/// visible here.
#[test]
fn queue_row_round_trips_through_serde() {
    use raul::tui::autopilot::QueueRow;
    let row = QueueRow {
        milestone_id: "209".to_string(),
        title: "Coordination".to_string(),
        lifecycle: "in-progress".to_string(),
        active: true,
    };
    let v = serde_json::to_value(&row).unwrap();
    let back: QueueRow = serde_json::from_value(v).unwrap();
    assert_eq!(back, row);
}