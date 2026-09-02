//! Welcome / plan overview lane (M89).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::mp_runner::MpRunner;
use raul::tui::app::{
    App, BlockerLine, ContentState, DashboardSnapshot, ExecutionCounts, InboxLine, Lane,
    LifecycleCounts, SpecCounts,
};
use raul::tui::dashboard;
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn render_to_string(app: &App) -> String {
    // M183: footer is two rows (was one); keep enough height for the
    // full overview hierarchy + both inbox group headers.
    let backend = TestBackend::new(120, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

fn sample_dashboard() -> DashboardSnapshot {
    DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 2,
        pending_review_count: 5,
        track_pending: 1,
        annotations_open: 3,
        next_action: "M86/S1".into(),
        path_preview: vec!["M86/S1".into(), "M86/S2".into()],
        execution_counts: ExecutionCounts {
            total: 88,
            done: 82,
            planned: 6,
            in_progress: 0,
            blocked: 0,
        },
        spec_counts: SpecCounts {
            ready: 3,
            review: 3,
            verified: 82,
        },
        lifecycle_counts: LifecycleCounts {
            total: 88,
            draft: 0,
            groomed: 0,
            approved: 3,
            in_progress: 0,
            done: 0,
            self_reviewed: 0,
            reviewed: 0,
            complete: 82,
            remediation: 0,
        },
        blockers: vec![BlockerLine {
            milestone: "87".into(),
            reason: "blocked on spec".into(),
        }],
        inbox_items: vec![
            InboxLine {
                id: "88".into(),
                kind: "milestone".into(),
                display: "M88 — Grooming needed".into(),
                reason: "needs grooming".into(),
                action: "mp milestone groom 88".into(),
            },
            InboxLine {
                id: "TW-03".into(),
                kind: "track".into(),
                display: "Fix output".into(),
                reason: "pending tweak".into(),
                action: "mp track show tweak".into(),
            },
        ],
    }
}

#[test]
fn welcome_overview_default_lane() {
    let app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn snapshot_rollup_fields_from_fixture_json() {
    // Fixture mirrors the canonical `mp status --format json` shape
    // observed on the repo's own plan (BF-15 / M171 AC-06):
    //   - `by_execution_status` is the legacy exec-status rollup
    //   - `by_spec_status` is the legacy spec-status rollup
    //   - `by_lifecycle` is the post-M100 canonical rollup the
    //     Dashboard's Plan-overview block actually reads.
    // The shape MUST match the real `mp status` output so the
    // dashboard mock tests stay in lock-step with the live schema.
    let status = serde_json::json!({
        "planning_status": "in-execution",
        "inbox_count": 2,
        "pending_review_count": 5,
        "track_pending": 1,
        "annotations_open": 3,
        "execution": { "mode": "autonomous" },
        "milestones": {
            "total": 88,
            "by_execution_status": { "done": 82, "planned": 6, "in-progress": 0, "blocked": 0 },
            "by_spec_status": { "ready": 3, "review": 3, "verified": 82, "implemented": 0 },
            "by_lifecycle": {
                "draft": 0,
                "groomed": 0,
                "approved": 3,
                "in-progress": 0,
                "executed": 0,
                "self-reviewed": 0,
                "reviewed": 0,
                "complete": 82,
                "remediation": 0
            }
        },
        "blockers": [],
        "suggested_path": {
            "next_action": { "display": "M86/S1" },
            "preview": ["M86/S1", "M86/S2"]
        }
    });
    let inbox = serde_json::json!({
        "items": [
            {
                "kind": "milestone",
                "id": "88",
                "display": "M88 — Example",
                "reason": "needs grooming",
                "action": "mp milestone groom 88"
            }
        ]
    });

    let snap = dashboard::snapshot_from_status_inbox(&status, &inbox);
    assert_eq!(snap.execution_mode, "autonomous");
    assert_eq!(snap.execution_counts.done, 82);
    assert_eq!(snap.execution_counts.planned, 6);
    assert_eq!(snap.spec_counts.ready, 3);
    assert_eq!(snap.pending_review_count, 5);
    assert_eq!(snap.next_action, "M86/S1");
    // BF-15 / M171 AC-06: the fixture must populate the same
    // `by_lifecycle` block `mp status` emits; the Dashboard reads it.
    assert_eq!(snap.lifecycle_counts.complete, 82);
    assert_eq!(snap.lifecycle_counts.approved, 3);
    assert_eq!(snap.lifecycle_counts.total, 88);
}

/// BF-15 / M171 AC-06 regression pin: the keys `mp status
/// --format json.milestones` exposes in the repo's own plan match the
/// keys the dashboard mock fixtures cover. A future drift in either
/// direction (real shape grows or fixture regresses) trips this test
/// immediately instead of producing a stale-mock surprise downstream.
///
/// Implementation note (M171 external-review F-01): the prior version
/// compared two hardcoded `serde_json::json!{…}` literals — passing
/// was a tautology. This version actually shells out to `mp status`
/// against the repo's own plan via [`MpRunner`], captures the live
/// `milestones` block, and asserts every key the live shape exposes
/// is also present in the dashboard mock fixture. A drift in either
/// direction (live shape grows → must update fixtures; fixture
/// regresses → live shape still covers it) trips immediately.
#[test]
fn dashboard_mock_keys_cover_real_mp_status_shape() {
    // MpRunner resolves the `mp` binary the same way `tui_overview_watch_toggle`
    // and the other integration tests do (sibling next to raul, MP_HOME/bin/mp,
    // PATH). We point it at the workspace's own master-plan so the captured
    // shape matches what a developer running `mp status` in this checkout sees.
    let mut runner = MpRunner::new().expect("mp binary discoverable for parity test");
    runner.set_project_root(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_str()
            .expect("workspace root utf-8"),
    );
    let real: serde_json::Value = runner
        .run("status", &["--format", "json"])
        .expect("mp status --format json parseable as JSON");
    let real_milestones = real
        .get("milestones")
        .and_then(|m| m.as_object())
        .expect("mp status payload must contain a `milestones` object");

    // The canonical fixture the dashboard tests pin against. Built to
    // mirror the *keys* produced by `tui::dashboard::tests::fixture_status`
    // — values may legitimately drift between fixtures and the live plan.
    let fixture = sample_dashboard_fixture_milestones_keys();
    let fixture_obj = fixture.as_object().expect("fixture shape is a JSON object");

    for key in real_milestones.keys() {
        assert!(
            fixture_obj.contains_key(key),
            "dashboard mock fixture missing key `{key}` that live `mp status.milestones` exposes. \
             Update `tui::dashboard::tests::fixture_status` (and any sibling dashboard mock \
             JSON in `welcome_overview.rs::snapshot_rollup_fields_from_fixture_json`) to \
             include this key so the dashboard mock stays in parity with the live shape."
        );
    }
    // The canonical lifecycle buckets the Plan-overview reads must all
    // be present in both sides — `count_from_map` falls back to 0 when
    // a key is missing, which silently hides shape drift downstream.
    let real_lifecycle = real_milestones
        .get("by_lifecycle")
        .and_then(|v| v.as_object())
        .expect("live mp status shape missing `by_lifecycle` block");
    let fixture_lifecycle = fixture_obj
        .get("by_lifecycle")
        .and_then(|v| v.as_object())
        .expect("dashboard mock fixture missing `by_lifecycle` block");
    // M196: executor end-state renamed lifecycle bucket "done" → "executed".
    for bucket in [
        "draft",
        "groomed",
        "approved",
        "in-progress",
        "executed",
        "self-reviewed",
        "reviewed",
        "complete",
        "remediation",
    ] {
        assert!(
            real_lifecycle.contains_key(bucket),
            "live mp status shape missing lifecycle bucket `{bucket}` \
             (regenerate this test or update mp to emit it)"
        );
        assert!(
            fixture_lifecycle.contains_key(bucket),
            "dashboard mock fixture missing lifecycle bucket `{bucket}` \
             (update fixture_status to include it)"
        );
    }
}

/// Return the *keys* (not the values) of the canonical dashboard mock
/// fixture's `milestones` block. Mirrors the shape produced by
/// `tui::dashboard::tests::fixture_status` (BF-15 / M171 AC-06).
/// Kept side-by-side with the live shell-out in
/// [`dashboard_mock_keys_cover_real_mp_status_shape`] so a reader can
/// audit which keys each side pins without flipping back and forth
/// between the dashboard fixture and this test.
fn sample_dashboard_fixture_milestones_keys() -> serde_json::Value {
    serde_json::json!({
        "by_execution_status": {},
        "by_spec_status": {},
        "by_lifecycle": {
            "draft": 0,
            "groomed": 0,
            "approved": 0,
            "in-progress": 0,
            "executed": 0,
            "self-reviewed": 0,
            "reviewed": 0,
            "complete": 0,
            "remediation": 0
        },
        "total": 0
    })
}

#[test]
fn welcome_render_shows_rollup_and_grouped_inbox() {
    let mut app = App::new();
    app.load_dashboard(sample_dashboard());
    // M181: also seed the typed snapshot from the legacy dashboard
    // so the renderer reads from `app.overview` (the new data path).
    // `legacy_dashboard_from_overview` populates the same numbers,
    // but seeding it explicitly keeps the test pinning the
    // `load_dashboard` translation rather than relying on a side
    // effect of `load_dashboard`.
    app.overview.health.execution_mode = "autonomous".into();
    app.overview.health.planning_state = "in-execution".into();
    app.overview.lifecycle.complete = 82;
    app.overview.lifecycle.approved = 3;
    // M202 S20: the lifecycle grid now rolls up by mp-flow stage.
    // Seed the per-stage counts so the grid shows the values this
    // test pins (complete=82, approved=3 land on stages 7 and 4).
    let mut flow_counts = std::collections::HashMap::new();
    flow_counts.insert("complete".to_string(), 82u64);
    flow_counts.insert("approve".to_string(), 3u64);
    app.overview.mp_flow_stage_counts = Some(flow_counts);
    app.overview.queues.pending_reviews = 5;
    app.overview.totals.milestones = 88;
    app.overview.path = vec![raul::overview_snapshot::PathItem {
        id: "86".into(),
        display: "M86/S1".into(),
        kind: String::new(),
        milestone: None,
        step: None,
    }];
    app.overview.inbox = vec![
        raul::overview_snapshot::InboxItem {
            id: "88".into(),
            kind: "milestone".into(),
            display: "M88 — Grooming needed".into(),
            reason: "needs grooming".into(),
            action: "mp milestone groom 88".into(),
        },
        raul::overview_snapshot::InboxItem {
            id: "TW-03".into(),
            kind: "track".into(),
            display: "Fix output".into(),
            reason: "pending tweak".into(),
            action: "mp track show tweak".into(),
        },
    ];
    let output = render_to_string(&app);

    assert!(output.contains("Overview"), "sidebar shows Overview lane");
    // M181: the redesigned dashboard replaces "Plan overview" with
    // separate Health / Statistics / Work queues / Lifecycle /
    // Suggested path / Inbox / Recent activity panels (AC-01).
    assert!(output.contains("Health"), "Health strip section");
    assert!(output.contains("Statistics"), "Statistics box section");
    assert!(output.contains("Work queues"), "Work queues box section");
    assert!(output.contains("Lifecycle"), "Lifecycle grid section");
    assert!(output.contains("Suggested path"), "Suggested path section");
    assert!(output.contains("Inbox"), "Inbox section");
    assert!(output.contains("Recent activity"), "Activity section");
    assert!(output.contains("autonomous"), "execution mode");
    // M181 + M202 S20: the typed snapshot drives the new lifecycle
    // grid, keyed by the 12 mp-flow stage buckets. complete=82 lands
    // on the `7/12 complete` bucket, approved=3 on `4/12 approve`.
    assert!(
        output.contains("7/12 complete"),
        "lifecycle complete bucket label"
    );
    assert!(
        output.contains("82"),
        "lifecycle complete value (rendered somewhere in the grid)"
    );
    assert!(
        output.contains("4/12 approve"),
        "lifecycle approve bucket label"
    );
    assert!(
        output.contains(" 3 ") || output.contains("3\n") || output.contains("3│"),
        "lifecycle approve value (3) rendered next to its label"
    );
    // Suggested path preview carries the M180 supplied display
    // string (one of the 3..5 items).
    assert!(output.contains("M86/S1"), "next action / path item");
    // Inbox heading + grouping survive the redesign.
    assert!(
        output.contains("── milestone ──"),
        "grouped milestone section"
    );
    assert!(output.contains("── track ──"), "grouped track section");
    assert!(output.contains("mp milestone groom 88"), "action hint");
}

#[test]
fn welcome_render_empty_inbox_state() {
    let mut app = App::new();
    let mut snap = sample_dashboard();
    snap.inbox_count = 0;
    snap.inbox_items.clear();
    app.load_dashboard(snap);

    let output = render_to_string(&app);
    assert!(output.contains("Inbox is empty"), "explicit empty inbox");
}

#[test]
fn overview_lane_inbox_list_count() {
    let mut app = App::new();
    app.load_dashboard(sample_dashboard());
    assert_eq!(app.dashboard.inbox_items.len(), 2);
}

#[test]
fn navigate_from_inbox_milestone_sets_flash_when_missing() {
    use raul::tui::inbox_nav::{apply_inbox_navigation, InboxNavFollowUp};

    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![]);
    app.load_dashboard(DashboardSnapshot {
        inbox_items: vec![InboxLine {
            id: "99".into(),
            kind: "milestone".into(),
            display: "M99".into(),
            reason: "review".into(),
            action: "mp milestone approve 99".into(),
        }],
        ..Default::default()
    });

    let item = app.dashboard.inbox_items[0].clone();
    assert_eq!(
        apply_inbox_navigation(&mut app, &item),
        InboxNavFollowUp::None
    );
    assert_eq!(
        app.flash_message.as_deref(),
        Some("mp milestone approve 99")
    );
}

#[test]
fn navigate_from_inbox_milestone_enters_detail_when_found() {
    use raul::tui::inbox_nav::{apply_inbox_navigation, InboxNavFollowUp};

    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "86".into(),
        title: "Visual".into(),
        lifecycle: "complete".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    let item = InboxLine {
        id: "86".into(),
        kind: "milestone".into(),
        display: "M86".into(),
        reason: "pending review".into(),
        action: "mp reviews pass 86".into(),
    };
    assert_eq!(
        apply_inbox_navigation(&mut app, &item),
        InboxNavFollowUp::LoadMilestoneDetail("86".into())
    );
    assert_eq!(app.content, ContentState::MilestoneDetail);
}
