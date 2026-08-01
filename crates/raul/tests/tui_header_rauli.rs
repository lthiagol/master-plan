//! M137-2 + M167: the header reads
//! `Review, Approve, Understand Layers - R.A.U.L. - <view title>`
//! — the project acronym expanded as a one-line mnemonic (the
//! R.A.U.L. acronym's L stands for "Layers"; pre-M167 it was
//! "Lanes"). Pre-M137-2 it was `raul TUI — <view title>`.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

#[test]
fn header_reads_review_approve_understand_lanes() {
    let app = App::new();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut header = String::new();
    for x in 0..buffer.area().width {
        header.push_str(buffer[(x, 0)].symbol());
    }
    assert!(
        header.contains("R.A.U.L."),
        "header must carry the R.A.U.L. acronym; got {header:?}"
    );
    assert!(
        header.contains("Review, Approve, Understand Layers"),
        "header must carry the spelled-out acronym; got {header:?}"
    );
}

#[test]
fn header_includes_active_lane_name() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut header = String::new();
    for x in 0..buffer.area().width {
        header.push_str(buffer[(x, 0)].symbol());
    }
    assert!(
        header.contains("Milestones"),
        "header must include the active lane label; got {header:?}"
    );
    assert!(
        header.contains("R.A.U.L."),
        "header must carry the R.A.U.L. acronym; got {header:?}"
    );
}
