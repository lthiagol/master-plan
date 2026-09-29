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
            created: String::new(),
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
            created: String::new(),
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
            created: String::new(),
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

// ─── M230 / AC-04: Autopilot lane lifecycle graph + compact queue ────
//
// M179's render_lifecycle_graph + render_compact_queue were
// migrated from `tui::watch` to `tui::autopilot` by M230; the
// public signature is preserved so the existing test contracts
// stay pinned. The queue rows now always read `[pending]` (the
// M229-removed watch verb was the only source of per-row
// outcomes, and it never landed before M229).
//
// The legacy `Watch` mirror that the pre-M230 tests pinned
// (the picker selection and the M178 status snapshot) is gone —
// those tests were deleted along with `tui::watch` in M230.
// The equivalent contracts on the typed `Picker` already exist
// in crates/raul/src/tui/autopilot.rs's #[cfg(test)] block.

#[test]
fn render_lifecycle_graph_highlights_current_node() {
    use raul::tui::autopilot::render_lifecycle_graph;
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
    use raul::tui::autopilot::render_lifecycle_graph;
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
fn render_compact_queue_surfaces_typed_picker_queue() {
    use raul::tui::autopilot::render_compact_queue;
    let mut app = App::new();
    app.autopilot.refresh_picker(&serde_json::json!([
        {"id": "M01", "title": "First", "lifecycle": "approved", "priority": "high"},
        {"id": "M02", "title": "Second", "lifecycle": "in-progress", "priority": "normal"},
    ]));
    app.autopilot.toggle_picker_select();
    app.autopilot.move_picker_cursor(1);
    app.autopilot.toggle_picker_select();
    let q = render_compact_queue(&app);
    // M230: every row is `[pending]` — the v2 status payload
    // that supplied per-milestone outcomes was removed with
    // the legacy watch verb in M229, so the renderer has no
    // live outcome to surface until the autopilot control
    // surface ships an equivalent.
    assert!(q.contains("[pending] 01"));
    assert!(q.contains("[pending] 02"));
    // The active queue row carries the `>` marker. After two
    // toggles, cursor=1, so the second line is the active row.
    let active_line = q
        .lines()
        .find(|l| l.starts_with(">"))
        .expect("at least one line must carry the active marker");
    assert!(active_line.contains("[pending] 02"));
}

#[test]
fn render_compact_queue_omitted_when_empty() {
    use raul::tui::autopilot::render_compact_queue;
    let app = App::new();
    let q = render_compact_queue(&app);
    assert!(q.contains("empty queue"));
}

// ─── M217 S8 cutover: the single Autopilot poller ───────────────
//
// M179's `watch::Poller` / `poll_watch_state` / `POLL_INTERVAL_MS`
// tests lived here. M217 deleted that scheduler; the equivalents
// now exercise `tui::poll::AutopilotPoller`, whose clock is
// injected so the cadence assertions no longer depend on real
// wall-clock time. M230 deleted the surrounding `tui::watch`
// module entirely; the poller lives on as the lone scheduler.

#[test]
fn poller_fires_on_lane_entry_then_respects_the_interval() {
    use raul::tui::poll::{AutopilotPoller, PollDecision};
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    assert_eq!(p.begin(0), PollDecision::Fire, "lane entry must poll");
    p.finish(0);
    assert_eq!(p.begin(500), PollDecision::NotDue);
    assert_eq!(p.begin(2_000), PollDecision::Fire);
}

#[test]
fn default_poll_cadence_is_in_the_2_to_5_second_window() {
    use raul::tui::poll::AutopilotPoller;
    let secs = AutopilotPoller::new().refresh_secs();
    assert!(
        (2..=5).contains(&secs),
        "default cadence must be in [2, 5]s; got {secs}"
    );
}

// The dirty-signal contract M179 F-08 pinned, carried forward: a
// poll that changes state must call `app.touch()` so `run_loop`
// sets `needs_render`. The runner points at a nonexistent binary,
// so every payload degrades to `Value::Null` — which is still a
// *change* on the first poll (None → Some snapshot), so the
// version must move exactly once and then stay put.
#[test]
fn poll_autopilot_lane_bumps_version_only_when_state_changes() {
    use raul::mp_runner::MpRunner;
    use raul::tui::poll::poll_autopilot_lane;

    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/m217/test");
    let mut app = App::new();
    app.autopilot_poller.set_focused(true);
    let before = app.version();

    poll_autopilot_lane(&runner, &mut app, 0);
    let after_first = app.version();
    assert_ne!(
        after_first, before,
        "the first poll establishes a snapshot and must bump the version"
    );

    // Second poll, one interval later, same (null) payloads → no
    // redraw. This is the AC-05 diffing guarantee.
    poll_autopilot_lane(&runner, &mut app, 10_000);
    assert_eq!(
        app.version(),
        after_first,
        "an unchanged snapshot must not bump the version (AC-05)"
    );
}

// ─── M230: tail_watch_log migrated to tui::autopilot ──────────

#[test]
fn tail_watch_log_returns_empty_when_file_missing() {
    use raul::tui::autopilot::tail_watch_log;
    let dir = tempfile::TempDir::new().unwrap();
    let lines = tail_watch_log(dir.path(), 50);
    assert!(lines.is_empty());
}

#[test]
fn tail_watch_log_returns_last_n_lines() {
    use raul::tui::autopilot::tail_watch_log;
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
