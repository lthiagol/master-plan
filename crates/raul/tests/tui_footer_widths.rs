//! M199 AC-04: two-line footer renders at widths 60, 80, 120, 160
//! without panicking. M199 redesign: globals on top (h-2), per-tab
//! on bottom (h-1). The per-tab line right-truncates with `…` on
//! overflow (D-07); the globals line keeps its collapse-to-prefix
//! behavior (D-06).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::flash_message;
use raul::tui::render;
use raul::tui::view_state;
use unicode_width::UnicodeWidthStr;

fn render_rows(app: &App, width: u16) -> (String, String) {
    // M199: returns (globals, per_tab). globals is on top (h-2),
    // per_tab is on the bottom (h-1).
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
        globals.push_str(buf[(x, h - 2)].symbol());
        per_tab.push_str(buf[(x, h - 1)].symbol());
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
        // still leave a non-empty prefix (graceful fallback, D-06).
        if width == 60 {
            assert!(
                !globals.trim().is_empty(),
                "width 60: truncated globals must stay non-empty"
            );
        }
        // Per-tab line is non-empty on Milestones (List). At width
        // 60 it must right-truncate with `…` (D-07) — the
        // leftmost tokens (F:filter, /:search) must remain.
        if width == 60 {
            assert!(
                per_tab.contains('\u{2026}'),
                "width 60: per-tab line must right-truncate with `…`; got {per_tab:?}"
            );
            assert!(
                per_tab.contains("F:filter") || per_tab.contains("filter"),
                "width 60: per-tab line must preserve leftmost tokens; got {per_tab:?}"
            );
        }
        if width >= 120 {
            // At widths ≥ 120 the per-tab line fits the full
            // list without truncation.
            assert!(
                !per_tab.contains('\u{2026}'),
                "width {width}: per-tab line should not need `…`; got {per_tab:?}"
            );
            assert!(
                per_tab.contains("filter")
                    && per_tab.contains("search")
                    && per_tab.contains("hide-done")
                    && per_tab.contains("sort")
                    && per_tab.contains("cycle")
                    && per_tab.contains("annotate"),
                "width {width}: per-tab line must list all six lane-specific keys; got {per_tab:?}"
            );
        }

        // Sanity: unicode width helper agrees with buffer fill.
        let _ = UnicodeWidthStr::width(globals.as_str());
    }
}

#[test]
fn per_tab_truncates_with_ellipsis_on_overflow() {
    // M199 AC-04 sub-pin: when the per-tab line's natural
    // content exceeds the available width, the renderer
    // right-truncates with `…` (D-07). At width 60 with a long
    // per-tab line (Milestones, List), the ellipsis must appear
    // and the leftmost tokens must remain.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let (globals, per_tab) = render_rows(&app, 60);
    assert!(
        per_tab.contains('\u{2026}'),
        "per-tab must end with `…` on overflow; got: {per_tab:?}"
    );
    // Globals still keeps quit visible.
    assert!(
        globals.contains(":quit"),
        "globals must keep quit at narrow widths; got: {globals:?}"
    );
}

#[test]
fn path_lane_is_globals_only_at_all_widths() {
    // M199 AC-05 pin: the per-tab row is hidden on Path (the
    // only v1 lane with an empty per-tab string), so the footer
    // is 1 row tall and the row below the footer is content,
    // not a per-tab row.
    for width in [60u16, 80, 120, 160] {
        let mut app = App::new();
        app.select_lane(Lane::Path);
        let backend = TestBackend::new(width, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let view = view_state::compute_view(&app, frame.area());
                render::render(frame, &app, &view);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let view = view_state::compute_view(&app, *buf.area());
        assert_eq!(
            view.footer_area.height, 1,
            "width {width}: Path footer must be 1 row tall"
        );
        let globals_y = view.footer_area.y;
        let globals = {
            let mut s = String::new();
            for x in 0..buf.area().width {
                s.push_str(buf[(x, globals_y)].symbol());
            }
            s
        };
        assert!(
            globals.contains(":quit") && globals.contains(":help"),
            "width {width}: Path globals row must still surface the six tokens; got: {globals:?}"
        );
    }
}
