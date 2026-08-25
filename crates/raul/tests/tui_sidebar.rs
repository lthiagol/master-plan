use raul::tui::app::{App, ContentState, Lane};

#[test]
fn new_app_starts_in_status_lane_with_list_content() {
    let app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    assert_eq!(app.content, ContentState::List);
    assert!(!app.quitting);
}

#[test]
fn select_lane_switches_active_lane_and_resets_to_list() {
    let mut app = App::new();

    app.select_lane(Lane::Milestones);
    assert_eq!(app.active_lane, Lane::Milestones);
    assert_eq!(app.content, ContentState::List);
    assert_eq!(app.selected_index, 0);
}

#[test]
fn all_lanes_are_selectable() {
    let lanes = [
        Lane::Overview,
        Lane::Milestones,
        Lane::Path,
        Lane::Backlog,
        Lane::Settings,
    ];

    let mut app = App::new();
    for lane in &lanes {
        app.select_lane(*lane);
        assert_eq!(app.active_lane, *lane);
        assert_eq!(app.content, ContentState::List);
    }
}

#[test]
fn select_lane_clears_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.content = ContentState::MilestoneDetail;

    app.select_lane(Lane::Backlog);
    assert_eq!(app.active_lane, Lane::Backlog);
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn back_from_non_status_list_returns_to_status() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    assert_eq!(app.active_lane, Lane::Milestones);

    app.go_back();
    assert_eq!(app.active_lane, Lane::Overview);
    assert!(!app.quitting);
}

#[test]
fn back_from_status_list_stays() {
    let mut app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    app.go_back();
    assert!(!app.quitting);
    assert_eq!(app.active_lane, Lane::Overview);
}

#[test]
fn enter_and_exit_milestone_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".to_string(),
        title: "Test".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);

    app.enter_milestone_detail(Some(0));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.selected_milestone_id, Some("01".to_string()));

    app.go_back();
    assert_eq!(app.content, ContentState::List);
    assert_eq!(app.active_lane, Lane::Milestones);
}

#[test]
fn drill_to_annotation_thread_and_back() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".to_string(),
        title: "Test".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.open_thread();
    assert_eq!(app.content, ContentState::AnnotationThread);

    app.go_back();
    assert_eq!(app.content, ContentState::MilestoneDetail);

    app.go_back();
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn review_menu_does_not_change_content_state() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".to_string(),
        title: "Test".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.open_review_menu();

    assert!(matches!(
        app.active_mode,
        raul::tui::mode::Mode::ReviewMenu(_)
    ));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    if let raul::tui::mode::Mode::ReviewMenu(menu) = &app.active_mode {
        // M172 S6: the menu grew a "Set dependency" item — 5 total.
        assert_eq!(menu.items.len(), 5);
    }

    app.close_review_menu();
    assert!(!matches!(
        app.active_mode,
        raul::tui::mode::Mode::ReviewMenu(_)
    ));
    assert_eq!(app.content, ContentState::MilestoneDetail);
}

#[test]
fn co_approval_enters_from_annotation_thread() {
    let ann = raul::tui::app::AnnotationInfo {
        id: "AN-01".into(),
        target: "01".into(),
        kind: "approval-request".into(),
        status: "open".into(),
        author: "alice".into(),
        body: "Please approve".into(),
        created_at: "".into(),
        resolved_at: "".into(),
    };
    let mut app = App::new();
    app.enter_co_approval(ann, "01".to_string());
    assert_eq!(app.content, ContentState::CoApproval);

    app.go_back();
    assert_eq!(app.content, ContentState::AnnotationThread);

    app.go_back();
    assert_eq!(app.content, ContentState::MilestoneDetail);
}

#[cfg(test)]
mod render_tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane};
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
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn sidebar_renders_all_lane_labels() {
        let labels: Vec<_> = Lane::ordered().iter().map(|l| l.label()).collect();
        assert!(labels.contains(&"Overview"));
        assert!(labels.contains(&"Milestones"));
        assert!(
            !labels.contains(&"Inbox"),
            "Inbox lane removed — inbox lives on Overview"
        );

        let app = App::new();
        let output = render_to_string(&app);
        assert!(output.contains("Overview"), "sidebar should show Overview");
        assert!(
            output.contains("Milestones"),
            "sidebar should show Milestones"
        );
    }

    #[test]
    fn sidebar_highlights_active_lane() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        assert_eq!(app.active_lane, Lane::Milestones);

        let output = render_to_string(&app);
        assert!(
            output.contains("Milestones"),
            "sidebar should contain Milestones label"
        );
    }

    #[test]
    fn changing_lane_updates_content_pane() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(vec![raul::tui::app::MilestoneSummary {
            id: "01".to_string(),
            title: "Setup project".to_string(),
            lifecycle: "complete".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        }]);

        let output = render_to_string(&app);
        assert!(
            output.contains("Setup"),
            "content pane should show milestone list"
        );
        assert!(
            output.contains("Milestones"),
            "sidebar should still show Milestones"
        );
    }

    #[test]
    fn overview_lane_shows_plan_rollup_in_content() {
        let app = App::new();
        let output = render_to_string(&app);
        assert!(
            output.contains("Overview"),
            "sidebar should show Overview lane"
        );
        // M181: the redesigned dashboard replaces the single
        // "Plan overview" block with separate Health / Statistics /
        // Work queues / Lifecycle / Suggested path / Inbox /
        // Recent activity panels (AC-01).
        assert!(
            output.contains("Health"),
            "content should show Health strip"
        );
        assert!(
            output.contains("Statistics"),
            "content should show Statistics box"
        );
        assert!(
            output.contains("Work queues"),
            "content should show Work queues box"
        );
        assert!(
            output.contains("Lifecycle"),
            "content should show Lifecycle grid"
        );
        assert!(
            output.contains("Suggested path"),
            "content should show path section"
        );
        assert!(
            output.contains("Inbox"),
            "content should show Inbox section"
        );
        assert!(
            output.contains("Recent activity"),
            "content should show Recent activity section"
        );
    }

    #[test]
    fn path_lane_shows_path_data() {
        let mut app = App::new();
        let path_data = serde_json::json!({
            "strategy": "sequential",
            "lanes": [
                {
                    "name": "execution",
                    "item_type": "milestone",
                    "item_count": 2,
                    "items": [
                        {
                            "rank": 1,
                            "type": "milestone",
                            "milestone": {
                                "id": "77",
                                "title": "Test milestone",
                                "lifecycle": "in-progress",
                                "priority": "high",
                                "depends_on": []
                            }
                        },
                        {
                            "rank": 2,
                            "type": "milestone",
                            "milestone": {
                                "id": "78",
                                "title": "Next milestone",
                                "lifecycle": "planned",
                                "priority": "medium",
                                "depends_on": ["77"]
                            }
                        }
                    ]
                }
            ],
            "summary": {}
        });
        app.load_path_data(path_data);
        app.select_lane(Lane::Path);

        let output = render_to_string(&app);
        assert!(output.contains("M77"), "path should show milestone id");
        assert!(
            output.contains("Test milestone"),
            "path should show milestone title"
        );
        assert!(
            output.contains("EXECUTION"),
            "path should show EXECUTION trunk header"
        );
    }

    #[test]
    fn overview_lane_shows_dashboard_inbox_items() {
        let mut app = App::new();
        app.load_dashboard(raul::tui::app::DashboardSnapshot {
            planning_status: "in-execution".into(),
            execution_mode: "autonomous".into(),
            inbox_count: 1,
            pending_review_count: 0,
            track_pending: 0,
            annotations_open: 0,
            next_action: "".into(),
            path_preview: vec![],
            inbox_items: vec![raul::tui::app::InboxLine {
                id: "TW-03".into(),
                kind: "track".into(),
                display: "Fix backlog output".into(),
                reason: "pending tweak".into(),
                action: "mp track show tweak".into(),
            }],
            ..Default::default()
        });
        // M181: also seed the typed snapshot from the legacy dashboard
        // so the renderer (which now reads `app.overview`) sees the
        // same data the legacy snapshot pinned. Mirrors what
        // `load_overview_snapshot` does internally.
        app.overview.inbox = vec![raul::overview_snapshot::InboxItem {
            id: "TW-03".into(),
            kind: "track".into(),
            display: "Fix backlog output".into(),
            reason: "pending tweak".into(),
            action: "mp track show tweak".into(),
        }];
        app.overview.health.execution_mode = "autonomous".into();
        app.overview.health.planning_state = "in-execution".into();

        let output = render_to_string(&app);
        assert!(output.contains("TW-03"), "overview should show item id");
        assert!(
            output.contains("Fix backlog output"),
            "overview should show item text"
        );
        assert!(
            output.contains("1 inbox item"),
            "overview should show item count"
        );
    }

    #[test]
    fn tab_bar_focused_indicator_renders() {
        // M91 S9: removed. Was asserting that the rendered output contained
        // "Lanes" (the legacy sidebar block title). The tab bar that replaced
        // the sidebar has no such title — the new contract is pinned by
        // tui_tab_bar.rs::tab_bar_is_in_row_one_below_header and related
        // tests.
        let _app = App::new();
    }
}

mod navigation_tests {
    use raul::tui::app::{App, Lane};

    #[test]
    fn tab_bar_starts_focused() {
        // M167: tab_bar_focused removed; this test name is preserved for
        // trace. The replacement contract is "tab bar is always visual
        // chrome" — asserted via the keybind surface and the focus-state
        // renames in m167_no_focus_toggle_remnants.rs.
        let _app = App::new();
    }

    #[test]
    fn tab_move_up_changes_lane() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.tab_move_up();
        assert_eq!(app.active_lane, Lane::Overview);
    }

    #[test]
    fn tab_move_down_changes_lane() {
        let mut app = App::new();
        app.tab_move_down();
        assert_eq!(app.active_lane, Lane::Milestones);
    }

    #[test]
    // M169-rev (LOW fix): renamed from `tab_move_up_at_top_stays`. Tab /
    // Shift+Tab now wrap (AC-01), so Shift+Tab from Overview lands on
    // Settings instead of staying put.
    fn tab_move_up_at_top_wraps_to_settings() {
        let mut app = App::new();
        assert_eq!(app.active_lane, Lane::Overview);
        app.tab_move_up();
        assert_eq!(
            app.active_lane,
            Lane::Settings,
            "Shift+Tab from Overview must wrap to Settings (AC-01)"
        );
    }

    #[test]
    // M169-rev (LOW fix): renamed from `tab_move_down_at_bottom_stays`.
    // Tab on Settings now wraps to Overview instead of clamping.
    fn tab_move_down_at_bottom_wraps_to_overview() {
        let mut app = App::new();
        app.select_lane(Lane::Settings);
        app.tab_move_down();
        assert_eq!(
            app.active_lane,
            Lane::Overview,
            "Tab on Settings must wrap to Overview (AC-01)"
        );
    }

    #[test]
    fn toggle_tab_bar_focus() {
        // M167: toggle_tab_bar_focus removed; this test name is preserved
        // for trace. The replacement contract is "Tab advances
        // active_lane" — covered by m167_keybinds::tab_advances_active_lane.
        let _app = App::new();
    }

    #[test]
    fn sidebar_select_lane_resets_focus_if_was_focused() {
        // M167: focus toggle removed; lane selection still works.
        let mut app = App::new();
        app.select_lane(Lane::Backlog);
        assert_eq!(app.active_lane, Lane::Backlog);
    }

    #[test]
    fn ac01_sidebar_up_down_loads_data() {
        // M91 S9: removed. Was asserting that handle_tab_bar_key contained
        // KeyCode::Up / KeyCode::Down handlers that called tab_move_up /
        // tab_move_down + load_data_for_lane. After S3/S4 the tab bar
        // uses Left/Right/h/l for lane nav; Up/Down are no longer tab-bar
        // binds. The new contract (Tab-bar Previous/Next routes through
        // App::tab_move_up/down + load_data_for_lane) is pinned by
        // tui_tab_bar.rs::s3_left_right_and_h_l_route_to_previous_next.
    }

    // Removed in M91 S4 follow-up: ac02_tab_works_in_overview_lane
    // asserted that OverviewKeyAction::ToggleSidebar existed and that
    // map_overview_key(Tab) mapped to it. After collapsing Tab dispatch
    // to a single top-level handler in runner.rs, that variant is gone
    // — Tab is now a global key, not an Overview-lane binding. The new
    // contract is covered by tui_tab_bar.rs::s4_* tests.
}

mod mouse_tests {
    // M91 S2 deleted sidebar_width and dragging_gutter from App. The three
    // tests that asserted them are removed. S5 will add tab-bar click
    // selection tests here (click on a lane label selects that lane).

    use raul::tui::app::App;

    #[test]
    fn app_new_is_constructible_post_s2() {
        let _app = App::new();
    }
}

mod drill_in_tests {
    use raul::tui::app::{App, ContentState, Lane};

    fn sample_backlog() -> Vec<raul::tui::app::BacklogLine> {
        vec![raul::tui::app::BacklogLine {
            id: "BL-01".into(),
            title: "Add feature X".into(),
            priority: "high".into(),
            status: "open".into(),
            resolution: String::new(),
        }]
    }

    #[test]
    fn drill_into_backlog_detail() {
        let mut app = App::new();
        app.select_lane(Lane::Backlog);
        app.load_backlog(sample_backlog());
        if let Some(b) = app.backlog.first() {
            app.selected_backlog_id = Some(b.id.clone());
            app.detail_scroll = 0;
            app.content = ContentState::BacklogDetail;
        }
        assert_eq!(app.content, ContentState::BacklogDetail);
        assert_eq!(app.selected_backlog_id, Some("BL-01".to_string()));
    }

    #[test]
    fn back_from_backlog_detail_returns_to_list() {
        let mut app = App::new();
        app.select_lane(Lane::Backlog);
        app.content = ContentState::BacklogDetail;
        app.go_back();
        assert_eq!(app.content, ContentState::List);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn detail_scroll_defaults_zero() {
        let app = App::new();
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn detail_scroll_moves_with_up_down_in_detail() {
        let mut app = App::new();
        app.content = ContentState::MilestoneDetail;
        assert_eq!(app.detail_scroll, 0);
        // render reports the detail content is scrollable
        app.detail_max_scroll.set(5);
        app.move_down();
        assert_eq!(app.detail_scroll, 1);
        app.move_up();
        assert_eq!(app.detail_scroll, 0);
    }
}
