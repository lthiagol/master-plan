//! M185 AC-02: the Milestones Table highlights the selected row.
//! M244: the highlight moved from a REVERSED modifier over an accent
//! fill to the `surface_2` / `focus_ring` layering roles.

use ratatui::backend::TestBackend;
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
fn selected_row_has_surface_2_highlight_and_focus_ring_marker() {
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
    let palette = app.effective_palette();

    // Locate the visual row that carries the `M03` id, then assert the
    // selection treatment on that row: the `surface_2` background and
    // the `focus_ring` marker glyph.
    let mut selected_row: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol())
            .collect();
        if row.contains("M03") {
            selected_row = Some(y);
            break;
        }
    }
    let y = selected_row.expect("the selected row's M03 id must be rendered");
    let row_has_surface_2 =
        (0..buf.area().width).any(|x| buf[(x, y)].style().bg == Some(palette.surface_2));
    let marker_in_focus_ring = (0..buf.area().width)
        .any(|x| buf[(x, y)].symbol() == "▌" && buf[(x, y)].style().fg == Some(palette.focus_ring));

    assert!(
        row_has_surface_2,
        "expected the selected milestone row (index 2 / M03) to be painted with surface_2"
    );
    assert!(
        marker_in_focus_ring,
        "expected a focus_ring marker glyph on the selected milestone row (index 2 / M03)"
    );
}
