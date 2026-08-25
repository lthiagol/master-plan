//! M199 AC-05: on `Lane::Path`, the per-tab line is empty so the
//! footer is one row tall (globals only). On every other lane,
//! the footer is two rows tall (globals + per-tab). The
//! 1-row ↔ 2-row transition must follow `Lane::Path` switches.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane};
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
fn path_lane_footer_is_one_row_tall() {
    // M199 S4: when the active lane is `Path`, the per-tab
    // string is empty, so `compute_view` collapses
    // `footer_area.height` to 1. `render_footer` then renders
    // only the globals row.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let view = view_state::compute_view(&app, area);
    assert_eq!(
        view.footer_area.height, 1,
        "Path must reserve a 1-row footer_area; got height={}",
        view.footer_area.height
    );

    let buf = render_to_buffer(&app, 80, 24);
    let globals_y = view.footer_area.y;
    let globals = row_text(&buf, globals_y);
    assert!(
        globals.contains(":quit") && globals.contains(":help") && globals.contains(":refresh"),
        "globals row must still surface the six tokens; got: {globals:?}"
    );
}

#[test]
fn non_path_lane_footer_is_two_rows_tall() {
    // M199: every non-Path lane (Milestones, Backlog, etc.)
    // produces a non-empty per-tab string, so the footer is
    // 2 rows tall.
    for lane in [
        Lane::Overview,
        Lane::Milestones,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Watch,
        Lane::Settings,
    ] {
        let mut app = App::new();
        if lane == Lane::Settings {
            app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
        }
        app.select_lane(lane);
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let view = view_state::compute_view(&app, area);
        // Watch is a 1-row footer in v1 because it has no
        // per-tab keys (per D-07 / `footer_per_tab` returns
        // empty for Watch). The other non-Path lanes are 2-row.
        if lane == Lane::Watch {
            assert_eq!(
                view.footer_area.height, 1,
                "{lane:?} (Watch) must reserve a 1-row footer_area"
            );
        } else {
            assert_eq!(
                view.footer_area.height, 2,
                "{lane:?} must reserve a 2-row footer_area"
            );
        }
    }
}

#[test]
fn switching_lane_changes_footer_height() {
    // M199 S4 pin: the 1-row ↔ 2-row transition follows the
    // active lane. Start on Milestones (2-row), switch to
    // Path (1-row), switch back to Milestones (2-row).
    let mut app = App::new();
    app.load_milestones(vec![]);
    app.select_lane(Lane::Milestones);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let v1 = view_state::compute_view(&app, area);
    assert_eq!(v1.footer_area.height, 2, "Milestones footer must be 2-row");

    app.select_lane(Lane::Path);
    let v2 = view_state::compute_view(&app, area);
    assert_eq!(v2.footer_area.height, 1, "Path footer must be 1-row");

    app.select_lane(Lane::Backlog);
    let v3 = view_state::compute_view(&app, area);
    assert_eq!(v3.footer_area.height, 2, "Backlog footer must be 2-row");
}

#[test]
fn footer_per_tab_path_returns_empty_string() {
    // Pin the source-of-truth: `Keybinds::footer_per_tab` for
    // every (Lane::Path, *) pair returns the empty string,
    // which is what `compute_view` consults to decide on the
    // 1-row footer.
    let app = App::new();
    for content in [
        ContentState::List,
        ContentState::MilestoneDetail,
        ContentState::BacklogDetail,
        ContentState::AnnotationThread,
        ContentState::CoApproval,
    ] {
        let s = app
            .keybinds
            .footer_per_tab(Lane::Path, content, false, false);
        assert!(
            s.is_empty(),
            "footer_per_tab(Path, {content:?}) must be empty; got={s:?}"
        );
    }
}

#[test]
fn path_lane_render_does_not_contain_per_tab_tokens() {
    // Render Path and inspect the row immediately above the
    // footer; that row must not be a per-tab line (it should
    // be content, not a keybind legend). This guards against
    // a regression where the 2-row layout is restored on Path
    // and starts bleeding the per-tab line into the content
    // area.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    let area = ratatui::layout::Rect::new(0, 0, 100, 24);
    let buf = render_to_buffer(&app, 100, 24);
    let view = view_state::compute_view(&app, area);
    let globals_y = view.footer_area.y;
    let above = if globals_y > 0 {
        row_text(&buf, globals_y - 1)
    } else {
        String::new()
    };
    // The row above the footer on Path is content (the path
    // view). It must not contain a per-tab legend token like
    // `:filter`, `:search`, `:annotate`, etc.
    for forbidden in [
        ":filter",
        ":search",
        ":hide-done",
        ":sort",
        ":cycle",
        ":annotate",
        ":menu",
    ] {
        assert!(
            !above.contains(forbidden),
            "row above the 1-row Path footer must not contain per-tab legend {forbidden:?}; got: {above:?}"
        );
    }
}
