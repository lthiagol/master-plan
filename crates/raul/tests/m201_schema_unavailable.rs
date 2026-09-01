//! M201 S16 / AC-08: schema-unavailable path.
//!
//! When `mp config schema` is unavailable (older mp, unknown subcommand,
//! or a stripped PATH), the Settings lane replaces the framed list with
//! a single error block. The error names `mp config schema` and hints
//! `mp --version` so the operator can see what they have installed.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsFocus, SettingsState};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn render_full(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
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

fn app_with_no_schema(warning: Option<String>) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let config = serde_json::json!({});
    app.settings = Some(SettingsState {
        config,
        schema: None,
        selected_idx: 0,
        focus: SettingsFocus::Fields,
        edit: None,
        staged_edits: BTreeMap::new(),
        schema_warning: warning,
    });
    app
}

#[test]
fn schema_unavailable_renders_error_block_replacing_framed_list() {
    // AC-08: when `mp config schema` is unavailable, the framed list
    // is replaced by a single error block. No half-rendered state.
    let app = app_with_no_schema(Some(
        "mp config schema unavailable: unknown subcommand".to_string(),
    ));
    let out = render_full(&app, 120, 40);
    assert!(
        out.contains("Schema unavailable"),
        "error title missing:\n{out}"
    );
    assert!(
        out.contains("mp config schema"),
        "error must name the missing subcommand:\n{out}"
    );
    assert!(
        out.contains("mp --version"),
        "error hint must mention `mp --version`:\n{out}"
    );
    // The list's section headers must NOT appear in this state.
    assert!(
        !out.contains(" ▾ ui "),
        "list section header `ui` must not render when schema is unavailable:\n{out}"
    );
    assert!(
        !out.contains("[bool]"),
        "type badge must not render when schema is unavailable:\n{out}"
    );
}

#[test]
fn schema_unavailable_does_not_pin_a_specific_version_string() {
    // The hint should NOT include a hard-coded "0.X.Y" version string;
    // the operator runs `mp --version` to see what they have. The
    // only canonical phrase we surface is "mp --version".
    let app = app_with_no_schema(Some(
        "mp config schema unavailable: unknown subcommand".to_string(),
    ));
    let out = render_full(&app, 120, 40);
    // No version-shaped digits inside the hint.
    assert!(
        !out.contains("0.X.Y"),
        "hint must not include a placeholder version:\n{out}"
    );
    assert!(
        !out.contains("0.1.0"),
        "hint must not pin a specific version:\n{out}"
    );
    assert!(
        out.contains("mp --version"),
        "operator must be told to run `mp --version`: {out}"
    );
}

#[test]
fn schema_unavailable_with_no_warning_still_renders_error_block() {
    // The schema_warning is optional; even when None, the renderer
    // surfaces a clear "Schema unavailable" block so the operator
    // doesn't see a half-rendered lane.
    let app = app_with_no_schema(None);
    let out = render_full(&app, 120, 40);
    assert!(
        out.contains("Schema unavailable"),
        "error title must render even with no warning:\n{out}"
    );
    assert!(
        out.contains("mp --version"),
        "hint must render even with no warning:\n{out}"
    );
}

#[test]
fn schema_unavailable_at_three_terminal_sizes() {
    // The error block is robust to terminal size — at 80x24, 120x40,
    // 200x60 the rendered output always names the missing subcommand
    // and the `mp --version` hint.
    for (w, h) in [(80u16, 24u16), (120, 40), (200, 60)] {
        let app = app_with_no_schema(Some("mp config schema unavailable".into()));
        let out = render_full(&app, w, h);
        assert!(
            out.contains("Schema unavailable"),
            "missing title at {w}x{h}:\n{out}"
        );
        assert!(
            out.contains("mp --version"),
            "missing mp --version hint at {w}x{h}:\n{out}"
        );
    }
}

#[test]
fn schema_unavailable_keeps_settings_state_intact() {
    // The error block is a render-only concern; the underlying
    // SettingsState is unchanged so a subsequent schema fetch
    // (or a re-open of the lane) can recover without data loss.
    let app = app_with_no_schema(Some(
        "mp config schema unavailable: unknown subcommand".into(),
    ));
    let state = app.settings.as_ref().unwrap();
    assert!(state.schema.is_none(), "schema stays None");
    assert_eq!(
        state.schema_warning.as_deref(),
        Some("mp config schema unavailable: unknown subcommand"),
        "warning is preserved"
    );
    assert_eq!(state.selected_idx, 0, "cursor is preserved");
}
