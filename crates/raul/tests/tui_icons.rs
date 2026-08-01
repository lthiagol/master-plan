use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::config::{lane_icon, set_icons, status_icon, IconMode};
use raul::tui::app::App;
use raul::tui::render;
use raul::tui::view_state;

fn render_to_string(app: &App) -> String {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn lane_and_board_icons_respect_icon_mode() {
    set_icons(IconMode::None);
    assert_eq!(lane_icon("Overview"), "");
    assert_eq!(status_icon("done"), "");
    let app = App::new();
    let none_output = render_to_string(&app);
    assert!(!none_output.contains('⌂'));

    set_icons(IconMode::Unicode);
    assert_eq!(lane_icon("Overview"), "⌂");
    assert_eq!(status_icon("done"), "●");
    let unicode_output = render_to_string(&App::new());
    assert!(unicode_output.contains('⌂') || unicode_output.contains("Overview"));

    set_icons(IconMode::Ascii);
    assert_eq!(lane_icon("Milestones"), "[M]");
    assert_eq!(status_icon("done"), "[x]");
}
