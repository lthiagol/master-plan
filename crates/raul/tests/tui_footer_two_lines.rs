//! M183 AC-01: footer renders as two horizontal lines — globals on
//! line 1, per-tab keybinds on line 2.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::render;
use raul::tui::view_state;

fn render_to_buffer(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    let mut out = String::new();
    for x in 0..buf.area().width {
        out.push_str(buf[(x, y)].symbol());
    }
    out
}

#[test]
fn footer_is_two_lines_per_tab_then_globals() {
    // M187: layout flip — per-tab on top (h-2), globals on bottom (h-1).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let buf = render_to_buffer(&app, 120, 30);
    let h = buf.area().height;
    assert!(
        h >= 2,
        "terminal height must leave room for a two-line footer"
    );

    let per_tab = row_text(&buf, h - 2);
    let globals = row_text(&buf, h - 1);

    for needle in [
        ":quit",
        ":help",
        ":refresh",
        "Shift+Tab/Tab:lanes",
        "S:sort",
        "o:cycle-sort",
        ":hide-done",
        "F:filter",
        "/:search",
    ] {
        assert!(
            globals.contains(needle),
            "globals line must contain {needle:?}; got: {globals:?}"
        );
    }

    let expected_per_tab = Keybinds::default().footer_list();
    let trimmed_expected = expected_per_tab.trim();
    assert!(
        per_tab.contains(trimmed_expected.trim())
            || trimmed_expected
                .split_whitespace()
                .filter(|t| t.contains(':'))
                .all(|tok| per_tab.contains(tok)),
        "per-tab line must match footer_list(); got per_tab={per_tab:?} expected={expected_per_tab:?}"
    );
}

#[test]
fn footer_area_height_is_two() {
    let app = App::new();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let view = view_state::compute_view(&app, area);
    assert_eq!(
        view.footer_area.height, 2,
        "compute_view must reserve a 2-row footer_area"
    );
}
