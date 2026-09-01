use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn render_sources() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("render");
    let mut out = String::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            out.push_str(&fs::read_to_string(&path).unwrap());
            out.push('\n');
        }
    }
    out
}

#[test]
fn render_uses_palette_not_hardcoded_colors() {
    let content = render_sources();

    let hardcoded_colors = [
        "Color::Cyan",
        "Color::Green",
        "Color::Yellow",
        "Color::Red",
        "Color::Blue",
        "Color::Magenta",
    ];

    let mut violations = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        if line.trim().starts_with("//") {
            continue;
        }
        for color in &hardcoded_colors {
            if line.contains(color) {
                violations.push(format!(
                    "render/*:{}: hardcoded color '{}' (use palette instead)",
                    line_no + 1,
                    color
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "render/ contains hardcoded colors (should use palette):\n{}",
        violations.join("\n")
    );
}

#[test]
fn render_allows_contrast_colors() {
    // M172 S4 (F-04): the contrast-color literal lives in
    // `palette.rs` (`on_accent_fg`, `caret_block`, `selection_border`)
    // and render/ consumes the helpers. Check both the palette
    // module (where the literal is allowed) and confirm render/
    // doesn't directly hit `Color::Black`/`Color::DarkGray`.
    let palette = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("tui")
            .join("palette.rs"),
    )
    .unwrap();
    assert!(
        palette.contains("Color::Black") || palette.contains("Color::DarkGray"),
        "palette.rs must expose contrast colors via helpers (the audit allows the literal here only)"
    );
    let render = render_sources();
    let direct_in_render = render.contains("Color::Black") || render.contains("Color::DarkGray");
    assert!(
        !direct_in_render,
        "render/ should NOT use Color::Black/Color::DarkGray directly — use palette::on_accent_fg / palette::caret_block / palette::selection_border"
    );
}

#[test]
fn app_has_palette_field() {
    let app_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("app.rs");
    let content = fs::read_to_string(&app_path).unwrap();

    assert!(
        content.contains("palette:") || content.contains("pub palette:"),
        "App struct must have a palette field"
    );
}

#[test]
fn run_tui_loads_ui_config() {
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = fs::read_to_string(&runner_path).unwrap();

    assert!(
        content.contains("ui_config") || content.contains("UiConfig"),
        "run_tui must load UiConfig"
    );
    assert!(
        content.contains("palette()") || content.contains(".palette"),
        "run_tui must extract palette from UiConfig"
    );
}

#[test]
fn run_tui_wires_color_and_icons() {
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = fs::read_to_string(&runner_path).unwrap();

    assert!(
        content.contains("set_color_enabled"),
        "run_tui must call set_color_enabled to honor --color flag"
    );
    assert!(
        content.contains("set_icons"),
        "run_tui must call set_icons to honor ui.icons config"
    );
}

#[test]
fn ac12_tui_honors_color_config() {
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = fs::read_to_string(&runner_path).unwrap();

    let run_tui_start = content
        .find("fn run_tui_inner")
        .expect("run_tui_inner must exist");
    let run_tui_section = &content[run_tui_start..];
    let next_fn = run_tui_section[1..]
        .find("\nfn ")
        .unwrap_or(run_tui_section.len());
    let run_tui_body = &run_tui_section[..next_fn];

    assert!(
        run_tui_body.contains("UiConfig::load"),
        "run_tui_inner must load UiConfig (AC-12)"
    );
    assert!(
        run_tui_body.contains("set_color_enabled"),
        "run_tui_inner must call set_color_enabled to honor --color off (AC-12)"
    );
    assert!(
        run_tui_body.contains("set_icons"),
        "run_tui_inner must call set_icons to honor ui.icons config (AC-12)"
    );
    assert!(
        run_tui_body.contains(".palette()") || run_tui_body.contains("palette()"),
        "run_tui_inner must extract and use palette from UiConfig (AC-12)"
    );
}

#[test]
fn tui_render_respects_color_enabled_toggle() {
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;
    use raul::config::set_color_enabled;
    use raul::theme::MOCHA;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.palette = &MOCHA;
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "complete".into(),
        lifecycle_at: Some("2026-07-04T00:00:00Z".into()),
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);

    let Color::Rgb(ar, ag, ab) = MOCHA.accent else {
        panic!("MOCHA accent must be RGB for this test");
    };

    set_color_enabled(true);
    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer_on = terminal.backend().buffer();
    assert!(
        buffer_contains_rgb(buffer_on, ar, ag, ab) || buffer_contains_rgb_bg(buffer_on, ar, ag, ab),
        "color on should render theme accent RGB in TUI buffer (fg or bg)"
    );

    set_color_enabled(false);
    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer_off = terminal.backend().buffer();
    assert!(
        !buffer_contains_rgb(buffer_off, ar, ag, ab)
            && !buffer_contains_rgb_bg(buffer_off, ar, ag, ab),
        "color off must not render MOCHA accent RGB in TUI buffer (AC-12)"
    );

    set_color_enabled(true);
}

fn buffer_contains_rgb(buffer: &ratatui::buffer::Buffer, r: u8, g: u8, b: u8) -> bool {
    use ratatui::style::Color;
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            if buffer[(x, y)].fg == Color::Rgb(r, g, b) {
                return true;
            }
        }
    }
    false
}

fn buffer_contains_rgb_bg(buffer: &ratatui::buffer::Buffer, r: u8, g: u8, b: u8) -> bool {
    use ratatui::style::Color;
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            if buffer[(x, y)].bg == Color::Rgb(r, g, b) {
                return true;
            }
        }
    }
    false
}
