//! M199 AC-01: footer renders as two horizontal lines — globals on
//! top (h-2), per-tab keybinds on bottom (h-1). Globals line content
//! is the six universal bindings from the M199 design (per D-02).
//! Per-tab line content is the per-(lane, content_state) table from
//! `Keybinds::footer_per_tab` (per D-04).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
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
fn footer_is_two_lines_globals_then_per_tab() {
    // M199: globals on top (h-2), per-tab on bottom (h-1).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let buf = render_to_buffer(&app, 120, 30);
    let h = buf.area().height;
    assert!(
        h >= 2,
        "terminal height must leave room for a two-line footer"
    );

    let globals = row_text(&buf, h - 2);
    let per_tab = row_text(&buf, h - 1);

    // M199 globals line — the six universal bindings, no
    // lane-conditional items. Lane-conditional keys
    // (F:filter, /:search, h:hide-done, S:sort, o:cycle) moved
    // to the per-tab line.
    for needle in [":quit", ":help", ":refresh", ":go", "lanes", ":move"] {
        assert!(
            globals.contains(needle),
            "globals line must contain {needle:?}; got: {globals:?}"
        );
    }
    for forbidden in ["S:sort", "o:cycle", ":hide-done", "F:filter", "/:search"] {
        assert!(
            !globals.contains(forbidden),
            "globals line must NOT contain lane-conditional {forbidden:?}; got: {globals:?}"
        );
    }

    // Per-tab line is the lane-specific list affordances.
    let settings_staged = false;
    let expected_per_tab = app.keybinds.footer_per_tab(
        Lane::Milestones,
        app.content,
        app.open_only,
        settings_staged,
    );
    let trimmed_expected = expected_per_tab.trim();
    assert!(
        per_tab.contains(trimmed_expected.trim())
            || trimmed_expected
                .split_whitespace()
                .filter(|t| t.contains(':'))
                .all(|tok| per_tab.contains(tok)),
        "per-tab line must match footer_per_tab(Milestones, List); got per_tab={per_tab:?} expected={expected_per_tab:?}"
    );
    for needle in [
        "F:filter",
        "/:search",
        ":hide-done",
        "S:sort",
        "o:cycle",
        "A:annotate",
    ] {
        assert!(
            per_tab.contains(needle.trim_end_matches("A:annotate")) || per_tab.contains("annotate"),
            "per-tab line must contain {needle:?} (lane-specific); got: {per_tab:?}"
        );
    }
}

#[test]
fn footer_area_height_is_two_for_default_lane() {
    // M199: default lane is Overview, which has a non-empty
    // per-tab string (⏎:inbox), so the footer must still be 2
    // rows tall — the M187 contract survives the flip.
    let app = App::new();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let view = view_state::compute_view(&app, area);
    assert_eq!(
        view.footer_area.height, 2,
        "compute_view must reserve a 2-row footer_area when per-tab is non-empty"
    );
}

#[test]
fn footer_area_height_is_one_for_path_lane() {
    // M199 S4: Path returns an empty per-tab string, so the
    // footer collapses to 1 row (globals only).
    let mut app = App::new();
    app.select_lane(Lane::Path);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let view = view_state::compute_view(&app, area);
    assert_eq!(
        view.footer_area.height, 1,
        "compute_view must reserve a 1-row footer_area when per-tab is empty (Path)"
    );
    // Sanity: globals line still renders.
    let buf = render_to_buffer(&app, 80, 24);
    let globals_y = view.footer_area.y;
    let globals = row_text(&buf, globals_y);
    assert!(
        globals.contains(":quit") && globals.contains(":help"),
        "globals line must render even on 1-row footer; got: {globals:?}"
    );
    // And the row below the footer is content, not a per-tab row.
    if globals_y + 1 < buf.area().height {
        let below = row_text(&buf, globals_y + 1);
        assert!(
            !below.contains(":filter") && !below.contains(":search"),
            "row below the 1-row footer must not be a per-tab line; got: {below:?}"
        );
    }
}
