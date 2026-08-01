//! M137-2 regression: the Overview inbox shows a visible
//! scrollbar thumb when the inbox overflows. Pre-fix the
//! `compute_overview_list_rects` function bailed out at the
//! heading-row-reserve step (`y = y.saturating_add(1); if y >=
//! block_bottom { return; }`) BEFORE pushing the scrollbar hit
//! area, so a long inbox that filled the visible block never got
//! the scrollbar pushed onto `view.scrollbar_rects`. Two things
//! broke: (1) the heading reserve ran even when the previous
//! iteration's last item had already pushed `y` to `block_bottom`,
//! and (2) the scrollbar push was at the END of the function so
//! any early return skipped it. This test exercises the full
//! render path (compute_view → render) on a 10-item inbox in an
//! 80×24 frame and asserts both the scrollbar hit area is pushed
//! AND the thumb glyph is visible in the rendered buffer.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, DashboardSnapshot, InboxLine, LifecycleCounts};
use raul::tui::render;
use raul::tui::view_state;
use raul::tui::view_state::ScrollableId;

fn make_inbox() -> Vec<InboxLine> {
    vec![
        InboxLine {
            id: "1".into(),
            kind: "milestone".into(),
            display: "M01".into(),
            reason: "r1".into(),
            action: "a1".into(),
        },
        InboxLine {
            id: "2".into(),
            kind: "milestone".into(),
            display: "M02".into(),
            reason: "r2".into(),
            action: "a2".into(),
        },
        InboxLine {
            id: "3".into(),
            kind: "track".into(),
            display: "T03".into(),
            reason: "r3".into(),
            action: "a3".into(),
        },
        InboxLine {
            id: "4".into(),
            kind: "track".into(),
            display: "T04".into(),
            reason: "r4".into(),
            action: "a4".into(),
        },
        InboxLine {
            id: "5".into(),
            kind: "backlog".into(),
            display: "B05".into(),
            reason: "r5".into(),
            action: "a5".into(),
        },
        InboxLine {
            id: "6".into(),
            kind: "spec-review".into(),
            display: "M88".into(),
            reason: "r6".into(),
            action: "a6".into(),
        },
        InboxLine {
            id: "7".into(),
            kind: "milestone".into(),
            display: "M07".into(),
            reason: "r7".into(),
            action: "a7".into(),
        },
        InboxLine {
            id: "8".into(),
            kind: "execution-review".into(),
            display: "M90".into(),
            reason: "r8".into(),
            action: "a8".into(),
        },
        InboxLine {
            id: "9".into(),
            kind: "milestone".into(),
            display: "M09".into(),
            reason: "r9".into(),
            action: "a9".into(),
        },
        InboxLine {
            id: "10".into(),
            kind: "milestone".into(),
            display: "M10".into(),
            reason: "r10".into(),
            action: "a10".into(),
        },
    ]
}

fn render_overview() -> ratatui::buffer::Buffer {
    let mut app = App::new();
    let snap = DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 10,
        pending_review_count: 5,
        track_pending: 0,
        annotations_open: 0,
        next_action: "M01".into(),
        path_preview: vec![],
        execution_counts: Default::default(),
        spec_counts: Default::default(),
        lifecycle_counts: LifecycleCounts::default(),
        blockers: vec![],
        inbox_items: make_inbox(),
    };
    app.load_dashboard(snap);
    // M181: render at 50 rows so the redesigned dashboard's
    // Health / Statistics / Lifecycle / Path blocks leave enough
    // vertical room for the inbox to publish its scrollbar hit area
    // (pre-M181 24 rows no longer accommodates the new top blocks).
    let backend = TestBackend::new(80, 50);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn overview_inbox_scrollbar_pushed_to_view() {
    let mut app = App::new();
    let snap = DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 10,
        pending_review_count: 0,
        track_pending: 0,
        annotations_open: 0,
        next_action: "M01".into(),
        path_preview: vec![],
        execution_counts: Default::default(),
        spec_counts: Default::default(),
        lifecycle_counts: Default::default(),
        blockers: vec![],
        inbox_items: make_inbox(),
    };
    app.load_dashboard(snap);
    // M181: the redesigned dashboard reserves 6/10/6/7 rows above
    // the inbox for Health / Statistics / Lifecycle / Path. 24 rows
    // can't fit them all, so we render at 50 — enough for the inbox
    // to land with enough height to publish its scrollbar hit area.
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 80, 50));
    let inbox_hit = view
        .scrollbar_rects
        .iter()
        .find(|hit| matches!(hit.id, ScrollableId::OverviewInbox))
        .expect("OverviewInbox scrollbar hit area must be pushed even when the inbox overflows past the block (M137-2 regression pin)");
    assert!(
        inbox_hit.total > inbox_hit.scroll,
        "total must exceed the offset for the thumb to be visible: total={} scroll={}",
        inbox_hit.total,
        inbox_hit.scroll
    );
}

#[test]
fn overview_inbox_scrollbar_thumb_painted() {
    let buf = render_overview();
    // M181: the redesigned dashboard splits the lower panel into
    // Inbox (left) + Activity (right) when wide, so the inbox
    // scrollbar gutter sits at the inbox's right edge — not the
    // rightmost column. We look up the gutter position from the
    // pre-computed scrollbar hit area instead of pinning a column.
    let mut app_for_view = App::new();
    let snap_for_view = DashboardSnapshot {
        planning_status: "in-execution".into(),
        execution_mode: "autonomous".into(),
        inbox_count: 10,
        pending_review_count: 5,
        track_pending: 0,
        annotations_open: 0,
        next_action: "M01".into(),
        path_preview: vec![],
        execution_counts: Default::default(),
        spec_counts: Default::default(),
        lifecycle_counts: LifecycleCounts::default(),
        blockers: vec![],
        inbox_items: make_inbox(),
    };
    app_for_view.load_dashboard(snap_for_view);
    let view = view_state::compute_view(&app_for_view, ratatui::layout::Rect::new(0, 0, 80, 50));
    let inbox_hit = view
        .scrollbar_rects
        .iter()
        .find(|hit| matches!(hit.id, ScrollableId::OverviewInbox))
        .expect("OverviewInbox scrollbar hit area must be present");
    let rail_x = inbox_hit.rect.x;
    eprintln!("Rail x = {} (inbox rect = {:?}", rail_x, inbox_hit.rect);
    let mut saw_thumb = false;
    for y in inbox_hit.rect.y..inbox_hit.rect.y + inbox_hit.rect.height {
        if buf[(rail_x, y)].symbol() == "█" {
            saw_thumb = true;
            break;
        }
    }
    assert!(
        saw_thumb,
        "Overview inbox scrollbar thumb must render at least one `█` cell at the rail x; the rail is being drawn but the thumb is missing"
    );
}
