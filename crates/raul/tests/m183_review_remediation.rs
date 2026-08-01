//! M183 external-review regressions (F-02..F-05).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn row(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    let mut s = String::new();
    for x in 0..buf.area().width {
        s.push_str(buf[(x, y)].symbol());
    }
    s
}

fn draw(app: &App, w: u16, h: u16) -> (ratatui::buffer::Buffer, view_state::ViewState) {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut vs = None;
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
            vs = Some(view);
        })
        .unwrap();
    (terminal.backend().buffer().clone(), vs.unwrap())
}

fn has_box_drawing(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '─' | '│' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼'
        )
    })
}

/// F-02: compact Overview lower panels must stay inside content_area
/// (never land on footer_area).
#[test]
fn m183_f02_overview_lower_panels_stay_in_content() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let view = view_state::compute_view(&app, area);
    let chunks = view
        .dashboard_chunks
        .as_ref()
        .expect("overview must populate dashboard_chunks");
    let content = view.content_area;
    let footer = view.footer_area;
    let content_bottom = content.y.saturating_add(content.height);
    let footer_top = footer.y;

    assert!(
        chunks.lower_inbox.y >= content.y,
        "inbox y above content: {:?}",
        chunks.lower_inbox
    );
    assert!(
        chunks
            .lower_inbox
            .y
            .saturating_add(chunks.lower_inbox.height)
            <= content_bottom,
        "inbox extends past content into footer: inbox={:?} content={:?} footer={:?}",
        chunks.lower_inbox,
        content,
        footer
    );
    assert!(
        chunks
            .lower_inbox
            .y
            .saturating_add(chunks.lower_inbox.height)
            <= footer_top
            || chunks.lower_inbox.height == 0,
        "inbox overlaps footer_top={footer_top}: {:?}",
        chunks.lower_inbox
    );
    if let Some(act) = chunks.lower_activity {
        assert!(
            act.y.saturating_add(act.height) <= content_bottom,
            "activity extends past content: act={act:?} content={content:?}"
        );
    }
}

/// F-02 + F-03: short flash on Overview must not leave dashboard
/// box-drawing on either footer row.
#[test]
fn m183_f02_f03_flash_footer_has_no_box_drawing() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    app.set_flash_message("Saved 3 setting(s)");
    let (buf, view) = draw(&app, 80, 24);
    let y0 = row(&buf, view.footer_area.y);
    let y1 = row(&buf, view.footer_area.y + 1);
    assert!(
        !has_box_drawing(&y0),
        "flash footer line 0 must not retain dashboard chrome; got {y0:?}"
    );
    assert!(
        !has_box_drawing(&y1),
        "flash footer line 1 must not retain dashboard chrome; got {y1:?}"
    );
    assert!(
        y0.contains("Saved 3 setting(s)"),
        "flash text must be visible; got {y0:?}"
    );
}

/// F-04: compact flag on DashboardChunks matches content-area predicate
/// and stays true on a 32-row frame (content = 28 after 2-line footer).
#[test]
fn m183_f04_compact_flag_follows_content_height() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);

    let view24 = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 100, 24));
    let c24 = view24.dashboard_chunks.as_ref().unwrap();
    assert!(
        c24.compact,
        "24-row frame → content < 32 → compact; content_h={}",
        view24.content_area.height
    );
    assert!(view24.content_area.height < view_state::DASHBOARD_COMPACT_CONTENT_HEIGHT);

    let view32 = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 100, 32));
    let c32 = view32.dashboard_chunks.as_ref().unwrap();
    // frame 32 → content 28 (header+tab+2-footer) → still compact
    assert!(
        c32.compact,
        "32-row frame content_h={} must still be compact after M183 2-line footer",
        view32.content_area.height
    );

    // Tall enough that content_area.height >= 32
    let view40 = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 100, 40));
    let c40 = view40.dashboard_chunks.as_ref().unwrap();
    assert!(
        !c40.compact,
        "40-row frame content_h={} should be full density",
        view40.content_area.height
    );
}

/// F-05: cleared keybind slots must not emit leading-colon tokens.
#[test]
fn m183_f05_empty_keybind_skips_colon_token() {
    let mut app = App::new();
    app.keybinds.quit.clear();
    app.keybinds.help.clear();
    let (buf, view) = draw(&app, 140, 24);
    // M187: globals moved to the bottom row of footer_area.
    let globals = row(&buf, view.footer_area.y + 1);
    assert!(
        !globals.contains(":quit"),
        "cleared quit must not render ':quit'; got {globals:?}"
    );
    assert!(
        !globals.contains(":help"),
        "cleared help must not render ':help'; got {globals:?}"
    );
    // Remaining live keys still present.
    assert!(
        globals.contains(":refresh") || globals.contains("refresh"),
        "refresh should remain; got {globals:?}"
    );
}
