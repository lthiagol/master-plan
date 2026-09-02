//! M185 AC-02: Milestones Table uses REVERSED highlight on selected row.

use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn ms(id: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: format!("title-{id}"),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }
}

#[test]
fn selected_row_has_reversed_modifier() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01"), ms("02"), ms("03")]);
    app.select_lane(Lane::Milestones);
    app.selected_index = 2;

    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Find a cell containing M03 (id at selected index 2) with REVERSED.
    let mut found = false;
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            let cell = &buf[(x, y)];
            if cell.symbol().contains('3') || cell.symbol() == "3" {
                // scan nearby cells on the row for REVERSED
            }
            if cell.modifier.contains(Modifier::REVERSED) {
                // Reconstruct row text
                let mut row = String::new();
                for xx in 0..buf.area().width {
                    row.push_str(buf[(xx, y)].symbol());
                }
                if row.contains("M03") || row.contains("03") {
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
    }
    assert!(
        found,
        "expected REVERSED style on the selected milestone row (index 2 / M03)"
    );
}
