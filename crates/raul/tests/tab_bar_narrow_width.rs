//! M115 AC-01 / AC-02: tab bar narrow-width budget (integration tests).
//!
//! Regression test for the M105 code-review entry 3 sub-4 budget
//! underflow: the renderer must never emit more columns than the
//! requested `area.width` once `width < 6`. Pre-M115 the overflow
//! path reserved `left_ind + right_ind = 6` columns up-front for
//! the ◂ / ▸ indicators, which meant any `width <= 5` produced a
//! fragmented / overflowing bar — exactly the bug the M105 reviewer
//! flagged.
//!
//! Design (per M115): at `area.width in 1..=5`, the function drops
//! ALL indicators/ellipses and emits only the active lane (or
//! nothing if the bar cannot fit the active lane glyph); at
//! `area.width >= 6`, M105/S6/M91 behavior is preserved.
//!
//! M135 (S4): the pure layout tests (which exercised the layout
//! internals directly) moved into `view_state::tests` so the
//! layout machinery can be `pub(super)`. The integration tests
//! here drive the layout through the public
//! `view_state::compute_view` entry point and inspect the
//! resulting `view.tab_layout` and the rendered buffer.

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn render_to_buffer(width: u16, height: u16, active: Lane) -> String {
    let mut app = App::new();
    app.select_lane(active);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    // Only look at the tab-bar row (row 1, since row 0 is the header).
    let mut out = String::new();
    for x in 0..buffer.area().width {
        out.push_str(buffer[(x, 1)].symbol());
    }
    out
}

#[test]
fn render_narrow_width_buffer_does_not_overflow() {
    // End-to-end: render the actual TUI buffer at narrow widths and
    // assert nothing escapes the area. The buffer is the surface
    // where the M105 bug underflow actually surfaced.
    for w in 1u16..=6 {
        let bar = render_to_buffer(w, 6, Lane::Overview);
        assert_eq!(
            bar.chars().count(),
            w as usize,
            "rendered bar length must equal width={w}; got bar={bar:?} (len={})",
            bar.chars().count()
        );
    }
}

/// M115 review F-1: pin the layout/render contract at the
/// empty-visible path. At width=1 with Overview active,
/// `view_state::compute_view` populates `view.tab_layout` with
/// `visible: []` (nothing fits). `render_tab_bar` must honor that
/// and emit an empty-ish bar — NOT fall through to the wide branch
/// and render all 7 lanes. Pre-fix the wide branch emitted ~90
/// cols into a 1-col area; ratatui clipped the buffer so the
/// existing buffer-length test passed, but the rendered spans
/// diverged from `layout.visible`. This test counts distinct
/// lane labels in the rendered output to catch that divergence.
#[test]
fn render_at_width_1_does_not_emit_all_lanes() {
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    let view = view_state::compute_view(&app, Rect::new(0, 0, 1, 6));
    assert!(
        view.tab_layout.visible.is_empty(),
        "width=1 must yield empty visible"
    );
    let bar = render_to_buffer(1, 6, Lane::Overview);
    for label in ["Ov", "Ml", "Path", "BF", "TW", "BG", "Bd"] {
        assert!(
            !bar.contains(label),
            "width=1 bar must not contain lane label {label:?} (layout.visible is empty); bar={bar:?}"
        );
    }
}
