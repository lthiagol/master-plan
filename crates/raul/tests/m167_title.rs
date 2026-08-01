//! M167 WP1 S1 / AC-08: window title carries "Understand Layers" (not
//! "Understand Lanes"). Driven through the same `render::render` path
//! the rest of the suite uses, so this is a regression against any
//! future title-format change.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::App;
use raul::tui::render;
use raul::tui::view_state;

#[test]
fn title_contains_understand_layers() {
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
        header.contains("Understand Layers"),
        "header must contain 'Understand Layers'; got {header:?}"
    );
    assert!(
        !header.contains("Understand Lanes"),
        "header must not contain the pre-M167 'Understand Lanes'; got {header:?}"
    );
}
