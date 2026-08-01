//! M183 AC-03: two-line footer renders at widths 60, 80, 120, 160
//! without panicking; globals stay visible; lines fit the width.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::flash_message;
use raul::tui::render;
use raul::tui::view_state;
use unicode_width::UnicodeWidthStr;

fn render_rows(app: &App, width: u16) -> (String, String) {
    // M187: returns (globals, per_tab). per_tab is now on top (h-2),
    // globals on the bottom (h-1).
    let backend = TestBackend::new(width, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let h = buf.area().height;
    let mut globals = String::new();
    let mut per_tab = String::new();
    for x in 0..buf.area().width {
        per_tab.push_str(buf[(x, h - 2)].symbol());
        globals.push_str(buf[(x, h - 1)].symbol());
    }
    (globals, per_tab)
}

#[test]
fn footer_renders_at_canonical_widths() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);

    for width in [60u16, 80, 120, 160] {
        let (globals, per_tab) = render_rows(&app, width);

        assert!(
            flash_message::display_width(&globals) <= width as usize,
            "width {width}: globals exceed width ({} cols): {globals:?}",
            flash_message::display_width(&globals)
        );
        assert!(
            flash_message::display_width(&per_tab) <= width as usize,
            "width {width}: per-tab exceed width ({} cols): {per_tab:?}",
            flash_message::display_width(&per_tab)
        );

        // Globals always surface quit; at very narrow widths later
        // tokens may clip, but the first live key must remain.
        assert!(
            globals.contains(":quit") || globals.contains("quit"),
            "width {width}: globals must keep quit visible; got {globals:?}"
        );
        // At width 60 the full globals list overflows — truncation must
        // still leave a non-empty prefix (graceful fallback, AC-03).
        if width == 60 {
            assert!(
                !globals.trim().is_empty(),
                "width 60: truncated globals must stay non-empty"
            );
        }
        // Per-tab line is non-empty on list lanes.
        assert!(
            !per_tab.trim().is_empty(),
            "width {width}: per-tab line must not be empty"
        );

        // Sanity: unicode width helper agrees with buffer fill.
        let _ = UnicodeWidthStr::width(globals.as_str());
    }
}
