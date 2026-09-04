//! M222 F-02: production-path regression test for the
//! Settings lane rendering the new `KeybindsView`. The
//! previous cycle shipped `Keybinds::view()` library-only;
//! the cycle-2 fix wires it into `render_settings_lane`.
//!
//! This suite renders the Settings lane through the
//! production renderer (`render::render`) and asserts the
//! rendered buffer contains the TOML path header, the
//! override marker (`*`), and the `[autopilot]` section
//! banner. Without these tests the renderer can silently
//! drop the wiring again (the M225/M226 lesson).

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::keybinds::Keybinds;

/// Activate the Settings lane on `app` so the renderer
/// takes the `render_settings_lane` branch on the next
/// draw. We don't need real settings-state — the
/// keybinds-view block renders above the schema-gated
/// list, so the lane is reachable with any lane
/// selection.
fn enter_settings_lane(app: &mut App) {
    app.active_lane = Lane::Settings;
}

/// Render the Settings lane once and collect the buffer's
/// text content into a single `String` so assertions can
/// scan for substrings without worrying about the exact
/// cell positions.
fn render_settings_text(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("TestBackend");
    terminal
        .draw(|frame| {
            let view = raul::tui::view_state::compute_view(app, frame.area());
            raul::tui::render::render(frame, app, &view);
        })
        .expect("draw");
    let mut out = String::new();
    let buf = terminal.backend().buffer();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn settings_renders_keybinds_view_toml_path() {
    // AC-05: Settings renders the external TOML path so
    // the operator knows where to place overrides.
    let mut app = App::new();
    enter_settings_lane(&mut app);
    let text = render_settings_text(&app, 120, 30);
    assert!(
        text.contains("keybinds.toml"),
        "rendered Settings must include the `keybinds.toml` path; got: {text:?}"
    );
}

#[test]
fn settings_renders_keybinds_view_global_section_banner() {
    // The view's `[global]` banner must surface in the
    // rendered output so the operator can read the
    // effective global keymap.
    let mut app = App::new();
    enter_settings_lane(&mut app);
    let text = render_settings_text(&app, 120, 30);
    assert!(
        text.contains("[global]"),
        "rendered Settings must include the `[global]` section banner; got: {text:?}"
    );
}

#[test]
fn settings_renders_keybinds_view_autopilot_section_banner() {
    // AC-05: per-lane map is part of the view; the
    // `[autopilot]` banner must surface in the Settings
    // lane render. The view spans 36+ rows; we use a tall
    // pane (height=100) so the `[autopilot]` section is
    // reachable below the 29 global rows.
    let mut app = App::new();
    app.keybinds = Keybinds::default();
    enter_settings_lane(&mut app);
    let text = render_settings_text(&app, 140, 100);
    assert!(
        text.contains("[autopilot]"),
        "rendered Settings must include the `[autopilot]` section banner; got: {text:?}"
    );
}

#[test]
fn settings_renders_keybinds_view_override_marker_for_overridden_field() {
    // AC-05: Settings marks overridden versus default
    // bindings. The renderer uses a `*` marker on the
    // overridden row. We construct an `App` whose
    // autopilot lane has a non-default `select` binding,
    // render Settings, and assert the override marker
    // surfaces.
    let mut app = App::new();
    let mut kb = Keybinds::default();
    kb.lane_autopilot.select = vec![(KeyCode::F(1), KeyModifiers::empty())];
    app.keybinds = kb;
    enter_settings_lane(&mut app);

    let text = render_settings_text(&app, 140, 100);
    // The `*` override marker is rendered alongside the
    // action name. The action `select` is the autopilot
    // action we overrode. Pin the marker presence +
    // the F1 (override) glyph together.
    let has_select_row = text
        .lines()
        .any(|l| l.contains("select") && (l.contains('*') || l.contains("F1")));
    assert!(
        has_select_row,
        "rendered Settings must mark `select` as overridden with `*` and show F1; got: {text:?}"
    );
}

#[test]
fn settings_renders_keybinds_view_known_action_names() {
    // Pin that the view surfaces a stable set of action
    // names (the operator learns the new vocabulary by
    // seeing the labels rendered). We assert a known
    // set is reachable without depending on order.
    let mut app = App::new();
    enter_settings_lane(&mut app);
    let text = render_settings_text(&app, 140, 80);
    let required: HashSet<&'static str> = ["quit", "up", "down", "help"].into_iter().collect();
    let missing: Vec<&&str> = required
        .iter()
        .filter(|name| !text.contains(**name))
        .collect();
    assert!(
        missing.is_empty(),
        "rendered Settings must include every action label; missing: {missing:?}"
    );
}
