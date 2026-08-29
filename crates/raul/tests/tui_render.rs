use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::App;
use raul::tui::render;
use raul::tui::view_state;

fn render_to_string(app: &App) -> String {
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            {
                let view = view_state::compute_view(app, frame.area());
                render::render(frame, app, &view);
            };
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        // Trim trailing spaces from each line for clean assertions
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

fn sample_milestones() -> Vec<raul::tui::app::MilestoneSummary> {
    vec![
        raul::tui::app::MilestoneSummary {
            id: "01".to_string(),
            title: "Setup project infrastructure".to_string(),
            lifecycle: "complete".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        raul::tui::app::MilestoneSummary {
            id: "02".to_string(),
            title: "Core engine implementation".to_string(),
            lifecycle: "approved".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
        raul::tui::app::MilestoneSummary {
            id: "03".to_string(),
            title: "Polish and documentation".to_string(),
            lifecycle: "draft".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        },
    ]
}

#[test]
fn milestone_list_render() {
    // M172 S2: the Milestones lane renders as a hierarchical tree
    // (not a Table). IDs render without the legacy `M` prefix —
    // the prefix was a UI convention carried over from the Table
    // renderer and the tree view drops it for cleaner tree
    // branches (the title carries the rest of the identifier).
    let mut app = App::new();
    app.select_lane(raul::tui::app::Lane::Milestones);
    app.load_milestones(sample_milestones());
    let output = render_to_string(&app);
    assert!(output.contains("Milestones"), "should contain header");
    assert!(output.contains("01"), "should show first milestone id");
    assert!(output.contains("02"), "should show second milestone id");
    assert!(output.contains("03"), "should show third milestone id");
    assert!(output.contains("Setup"), "should contain milestone title");
    assert!(
        output.contains("└─") || output.contains("├─"),
        "should render tree-branch markers; got:\n{output}"
    );
}

#[test]
fn backlog_list_renders_table_columns() {
    let mut app = App::new();
    app.select_lane(raul::tui::app::Lane::Backlog);
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-04".into(),
        title: "JSON schema validate on CLI write".into(),
        priority: "high".into(),
        status: "open".into(),
        resolution: String::new(),
    }]);
    let output = render_to_string(&app);
    assert!(output.contains("Backlog"), "should contain backlog header");
    assert!(
        output.contains("Priority") && output.contains("Status"),
        "should show table column headers; got:\n{output}"
    );
    assert!(output.contains("BL-04"), "should show backlog id");
    assert!(output.contains("JSON schema"), "should show title");
    assert!(output.contains("high"), "should show priority");
    assert!(output.contains("open"), "should show status");
}

#[test]
fn milestone_detail_render() {
    let mut app = App::new();
    app.load_milestones(sample_milestones());
    app.enter_milestone_detail(Some(0));
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "01",
            "title": "Setup project infrastructure",
            "lifecycle": "complete",
            "spec_status": "verified",
            "execution_status": "done",
            "effort": "S",
            "risk": "low"
        },
        "intent": {
            "outcome": "A working project skeleton"
        },
        "problem": {
            "description": "Need a foundation to build on"
        },
        "scope": {
            "in_scope": ["Cargo setup", "CI config"],
            "out_of_scope": ["Full feature set"]
        },
        "acceptance_criteria": [
            {
                "id": "AC-01",
                "description": "Project compiles",
                "status": "passed"
            },
            {
                "id": "AC-02",
                "description": "CI runs green",
                "status": "passed"
            }
        ],
        "steps": [
            {
                "id": "S1",
                "action": "Initialize Cargo workspace",
                "status": "done"
            },
            {
                "id": "S2",
                "action": "Set up CI pipeline",
                "status": "done"
            }
        ]
    }));
    let output = render_to_string(&app);
    assert!(output.contains("M01"), "should show milestone id");
    assert!(output.contains("Setup"), "should show title");
    // M173 S7: the badge now renders the effective lifecycle
    // (`complete` for verified+done), not the raw spec_status. The
    // parity test in `tui_status_parity.rs` pins the helper-vs-render
    // agreement across canonical and legacy shapes.
    assert!(
        output.contains("complete"),
        "should show effective lifecycle badge"
    );
    // M167: section headers render as `##  ✦  Section  (count)  ──`
    // instead of the pre-M167 bare names.
    assert!(
        output.contains("Acceptance Criteria"),
        "should show ACs section"
    );
    // M167: the "Steps" section header is present but falls below the
    // 40-row test viewport when the milestone carries a rich intent /
    // problem / scope. The richer Steps rendering is covered in
    // `m167_detail_render` with a larger render surface.
}

#[test]
fn annotation_thread_render() {
    let mut app = App::new();
    app.load_milestones(sample_milestones());
    app.enter_milestone_detail(Some(0));
    app.open_thread();
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "01",
            "title": "Setup",
            "spec_status": "verified",
            "execution_status": "done",
            "effort": "S",
            "risk": "low"
        }
    }));
    app.load_annotations(vec![
        raul::tui::app::AnnotationInfo {
            id: "AN-01".to_string(),
            target: "01".to_string(),
            kind: "review-request".to_string(),
            status: "open".to_string(),
            author: "alice".to_string(),
            body: "Please review the CI setup step".to_string(),
            created_at: "2026-01-15".to_string(),
            resolved_at: String::new(),
        },
        raul::tui::app::AnnotationInfo {
            id: "AN-02".to_string(),
            target: "01".to_string(),
            kind: "approval-request".to_string(),
            status: "resolved".to_string(),
            author: "bob".to_string(),
            body: "Approving milestone for execution".to_string(),
            created_at: "2026-01-14".to_string(),
            resolved_at: "2026-01-16".to_string(),
        },
    ]);
    let output = render_to_string(&app);
    assert!(output.contains("Annotations"), "should show header");
    assert!(output.contains("AN-01"), "should show first annotation");
    assert!(output.contains("open"), "should show open status");
    assert!(output.contains("AN-02"), "should show second annotation");
    assert!(output.contains("resolved"), "should show resolved status");
}

#[test]
fn empty_state_render() {
    let mut app = App::new();
    app.select_lane(raul::tui::app::Lane::Milestones);
    let output = render_to_string(&app);
    assert!(
        output.contains("No milestones"),
        "should show empty state message"
    );
    assert!(output.contains("R.A.U.L."), "should still show header");
}

#[test]
fn help_overlay_render() {
    let mut app = App::new();
    app.load_milestones(sample_milestones());
    app.toggle_help();
    let output = render_to_string(&app);
    assert!(
        output.contains("Keyboard Shortcuts"),
        "should show help header"
    );
    assert!(output.contains("Quit"), "should show quit key");
}

#[test]
fn filter_toggle_render() {
    let mut app = App::new();
    app.load_milestones(sample_milestones());
    app.enter_milestone_detail(Some(0));
    app.open_thread();
    app.load_annotations(vec![
        raul::tui::app::AnnotationInfo {
            id: "AN-01".to_string(),
            target: "01".to_string(),
            kind: "review-request".to_string(),
            status: "open".to_string(),
            author: "alice".to_string(),
            body: "Review".to_string(),
            created_at: "".to_string(),
            resolved_at: "".to_string(),
        },
        raul::tui::app::AnnotationInfo {
            id: "AN-02".to_string(),
            target: "01".to_string(),
            kind: "approval-request".to_string(),
            status: "resolved".to_string(),
            author: "bob".to_string(),
            body: "Approved".to_string(),
            created_at: "".to_string(),
            resolved_at: "".to_string(),
        },
    ]);

    // Default: all annotations
    let output = render_to_string(&app);
    assert!(output.contains("AN-01"));
    assert!(output.contains("AN-02"));

    // After toggle: only open
    app.toggle_filter();
    let output = render_to_string(&app);
    assert!(output.contains("AN-01"));
    assert!(output.contains("open only"), "should indicate filter is on");
}

#[test]
fn watch_lane_log_pane_uses_cached_entries_without_render_io() {
    use raul::tui::app::Lane;
    let dir = tempfile::tempdir().expect("tempdir");
    let log_dir = dir.path().join(".mp");
    std::fs::create_dir_all(&log_dir).expect("create .mp dir");
    std::fs::write(
        log_dir.join("watch.log"),
        "{\"event\":\"run-started\",\"ts\":\"2026-07-17T10:00:00Z\"}\n\
         {\"event\":\"stage-changed\",\"stage\":\"execute\",\"ts\":\"2026-07-17T10:01:00Z\"}\n\
         {\"event\":\"step-done\",\"step\":\"S3\",\"ts\":\"2026-07-17T10:02:00Z\"}\n",
    )
    .expect("write watch.log");

    let mut app = App::new();
    app.active_lane = Lane::Watch;
    app.plan_dir = dir.path().to_path_buf();
    app.watch.log_tail = raul::tui::watch::tail_watch_log(&app.plan_dir, 8);
    std::fs::remove_dir_all(&log_dir).expect("poison render-time log path");

    let output = render_to_string(&app);
    assert!(
        output.contains("run-started"),
        "log pane must surface watch.log entries; output was:\n{output}"
    );
    assert!(
        output.contains("stage-changed") || output.contains("step-done"),
        "log pane must surface at least one later entry; output was:\n{output}"
    );
    assert!(
        !output.contains("no log lines yet"),
        "log pane must NOT show the empty placeholder when the log has content; output was:\n{output}"
    );
}

// M179 F-02 negative case: when there is no watch.log, the pane
// falls back to the placeholder. Pins the empty-state contract.
#[test]
fn watch_lane_log_pane_placeholder_when_log_absent() {
    use raul::tui::app::Lane;
    let dir = tempfile::tempdir().expect("tempdir");

    let mut app = App::new();
    app.active_lane = Lane::Watch;
    app.plan_dir = dir.path().to_path_buf();

    let output = render_to_string(&app);
    assert!(
        output.contains("no log lines yet"),
        "empty log pane must show the placeholder; output was:\n{output}"
    );
}
