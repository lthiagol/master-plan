//! M185 AC-05: title bar chip for Milestones filter.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn render_header(app: &App) -> String {
    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut row = String::new();
    for x in 0..buf.area().width {
        row.push_str(buf[(x, 0)].symbol());
    }
    row
}

#[test]
fn chip_all_and_filtered() {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "01".into(),
            title: "a".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "02".into(),
            title: "b".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
        flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);

    let header = render_header(&app);
    assert!(
        header.contains("All (2)") || header.contains("All (2"),
        "empty filter chip; got {header:?}"
    );

    app.milestone_filter.insert("approved".into());
    let header = render_header(&app);
    assert!(
        header.contains("approved") && header.contains("(1)"),
        "filtered chip; got {header:?}"
    );
}
