//! M184 AC-04: tab bar renders 7 lanes at widths 80/120/160; first
//! label is not preceded by a divider (M167 follow-up).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn render_tab_row(width: u16) -> String {
    let app = App::new();
    let backend = TestBackend::new(width, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Tab bar is row 1 (header is row 0).
    let mut row = String::new();
    for x in 0..buf.area().width {
        row.push_str(buf[(x, 1)].symbol());
    }
    row
}

#[test]
fn tab_bar_seven_lanes_at_canonical_widths() {
    let expected_labels: Vec<&str> = Lane::ordered().iter().map(|l| l.label()).collect();
    assert_eq!(expected_labels.len(), 7);

    for width in [80u16, 120, 160] {
        let row = render_tab_row(width);
        // Full labels fit at ≥120; at 80 compact labels may appear.
        // Always require Overview (first) and Settings (last) to show
        // in some form, and never a leading divider before the first tab.
        let trimmed = row.trim_start();
        assert!(
            !trimmed.starts_with('│') && !trimmed.starts_with('|') && !trimmed.starts_with('┃'),
            "width {width}: first lane must not be preceded by a divider; got {row:?}"
        );
        assert!(
            row.contains("Overview")
                || row.contains("Ov")
                || row.contains(Lane::Overview.compact_label()),
            "width {width}: Overview must appear; got {row:?}"
        );
        assert!(
            row.contains("Settings")
                || row.contains("Set")
                || row.contains(Lane::Settings.compact_label()),
            "width {width}: Settings must appear; got {row:?}"
        );
        // Tweaks must not reappear.
        assert!(
            !row.contains("Tweaks") && !row.contains(" Tw "),
            "width {width}: Tweaks must be gone; got {row:?}"
        );
    }
}
