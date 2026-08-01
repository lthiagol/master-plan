//! M183 AC-02: on every lane, the per-tab footer line matches the
//! corresponding Keybinds::footer_* method (no behavior change to
//! per-tab keys — only the layout split).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::render;
use raul::tui::view_state;

fn render_footer_rows(app: &App) -> (String, String) {
    // M187: returns (globals, per_tab). After the footer flip, per_tab
    // is on the top row (closer to content) and globals is on the
    // bottom row.
    let backend = TestBackend::new(140, 30);
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

fn expected_per_tab(app: &App) -> String {
    let kb = &app.keybinds;
    match app.active_lane {
        Lane::Overview => kb.footer_overview(),
        Lane::Settings => Keybinds::footer_settings(app.settings.as_ref()),
        _ => kb.footer_list(),
    }
}

#[test]
fn per_tab_line_matches_footer_methods_on_all_lanes() {
    // M184: exactly the 7 ordered lanes.
    let lanes = [
        Lane::Overview,
        Lane::Milestones,
        Lane::Path,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Watch,
        Lane::Settings,
    ];

    for lane in lanes {
        let mut app = App::new();
        // Settings needs an open settings state for the Save/Cancel footer.
        if lane == Lane::Settings {
            app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
        }
        app.select_lane(lane.clone());
        let (_globals, per_tab) = render_footer_rows(&app);
        let expected = expected_per_tab(&app);
        let tokens: Vec<&str> = expected
            .split_whitespace()
            .filter(|t| t.contains(':') || t.starts_with('['))
            .collect();
        assert!(
            !tokens.is_empty(),
            "expected per-tab tokens for {lane:?}; raw={expected:?}"
        );
        for tok in tokens {
            assert!(
                per_tab.contains(tok),
                "lane {lane:?}: per-tab line missing {tok:?}; got={per_tab:?} expected={expected:?}"
            );
        }
    }
}
